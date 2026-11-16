//! Port of `src/ezagent/voice/app.py` 285-383 (F4-03).
//!
//! Arranque de `whisper-server`: sondeo del endpoint, puerto local libre,
//! selección de motor (Vulkan o CPU) delegada en `scripts/start-whisper.ps1`
//! y espera acotada a que responda.

use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use tracing::{info, warn};

use ez_core::config::{python_str, truthy, VoiceConfig};

use crate::client::{whisper_home, whisper_model_path, UrlParts};

/// `wait_secs` por defecto de `ensure_whisper`.
pub const WHISPER_WAIT_SECS: f64 = 25.0;
/// `timeout` por defecto de `_whisper_ready`.
pub const WHISPER_READY_TIMEOUT: Duration = Duration::from_secs(1);

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

/// Nombres de carpeta donde puede estar `whisper-server.exe`.
const SERVER_SUBDIRS: [&str; 4] = ["", "whisper_vulkan", "vulkan", "cpu"];

fn has_server(home: Option<&Path>) -> bool {
    home.is_some_and(|home| {
        SERVER_SUBDIRS
            .iter()
            .any(|sub| home.join(sub).join("whisper-server.exe").is_file())
    })
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

/// Argumentos de `scripts/start-whisper.ps1`, en el orden del Python.
fn start_args(
    script: &Path,
    port: u16,
    engine: &str,
    home: Option<&Path>,
    model: Option<&Path>,
) -> Vec<String> {
    let mut args = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        script.to_string_lossy().into_owned(),
        "-Port".to_string(),
        port.to_string(),
        "-Engine".to_string(),
        engine.to_string(),
    ];
    if let Some(home) = home {
        args.push("-WhisperHome".to_string());
        args.push(home.to_string_lossy().into_owned());
    }
    if let Some(model) = model {
        args.push("-Model".to_string());
        args.push(model.to_string_lossy().into_owned());
    }
    args
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn spawn_server(
    script: &Path,
    port: u16,
    engine: &str,
    home: Option<&Path>,
    model: Option<&Path>,
) -> std::io::Result<Child> {
    // DECISION: se ejecuta el script del repo igual que el Python, sin shell
    // interactivo (`-File` y argumentos en lista); el script elige Vulkan/CPU.
    let mut command = Command::new("powershell");
    command
        .args(start_args(script, port, engine, home, model))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // DECISION: Python hereda el entorno; se copia explícitamente para que
        // PowerShell tenga PATH, LOCALAPPDATA, SystemRoot, etc.
        .env_clear()
        .envs(std::env::vars_os());
    if let Some(directory) = script.parent().and_then(Path::parent) {
        command.current_dir(directory);
    }
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

fn ensure_whisper_at(config: &mut VoiceConfig, wait_secs: f64, root: &Path) -> Option<Child> {
    let url = config_text(config, "whisper_url");
    if whisper_ready(&url) {
        return None;
    }
    if !config_truthy(config, "auto_start_whisper", true) {
        return None;
    }
    let parts = UrlParts::split(&url);
    let host = parts.host();
    if !is_local_host(host.as_deref()) {
        return None;
    }
    let script = root.join("scripts").join("start-whisper.ps1");
    if !script.is_file() {
        info!(
            "No encuentro {}; arranca whisper-server a mano.",
            script.display()
        );
        return None;
    }
    let home = whisper_home();
    let model = whisper_model_path(&config_text(config, "stt_model"));
    if !has_server(home.as_deref()) && model.is_none() {
        // DECISION: app.py usa log() propio (sin niveles); el aviso hablado lo
        // emite la capa que trata el estado "error" del HUD.
        info!("{}", missing_whisper_hint(packaged()));
        return None;
    }
    let Some(port) = free_local_port() else {
        warn!("No se pudo arrancar whisper-server: no hay puerto local libre");
        return None;
    };
    config.set("whisper_url", Value::String(url_for_local_port(&url, port)));
    info!("whisper-server: puerto local libre {port}.");
    info!("whisper-server no responde; arrancándolo…");
    let engine = {
        let configured = config_text(config, "stt_engine");
        if configured.is_empty() {
            "auto".to_string()
        } else {
            configured
        }
    };
    let process = match spawn_server(&script, port, &engine, home.as_deref(), model.as_deref()) {
        Ok(process) => process,
        Err(error) => {
            warn!("No se pudo arrancar whisper-server: {error}");
            return None;
        }
    };
    let updated = config_text(config, "whisper_url");
    if wait_until_ready(&updated, wait_secs) {
        info!("whisper-server listo.");
    } else {
        info!("whisper-server no respondió; revisa scripts\\start-whisper.ps1.");
    }
    Some(process)
}

/// Si whisper-server no responde, lo arranca y devuelve el proceso dueño.
pub fn ensure_whisper(config: &mut VoiceConfig) -> Option<Child> {
    ensure_whisper_with(config, WHISPER_WAIT_SECS)
}

/// Igual que [`ensure_whisper`], con una espera explícita en segundos.
pub fn ensure_whisper_with(config: &mut VoiceConfig, wait_secs: f64) -> Option<Child> {
    ensure_whisper_at(config, wait_secs, &resource_root())
}

#[cfg(test)]
mod tests;
