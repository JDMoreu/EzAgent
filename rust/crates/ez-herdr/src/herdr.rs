//! Port of `src/ezagent/voice/herdr.py` (F4-10).
//!
//! Adaptador entre `ezagent-voice` y Herdr (`hc` + `herdr-app`): listar y leer
//! terminales, enviar mensajes y teclas, abrir agentes y guardar contexto
//! largo. Herdr es un producto externo y se invoca como proceso hijo, sin
//! shell y con argumentos en lista.
//!
//! El Python mezcla `Herdr(AgentDriver(AgentSession))` por herencia. En Rust
//! `run_agent` recibe la parte de lanzamiento por el trait [`LaunchSupport`]
//! (F4-11 implementa `agent_driver.py`/`agent_session.py`); así este crate no
//! depende de `ez-agent` ni duplica el catálogo de agentes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use ez_core::config::{int_value, VoiceConfig};
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::{head_chars, py_strip, python_repr, python_str, text_or_empty, truthy};
use serde_json::{Map, Value};

mod actions;
mod handoff;
pub(crate) mod process;
mod runtime;
mod terminals;
mod text;

pub use actions::ReadyCallback;
pub use actions::RunAgent;
pub use runtime::{system as system_runtime, HerdrRuntime, SystemRuntime};
pub use text::{strip_ansi, visible_name};

pub use process::{Completed, RunError};

/// Instrucción que se antepone a la tarea cuando el agente trabaja en la GUI.
pub const GUI_INSTRUCTION: &str = "Trabaja de forma visible en el escritorio. Usa la skill cua-driver para controlar la interfaz gráfica: abre las aplicaciones, navega, haz clic y escribe como lo haría una persona. Deja las ventanas a la vista para que el usuario vea todo el proceso. ";

/// `hc send` recibe el texto como argumento y Windows limita la línea de
/// comandos a ~32 000 caracteres (un plan detallado mide más). Por encima de
/// este umbral el mensaje se entrega como archivo (`hc put`).
pub const LONG_TEXT_CHARS: usize = 12_000;

/// Shell por defecto de las terminales visibles.
pub const DEFAULT_SHELL: &str = "powershell.exe";

/// Carpeta de instalación por defecto y script oficial de cada pieza.
const HERDR_PARTS: [(&str, &str, &str); 2] = [
    ("hc_path", "herdr-core", "hc.exe"),
    ("herdr_app_path", "herdr-app", "herdr-app.exe"),
];
const HERDR_INSTALLERS: [(&str, &str); 2] = [
    (
        "herdr-core",
        "irm https://releases.herdr.dev/core/install.ps1 | iex",
    ),
    (
        "herdr-app",
        "irm https://releases.herdr.dev/app/install.ps1 | iex",
    ),
];

/// `HerdrError`: cualquier fallo del adaptador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HerdrError {
    Failed(String),
    /// `HerdrNotFound`: Herdr respondió que la terminal no existe.
    NotFound(String),
}

impl HerdrError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Failed(message) | Self::NotFound(message) => message,
        }
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

impl std::fmt::Display for HerdrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for HerdrError {}

/// Cómo abrir un agente y qué falta por fijar dentro de su sesión.
#[derive(Debug, Clone, Default)]
pub struct AgentLaunch {
    pub argv: Option<Vec<String>>,
    pub warnings: Vec<String>,
    /// Valores que el CLI no acepta como flag y se fijan con comandos de sesión.
    pub pending: Map<String, Value>,
    /// Variables de entorno que se fijan en la shell antes de lanzar el comando.
    pub env: Map<String, Value>,
    pub error: String,
}

/// Lo que `run_agent` necesita del driver de agentes (F4-11).
pub trait LaunchSupport {
    /// `AgentDriver.agent_launch`.
    fn agent_launch(&self, agent: &str, model: Option<&str>, effort: Option<&str>) -> AgentLaunch;

    /// `AgentSession.wait_shell_prompt`.
    fn wait_shell_prompt(&self, address: &str) -> bool;

