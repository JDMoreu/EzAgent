//! Port of `src/ezagent/voice/app.py` 285-383 (F4-03), con la corrección DX-3.
//!
//! Arranque de `whisper-server`: sondeo del endpoint, puerto local libre y
//! espera acotada a que responda. El ejecutable se lanza directo (sin
//! `powershell` intermedio) dentro de un Job Object que lo mata con la app, y
//! el puerto elegido se guarda en `%APPDATA%\EzAgent\whisper-server.json` para
//! reutilizar el servidor sano en el siguiente arranque.

mod job;

use std::fs;
use std::io;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use ez_core::config::{python_str, truthy, VoiceConfig};
use ez_core::dirs;

use crate::client::{whisper_home, whisper_model_path, UrlParts};

/// `wait_secs` por defecto de `ensure_whisper`.
pub const WHISPER_WAIT_SECS: f64 = 25.0;
/// `timeout` por defecto de `_whisper_ready`.
pub const WHISPER_READY_TIMEOUT: Duration = Duration::from_secs(1);
/// Tope de la consulta a `/health` del servidor guardado.
const WHISPER_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// Nombre del archivo de estado propio del servidor.
const STATE_FILE: &str = "whisper-server.json";
/// Nombre del ejecutable de whisper.cpp.
const SERVER_EXE: &str = "whisper-server.exe";
/// Modelo VAD que `start-whisper.ps1` busca junto al modelo elegido.
const VAD_MODEL: &str = "ggml-silero-v5.1.2.bin";
/// Modelos por omisión del script, en su orden.
const DEFAULT_MODELS: [&str; 2] = [
    "ggml-large-v3-turbo-q8_0.bin",
    "ggml-large-v3-turbo-q5_0.bin",
];

/// `_resource_root`: raíz de los recursos que acompañan al ejecutable.
///
/// En el empaquetado del Python es `sys._MEIPASS`; aquí es la carpeta del
/// ejecutable cuando el instalador copió `scripts/start-whisper.ps1` a su lado
/// (F8-01b). En desarrollo, la raíz del repo.
pub fn resource_root() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resource_root_in(exe_dir.as_deref())
}

/// La app corre instalada (recursos junto al ejecutable), no desde el repo.
fn packaged() -> bool {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    exe_dir
        .as_deref()
        .is_some_and(|dir| resource_root_in(Some(dir)) == dir)
}

/// Aviso de whisper ausente con la orden que sirve en cada caso.
// DECISION: el texto del Python manda a `uv run python -m …`, que no existe en
// la app instalada; ahí se da el subcomando del propio ejecutable.
pub fn missing_whisper_hint(packaged: bool) -> &'static str {
    if packaged {
        "No encuentro whisper. Descárgalo con: ezagent-voice downloads --all"
    } else {
        "No encuentro whisper. Descárgalo con: uv run python -m ezagent.core.downloads --all"
    }
}

/// `resource_root` con la carpeta del ejecutable recibida como parámetro.
// DECISION: el Python usa `sys._MEIPASS`; aquí la carpeta del ejecutable se
// reconoce por `scripts/start-whisper.ps1`, que es el recurso que necesita.
fn resource_root_in(exe_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = exe_dir {
        if dir.join("scripts").join("start-whisper.ps1").is_file() {
            return dir.to_path_buf();
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .map_or(manifest.clone(), Path::to_path_buf)
}

/// `_free_local_port`: puerto libre en `127.0.0.1`.
pub fn free_local_port() -> Option<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    Some(listener.local_addr().ok()?.port())
}

fn is_local_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"))
}

/// `_whisper_ready` con el timeout por defecto del Python.
pub fn whisper_ready(url: &str) -> bool {
    whisper_ready_timeout(url, WHISPER_READY_TIMEOUT)
}

/// `_whisper_ready`: ¿acepta conexiones el host/puerto de `url`?
pub fn whisper_ready_timeout(url: &str, timeout: Duration) -> bool {
    let parts = UrlParts::split(url);
    let Some(host) = parts.host() else {
        return false;
    };
    let port = parts.default_port();
    let Ok(addresses) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };
    for address in addresses {
        if TcpStream::connect_timeout(&address, timeout).is_ok() {
            return true;
        }
    }
    false
}