    /// `AgentSession.finish_launch`; el mapa lleva la clave `ok` como Python.
    fn finish_launch(
        &self,
        address: &str,
        key: &str,
        launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value>;
}

impl<T: LaunchSupport + ?Sized> LaunchSupport for Arc<T> {
    fn agent_launch(&self, agent: &str, model: Option<&str>, effort: Option<&str>) -> AgentLaunch {
        (**self).agent_launch(agent, model, effort)
    }

    fn wait_shell_prompt(&self, address: &str) -> bool {
        (**self).wait_shell_prompt(address)
    }

    fn finish_launch(
        &self,
        address: &str,
        key: &str,
        launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        (**self).finish_launch(address, key, launch, task)
    }
}

#[derive(Default)]
struct ContextState {
    at: Option<f64>,
    pending: bool,
    snapshot: Option<Map<String, Value>>,
    agents_at: Option<f64>,
    agents_cache: Option<Vec<Map<String, Value>>>,
}

pub struct Herdr {
    cfg: RwLock<VoiceConfig>,
    install_lock: Mutex<()>,
    context: Mutex<ContextState>,
    runtime: Arc<dyn HerdrRuntime>,
}

impl Herdr {
    pub fn new(cfg: VoiceConfig) -> Self {
        Self::with_runtime(cfg, runtime::system())
    }

    pub fn with_runtime(cfg: VoiceConfig, runtime: Arc<dyn HerdrRuntime>) -> Self {
        let herdr = Self {
            cfg: RwLock::new(cfg),
            install_lock: Mutex::new(()),
            context: Mutex::new(ContextState::default()),
            runtime,
        };
        herdr.resolve_paths();
        herdr
    }

    pub fn config(&self) -> VoiceConfig {
        self.read_cfg().clone()
    }

    pub fn hc_path(&self) -> String {
        self.cfg_text("hc_path")
    }

    pub fn herdr_app_path(&self) -> String {
        self.cfg_text("herdr_app_path")
    }

    /// `_anonymous_env()`.
    pub fn anonymous_env() -> std::collections::HashMap<String, String> {
        process::anonymous_env()
    }

    /// Clave canónica de un agente a partir de cómo se dictó.
    pub fn resolve_agent(&self, name: &str) -> Option<String> {
        let cfg = self.read_cfg();
        let Some(Value::Object(agents)) = cfg.get("agents") else {
            return None;
        };
        let profiles = cfg.get("agent_profiles").and_then(Value::as_object);
        ez_core::config::resolve_agent_key(name, agents, profiles)
    }

    fn read_cfg(&self) -> std::sync::RwLockReadGuard<'_, VoiceConfig> {
        self.cfg
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn cfg_text(&self, key: &str) -> String {
        text_or_empty(self.read_cfg().get(key))
    }

    fn set_cfg_text(&self, key: &str, value: &str) {
        self.cfg
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set(key, Value::String(value.to_string()));
    }

    fn default_cwd(&self) -> String {
        self.cfg_text("default_cwd")
    }

    fn max_read_lines(&self) -> i64 {
        let cfg = self.read_cfg();
        int_value(cfg.get("max_read_lines").unwrap_or(&Value::Null), 40)
    }

    fn context(&self) -> std::sync::MutexGuard<'_, ContextState> {
        self.context
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ------------------------------------------------------------- instalación

    /// Usa el binario del PATH o el que deja el instalador de Herdr.
    pub fn resolve_paths(&self) {
        for (attr, folder, exe) in HERDR_PARTS {
            let current = self.cfg_text(attr);
            if let Some(found) = self.runtime.which(&current) {
                self.set_cfg_text(attr, &found.to_string_lossy());
                continue;
            }
            if !current.is_empty() && Path::new(&current).is_absolute() {
                continue; // ruta elegida por el usuario: no se sustituye
            }
            let candidate = programs_dir().join(folder).join(exe);
            if candidate.is_file() {
                self.set_cfg_text(attr, &candidate.to_string_lossy());
            }
        }
    }