fn config_text(config: &VoiceConfig, key: &str) -> String {
    config
        .get(key)
        .filter(|value| !value.is_null())
        .map(python_str)
        .unwrap_or_default()
}

fn config_truthy(config: &VoiceConfig, key: &str, default: bool) -> bool {
    config.get(key).map_or(default, truthy)
}

/// `urlunsplit` con el host local y el puerto recién elegido.
fn url_for_local_port(url: &str, port: u16) -> String {
    let mut parts = UrlParts::split(url);
    if parts.scheme.is_empty() {
        parts.scheme = "http".to_string();
    }
    parts.netloc = format!("127.0.0.1:{port}");
    parts.unsplit()
}

/// `{esquema}://{host}/health` del servidor.
fn health_url(url: &str) -> String {
    let mut parts = UrlParts::split(url);
    if parts.scheme.is_empty() {
        parts.scheme = "http".to_string();
    }
    parts.path = "/health".to_string();
    parts.query.clear();
    parts.unsplit()
}

/// ¿El servidor guardado sigue sano? Se pregunta a `/health`; si el binario es
/// viejo y no tiene esa ruta, basta con que el puerto acepte conexiones.
fn server_healthy(url: &str) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(WHISPER_HEALTH_TIMEOUT)
        .build()
    else {
        return whisper_ready_timeout(url, WHISPER_READY_TIMEOUT);
    };
    match client.get(health_url(url)).send() {
        Ok(response) if response.status().is_success() => true,
        Ok(_) => whisper_ready_timeout(url, WHISPER_READY_TIMEOUT),
        Err(_) => false,
    }
}

/// Estado del servidor que arrancó esta app en un arranque anterior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ServerState {
    pid: u32,
    port: u16,
    url: String,
}

/// `%APPDATA%\EzAgent\whisper-server.json`.
fn state_path() -> PathBuf {
    dirs::app_data_dir().join(STATE_FILE)
}

fn read_state(path: &Path) -> Option<ServerState> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Escritura atómica: temporal en la misma carpeta y renombrar.
fn write_state(path: &Path, state: &ServerState) -> io::Result<()> {
    let text = serde_json::to_string(state).map_err(io::Error::other)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text)?;
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

fn remove_state(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Consulta al sistema por el proceso guardado; las pruebas lo sustituyen.
trait ProcessProbe {
    fn image_name(&self, pid: u32) -> Option<String>;
    fn terminate(&self, pid: u32) -> bool;
}

struct SystemProbe;

impl ProcessProbe for SystemProbe {
    fn image_name(&self, pid: u32) -> Option<String> {
        job::process_image_name(pid)
    }

    fn terminate(&self, pid: u32) -> bool {
        job::terminate(pid)
    }
}

fn is_whisper_server(image: Option<&str>) -> bool {
    image.is_some_and(|name| name.eq_ignore_ascii_case(SERVER_EXE))
}

/// Reutiliza el servidor sano del estado o retira el estado inservible.
///
/// Devuelve `true` cuando hay un servidor sano y deja su URL en `config`. Un
/// servidor nuestro que no responde se cierra solo si `replace` es `true`.
fn reuse_server(
    config: &mut VoiceConfig,
    state_file: &Path,
    probe: &dyn ProcessProbe,
    replace: bool,
) -> bool {
    let Some(state) = read_state(state_file) else {
        // Un estado ilegible también es nuestro: se descarta.
        remove_state(state_file);
        return false;
    };
    let ours = is_whisper_server(probe.image_name(state.pid).as_deref());
    if ours && server_healthy(&state.url) {
        config.set("whisper_url", Value::String(state.url.clone()));
        info!(
            "whisper-server: reutilizo el de la app anterior (PID {}, puerto {}).",
            state.pid, state.port
        );
        return true;
    }
    if ours && replace {
        warn!(
            "whisper-server (PID {}) no responde; lo cierro para arrancar otro.",
            state.pid
        );
        if !probe.terminate(state.pid) {
            warn!("No pude cerrar whisper-server (PID {}).", state.pid);
        }
    }
    if !ours || replace {
        remove_state(state_file);
    }
    false
}

/// Carpetas donde `start-whisper.ps1` busca `whisper-server.exe`, en su orden.
fn server_candidates(engine: &str) -> Vec<&'static str> {
    if engine.eq_ignore_ascii_case("cpu") {
        vec!["cpu", "", "whisper_vulkan", "vulkan"]
    } else {
        vec!["", "whisper_vulkan", "vulkan", "cpu"]
    }
}

/// Primer `whisper-server.exe` que existe en la carpeta de whisper.
fn resolve_server(home: Option<&Path>, engine: &str) -> Option<PathBuf> {
    let home = home?;
    server_candidates(engine)
        .into_iter()
        .map(|subdirectory| home.join(subdirectory).join(SERVER_EXE))
        .find(|path| path.is_file())
}

/// El modelo configurado o los que el script usa por omisión.
fn resolve_model(model: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(model) = model.filter(|path| path.is_file()) {
        return Some(model.to_path_buf());
    }
    let models = home?.join("models");
    DEFAULT_MODELS
        .iter()
        .map(|name| models.join(name))
        .find(|path| path.is_file())
}

/// Idioma e hilos que `start-whisper.ps1` usa por omisión (`-Language es`,
/// `-Threads 6`). `voice.json` no los define; el servidor de hoy corre así.
const WHISPER_LANGUAGE: &str = "es";
const WHISPER_THREADS: u32 = 6;

/// Argumentos que recibía `whisper-server` del script `start-whisper.ps1`.
fn server_args(port: u16, engine: &str, server: &Path, model: &Path) -> Vec<String> {
    let mut args = vec![
        "-m".to_string(),
        model.to_string_lossy().into_owned(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "-l".to_string(),
        WHISPER_LANGUAGE.to_string(),
        "-t".to_string(),
        WHISPER_THREADS.to_string(),
    ];
    let cpu = engine.eq_ignore_ascii_case("cpu")
        || server
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("cpu"));
    if cpu {
        args.push("-ng".to_string());
    }
    if let Some(vad) = model
        .parent()
        .map(|folder| folder.join(VAD_MODEL))
        .filter(|path| path.is_file())
    {
        args.push("--vad".to_string());
        args.push("--vad-model".to_string());
        args.push(vad.to_string_lossy().into_owned());
    }
    args
}

/// `stt_engine`, con el valor por omisión del script.
fn engine_name(config: &VoiceConfig) -> String {
    let configured = config_text(config, "stt_engine");
    if configured.is_empty() {
        "auto".to_string()
    } else {
        configured
    }
}

/// Ejecutable y argumentos que lanzaría [`ensure_whisper`], sin lanzarlos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// Ruta del `whisper-server.exe` elegido.
    pub executable: PathBuf,
    /// Argumentos en lista (sin shell); el puerto lo pone quien llama.
    pub args: Vec<String>,
}

/// Plan de arranque con la instalación y la configuración actuales.
///
/// No lanza nada ni toca la red: resuelve la carpeta de whisper, el modelo y el
/// VAD con las mismas reglas que [`ensure_whisper`]. `port` se recibe porque en
/// producción lo elige [`free_local_port`].
pub fn launch_plan(config: &VoiceConfig, port: u16) -> Option<LaunchPlan> {
    let home = whisper_home();
    let model = whisper_model_path(&config_text(config, "stt_model"));
    plan_for(config, home.as_deref(), model.as_deref(), port)
}