    /// Instala Herdr y su motor (hc) si faltan. True si ambos están listos.
    ///
    /// Usa el instalador oficial y fijo de herdr.dev, sin texto del usuario ni
    /// del LLM. Si el binario ya existe no descarga nada.
    pub fn ensure_installed(&self) -> bool {
        let _guard = self
            .install_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.resolve_paths();
        for (attr, folder, _exe) in HERDR_PARTS {
            if self.runtime.which(&self.cfg_text(attr)).is_some() {
                continue;
            }
            tracing::info!("Herdr: falta {}; instalándolo…", folder);
            let script = HERDR_INSTALLERS
                .iter()
                .find(|(name, _)| *name == folder)
                .map(|(_, script)| *script)
                .unwrap_or("");
            if !self.run_installer(script) {
                tracing::warn!("Herdr: no pude instalar {}.", folder);
            }
            self.resolve_paths();
        }
        HERDR_PARTS
            .iter()
            .all(|(attr, _, _)| self.runtime.which(&self.cfg_text(attr)).is_some())
    }

    /// DECISION: `iex` sobre un script de URL fija es el instalador oficial de
    /// Herdr. Va como `-EncodedCommand` (base64 UTF-16LE): no usa shell ni
    /// interpola texto del usuario ni del modelo en la línea de comandos.
    pub fn run_installer(&self, script: &str) -> bool {
        let argv = powershell_script(&format!(
            "$ProgressPreference = 'SilentlyContinue'; {script}"
        ));
        match self.runtime.run(&argv, 900.0, false) {
            Ok(result) if result.returncode == 0 => true,
            Ok(result) => {
                tracing::warn!(
                    "Herdr: el instalador devolvió {}: {}",
                    result.returncode,
                    tail(&result.stderr, 400)
                );
                false
            }
            Err(error) => {
                tracing::warn!("Herdr: instalador: {}", error);
                false
            }
        }
    }

    // ------------------------------------------------------------------- base

    pub fn run_command(
        &self,
        argv: &[String],
        timeout: f64,
        anonymous: bool,
    ) -> Result<Completed, HerdrError> {
        self.spawn(argv, timeout, anonymous)
    }

    /// `spawn`/`_exec` del Python: ejecuta y, si falta el binario, reintenta con
    /// el que resolvió el instalador.
    pub fn spawn(
        &self,
        argv: &[String],
        timeout: f64,
        anonymous: bool,
    ) -> Result<Completed, HerdrError> {
        match self.runtime.run(argv, timeout, anonymous) {
            Ok(result) => Ok(result),
            Err(RunError::NotFound) => {
                let Some(replacement) = argv
                    .first()
                    .and_then(|program| self.replacement_for(program))
                else {
                    return Err(not_found_error(argv));
                };
                let mut retry = Vec::with_capacity(argv.len());
                retry.push(replacement);
                retry.extend_from_slice(argv.get(1..).unwrap_or(&[]));
                match self.runtime.run(&retry, timeout, anonymous) {
                    Ok(result) => Ok(result),
                    Err(RunError::NotFound) => Err(not_found_error(argv)),
                    Err(error) => Err(run_error(&retry, error)),
                }
            }
            Err(error) => Err(run_error(argv, error)),
        }
    }

    /// `_replacement_for`: binario ya instalado para `hc` o `herdr-app`.
    pub fn replacement_for(&self, executable: &str) -> Option<String> {
        let name = Path::new(executable)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        let attr = match name.as_str() {
            "hc" | "hc.exe" => "hc_path",
            "herdr-app" | "herdr-app.exe" => "herdr_app_path",
            _ => return None,
        };
        self.ensure_installed();
        self.runtime
            .which(&self.cfg_text(attr))
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn run(&self, args: &[String], timeout: f64) -> Result<Completed, HerdrError> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(self.hc_path());
        argv.extend_from_slice(args);
        self.run_command(&argv, timeout, false)
    }

    pub fn json_command(&self, argv: &[String], timeout: f64) -> Result<Value, HerdrError> {
        let result = self.run_command(argv, timeout, false)?;
        parse_json_output(&argv[0], &result)
    }

    fn json(&self, args: &[String], timeout: f64) -> Result<Value, HerdrError> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(self.hc_path());
        argv.extend_from_slice(args);
        self.json_command(&argv, timeout)
    }

    fn json_app(&self, args: &[String], timeout: f64) -> Result<Value, HerdrError> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(self.herdr_app_path());
        argv.extend_from_slice(args);
        self.json_command(&argv, timeout)
    }
}

fn not_found_error(argv: &[String]) -> HerdrError {
    HerdrError::failed(format!(
        "No encuentro `{}`. Revisa voice.json.",
        argv.first().map(String::as_str).unwrap_or("")
    ))
}

fn run_error(argv: &[String], error: RunError) -> HerdrError {
    match error {
        RunError::Timeout => HerdrError::failed(format!(
            "`{}` tardó demasiado.",
            argv.first().map(String::as_str).unwrap_or("")
        )),
        RunError::Os(message) => HerdrError::failed(message),
        RunError::NotFound => not_found_error(argv),
    }
}

/// Texto final de `_json_command`: salida vacía, JSON inválido o `error`.
fn parse_json_output(program: &str, result: &Completed) -> Result<Value, HerdrError> {
    let output = py_strip(&result.stdout).to_string();
    if output.is_empty() {
        let message = if result.stderr.is_empty() {
            format!("{program} no devolvió nada")
        } else {
            result.stderr.clone()
        };
        return Err(HerdrError::failed(py_strip(&message)));
    }
    let data = ez_core::pyloads::loads(&output).map_err(|_| {
        HerdrError::failed(format!(
            "{program} devolvió algo que no es JSON: {}",
            head_chars(&output, 300)
        ))
    })?;
    if let Value::Object(map) = &data {
        if let Some(error) = map.get("error").filter(|value| truthy(value)) {
            let message = match error {
                Value::Object(spec) => python_str(spec.get("message").unwrap_or(&Value::Null)),
                other => python_str(other),
            };
            if let Value::Object(spec) = error {
                if spec.get("kind").and_then(Value::as_str) == Some("not_found") {
                    return Err(HerdrError::not_found(message));
                }
            }
            return Err(HerdrError::failed(message));
        }
    }
    Ok(data)
}

fn programs_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base).join("Programs"),
        _ => ez_core::dirs::home()
            .join("AppData")
            .join("Local")
            .join("Programs"),
    }
}

/// `powershell.exe -NoProfile -Command <script>` del lanzamiento en segundo
/// plano (Python no pasa `-ExecutionPolicy` aquí), con el script en base64
/// para no interpolar texto en la línea de comandos.
fn powershell_command(script: &str) -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-EncodedCommand".to_string(),
        utf16le_base64(script),
    ]
}

/// `powershell.exe` con el script en base64 UTF-16LE (`-EncodedCommand`).
fn powershell_script(script: &str) -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-EncodedCommand".to_string(),
        utf16le_base64(script),
    ]
}

fn utf16le_base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(*chunk.get(1).unwrap_or(&0));
        let third = u32::from(*chunk.get(2).unwrap_or(&0));
        let packed = (first << 16) | (second << 8) | third;
        out.push(ALPHABET[((packed >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((packed >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((packed >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(packed & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn tail(text: &str, count: usize) -> String {
    ez_core::pyvalue::tail_chars(text, count).to_string()
}

/// `str(value or fallback)` de las pantallas.
pub(crate) fn value_or_text(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => fallback.to_string(),
    }
}

/// `f"{data}"` de Python (un dict se imprime como su `repr`).
pub(crate) fn python_text(value: &Value) -> String {
    python_repr(value)
}

/// `{"ok": False, "error": …}` de las acciones.
pub(crate) fn error_row(message: impl Into<String>) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("ok".to_string(), Value::Bool(false));
    row.insert("error".to_string(), Value::String(message.into()));
    row
}

/// `json.dumps(..., ensure_ascii=False)` de los bloques del prompt.
pub(crate) fn json_text(value: &Value) -> String {
    dumps_with(value, &DumpOptions::utf8())
}

#[cfg(test)]
mod tests;