fn plan_for(
    config: &VoiceConfig,
    home: Option<&Path>,
    model: Option<&Path>,
    port: u16,
) -> Option<LaunchPlan> {
    let engine = engine_name(config);
    let executable = resolve_server(home, &engine)?;
    let model = resolve_model(model, home)?;
    Some(LaunchPlan {
        args: server_args(port, &engine, &executable, &model),
        executable,
    })
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// DECISION: se lanza el ejecutable directo, sin `powershell` intermedio; el
// script queda solo para arrancarlo a mano. El proceso es el que queda en el
// Job Object y el que se guarda en el estado.
fn spawn_server(server: &Path, args: &[String], cwd: &Path) -> io::Result<Child> {
    let mut command = Command::new(server);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

fn wait_until_ready(url: &str, wait_secs: f64) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(wait_secs.max(0.0));
    while Instant::now() < deadline {
        if whisper_ready(url) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

fn ensure_whisper_at(
    config: &mut VoiceConfig,
    wait_secs: f64,
    root: &Path,
    state_file: &Path,
) -> Option<Child> {
    let home = whisper_home();
    let model = whisper_model_path(&config_text(config, "stt_model"));
    ensure_whisper_in(
        config,
        wait_secs,
        root,
        state_file,
        home.as_deref(),
        model.as_deref(),
        &SystemProbe,
    )
}

/// Núcleo de [`ensure_whisper`] con la carpeta, el estado y la sonda inyectables.
fn ensure_whisper_in(
    config: &mut VoiceConfig,
    wait_secs: f64,
    root: &Path,
    state_file: &Path,
    home: Option<&Path>,
    model: Option<&Path>,
    probe: &dyn ProcessProbe,
) -> Option<Child> {
    let replace = config_truthy(config, "auto_start_whisper", true);
    if reuse_server(config, state_file, probe, replace) {
        return None;
    }
    let url = config_text(config, "whisper_url");
    if whisper_ready(&url) {
        return None;
    }
    if !replace {
        return None;
    }
    let parts = UrlParts::split(&url);
    let host = parts.host();
    if !is_local_host(host.as_deref()) {
        return None;
    }
    let Some(port) = free_local_port() else {
        warn!("No se pudo arrancar whisper-server: no hay puerto local libre");
        return None;
    };
    let Some(plan) = plan_for(config, home, model, port) else {
        if resolve_server(home, &engine_name(config)).is_none() {
            // DECISION: app.py usa log() propio (sin niveles); el aviso hablado
            // lo emite la capa que trata el estado "error" del HUD.
            info!("{}", missing_whisper_hint(packaged()));
        } else {
            info!("No encuentro el modelo de whisper; arranca whisper-server a mano.");
        }
        return None;
    };
    config.set("whisper_url", Value::String(url_for_local_port(&url, port)));
    let updated = config_text(config, "whisper_url");
    info!("whisper-server: puerto local libre {port}.");
    info!("whisper-server no responde; arrancándolo…");
    let process = match spawn_server(&plan.executable, &plan.args, root) {
        Ok(process) => process,
        Err(error) => {
            warn!("No se pudo arrancar whisper-server: {error}");
            return None;
        }
    };
    if !job::assign(&process) {
        warn!("No pude atar whisper-server al Job de la app; cerrarlo quedará a medias.");
    }
    let state = ServerState {
        pid: process.id(),
        port,
        url: updated.clone(),
    };
    if let Err(error) = write_state(state_file, &state) {
        warn!("No pude guardar el estado de whisper-server: {error}");
    }
    if wait_until_ready(&updated, wait_secs) {
        info!("whisper-server listo.");
    } else {
        info!("whisper-server no respondió; revisa los registros de whisper-server.");
    }
    Some(process)
}

/// Si whisper-server no responde, lo arranca y devuelve el proceso dueño.
pub fn ensure_whisper(config: &mut VoiceConfig) -> Option<Child> {
    ensure_whisper_with(config, WHISPER_WAIT_SECS)
}

/// Igual que [`ensure_whisper`], con una espera explícita en segundos.
pub fn ensure_whisper_with(config: &mut VoiceConfig, wait_secs: f64) -> Option<Child> {
    ensure_whisper_at(config, wait_secs, &resource_root(), &state_path())
}

#[cfg(test)]
mod tests;
