//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! Acciones, argumentos, callbacks y resúmenes auxiliares de las dos
//! herramientas de Herdr. Los textos se copian literalmente del Python.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use ez_core::config::{python_str, truthy};
use ez_core::pyvalue::{py_strip, text_or_empty};
use ez_herdr::herdr::HerdrError;
use serde_json::{Map, Value};

mod parameters;

pub use parameters::{delegate_parameters, voice_parameters};

pub const DELEGATE_ACTIONS: [&str; 13] = [
    "list",
    "get",
    "read",
    "send",
    "run",
    "open_terminal",
    "focus",
    "kill",
    "watch",
    "agents",
    "configure",
    "keys",
    "guide",
];

pub const VOICE_ACTIONS: [&str; 5] = [
    "close",
    "close_and_watch",
    "mic_mute",
    "repeat_last",
    "mute",
];

/// Mismas expresiones que usa el mensajero de voz actual (llm.py) para no
/// cambiar el comportamiento de «avísame» y «ciérrate».
const NOTIFY_WORDS: [&str; 34] = [
    "avísame",
    "avisame",
    "avisa",
    "avísame cuando",
    "me avisas",
    "me avise",
    "me avises",
    "notifícame",
    "notificame",
    "notifica",
    "cuando termine",
    "cuando acabe",
    "cuando hayas terminado",
    "cuando esté",
    "cuando este",
    "al terminar",
    "al acabar",
    "estate atento",
    "estate pendiente",
    "esté atento",
    "este atento",
    "vigila",
    "vigílame",
    "vigilame",
    "pendiente de",
    "me dice",
    "me digas",
    "me dices",
    "dime",
    "me pasas",
    "me cuentas",
    "resumen",
    "resúmeme",
    "resumeme",
];

const DELIVER_WORDS: [&str; 11] = [
    "respuesta",
    "respóndeme",
    "respondeme",
    "contéstame",
    "contestame",
    "dime",
    "me dices",
    "me dice",
    "cuéntame",
    "cuentame",
    "resultado",
];

const CLOSE_WORDS: [&str; 6] = [
    "ciérrate",
    "cierrate",
    "cierra la escucha",
    "cierra el micro",
    "deja de escuchar",
    "sin escuchar",
];

/// Nivel de riesgo de cada acción de `delegate_to_agent`.
pub fn delegate_level(action: &str) -> Option<i64> {
    Some(match action {
        "list" | "get" | "read" | "focus" | "watch" | "agents" | "guide" => 0,
        "send" | "run" | "open_terminal" | "configure" | "keys" => 1,
        "kill" => 2,
        _ => return None,
    })
}

pub type Hook = Arc<dyn Fn() + Send + Sync>;
pub type TextHook = Arc<dyn Fn(&str) + Send + Sync>;
pub type BoolHook = Arc<dyn Fn(bool) + Send + Sync>;
pub type CountHook = Arc<dyn Fn(i64) + Send + Sync>;
pub type TextFn = Arc<dyn Fn() -> String + Send + Sync>;
pub type TaskHook = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;
pub type SpecHook = Arc<dyn Fn(Value) + Send + Sync>;

fn no_text(_: &str) {}

fn no_bool(_: bool) {}

fn no_count(_: i64) {}

fn no_task(_: &str, _: &str, _: &str) {}

fn no_spec(_: Value) {}

/// Callbacks que conectan las herramientas con la aplicación de voz.
#[derive(Clone)]
pub struct HerdrCallbacks {
    pub watch: TextHook,
    pub request_close: Hook,
    pub mic_mute: BoolHook,
    pub last_summary: TextFn,
    pub mute: CountHook,
    pub get_last_user_text: TextFn,
    /// Persiste la tarea que el usuario quiere vigilar, para poder decir qué
    /// terminó y sobrevivir a un reinicio de EzAgent.
    pub remember_task: TaskHook,
    /// Si `input_from` sigue trabajando, la app guarda la cadena y lanza el
    /// agente destino cuando la fuente termine.
    pub enqueue_chain: SpecHook,
    /// Dice la respuesta del agente al terminar, sin preguntar antes.
    pub deliver_answer: TextHook,
    /// Abre en paralelo el agente del primer paso de un flujo (su arranque se
    /// esconde detrás del trabajo del paso anterior).
    pub prewarm_chain: SpecHook,
    /// True: run responde en cuanto la terminal está abierta y el resto del
    /// arranque (programa, modelo, tarea) ocurre aparte.
    pub async_launch: bool,
    pub launch_failed: TaskHook,
}

impl Default for HerdrCallbacks {
    fn default() -> Self {
        Self {
            watch: Arc::new(no_text),
            request_close: Arc::new(|| {}),
            mic_mute: Arc::new(no_bool),
            last_summary: Arc::new(String::new),
            mute: Arc::new(no_count),
            get_last_user_text: Arc::new(String::new),
            remember_task: Arc::new(no_task),
            enqueue_chain: Arc::new(no_spec),
            deliver_answer: Arc::new(no_text),
            prewarm_chain: Arc::new(no_spec),
            async_launch: false,
            launch_failed: Arc::new(no_task),
        }
    }
}

/// Estado compartido por las dos herramientas durante la sesión.
#[derive(Debug, Default)]
pub struct SessionState {
    pub last_address: Option<String>,
}

pub type SharedState = Arc<Mutex<SessionState>>;

pub fn shared_state() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}

pub fn last_address(state: &SharedState) -> Option<String> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .last_address
        .clone()
}

pub fn set_last_address(state: &SharedState, address: Option<String>) {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .last_address = address;
}

/// `_ArgumentError`: argumentos inválidos; se devuelven como `ToolResult(ok=False)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentError(pub String);

impl ArgumentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for ArgumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Fallo de un manejador: argumentos inválidos o Herdr.
#[derive(Debug)]
pub enum DelegateError {
    Argument(ArgumentError),
    Herdr(HerdrError),
}

impl From<ArgumentError> for DelegateError {
    fn from(error: ArgumentError) -> Self {
        Self::Argument(error)
    }
}

impl From<HerdrError> for DelegateError {
    fn from(error: HerdrError) -> Self {
        Self::Herdr(error)
    }
}

impl DelegateError {
    /// DECISION: en Rust no hay excepciones genéricas; la rama
    /// «Error inesperado…» del Python es inalcanzable y se omite.
    pub fn into_result(self) -> crate::registry::ToolResult {
        match self {
            Self::Argument(error) => crate::registry::ToolResult::err(error.0),
            Self::Herdr(error) => {
                crate::registry::ToolResult::err(format!("No he podido hablar con Herdr: {error}"))
            }
        }
    }
}

/// `_action`: texto de la acción, recortado y en minúsculas.
pub fn action(args: &Map<String, Value>) -> String {
    match args.get("action") {
        Some(Value::String(value)) => py_strip(value).to_lowercase(),
        _ => String::new(),
    }
}

/// `_checked_action`.
pub fn checked_action(
    args: &Map<String, Value>,
    valid: &[&str],
    tool: &str,
) -> Result<String, ArgumentError> {
    let action = action(args);
    if action.is_empty() {
        return Err(ArgumentError::new(format!(
            "Falta la acción de «{tool}». Acciones válidas: {}.",
            valid.join(", ")
        )));
    }
    if !valid.contains(&action.as_str()) {
        return Err(ArgumentError::new(format!(
            "Acción desconocida para «{tool}»: {}. Acciones válidas: {}.",
            ez_core::pyvalue::repr_str(&action),
            valid.join(", ")
        )));
    }
    Ok(action)
}

/// `_required_text`.
pub fn required_text(
    args: &Map<String, Value>,
    key: &str,
    action: &str,
) -> Result<String, ArgumentError> {
    match args.get(key) {
        Some(Value::String(value)) if !py_strip(value).is_empty() => {
            Ok(py_strip(value).to_string())
        }
        _ => Err(ArgumentError::new(format!(
            "Para la acción «{action}» falta el parámetro «{key}»."
        ))),
    }
}

/// `_optional_text`.
pub fn optional_text(
    args: &Map<String, Value>,
    key: &str,
    action: &str,
) -> Result<Option<String>, ArgumentError> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Value::String(text) = value else {
        return Err(ArgumentError::new(format!(
            "El parámetro «{key}» de la acción «{action}» debe ser texto."
        )));
    };
    let text = py_strip(text);
    Ok(if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    })
}

/// `_optional_bool`.
pub fn optional_bool(
    args: &Map<String, Value>,
    key: &str,
    action: &str,
) -> Result<bool, ArgumentError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(ArgumentError::new(format!(
            "El parámetro «{key}» de la acción «{action}» debe ser verdadero o falso."
        ))),
    }
}

/// `_optional_lines`.
pub fn optional_lines(
    args: &Map<String, Value>,
    action: &str,
) -> Result<Option<i64>, ArgumentError> {
    let Some(value) = args.get("lines") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = match value {
        Value::Bool(_) | Value::Null => None,
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            Some(number.as_i64().unwrap_or(i64::MAX))
        }
        _ => None,
    };
    let Some(lines) = value else {
        return Err(ArgumentError::new(format!(
            "El parámetro «lines» de la acción «{action}» debe ser un \
             número entero entre 10 y 120."
        )));
    };
    if !(10..=120).contains(&lines) {
        return Err(ArgumentError::new(
            "El parámetro «lines» debe estar entre 10 y 120.",
        ));
    }
    Ok(Some(lines))
}

/// `_wrapped`: normaliza el diccionario de Herdr a `ToolResult`.
pub fn wrapped(result: Map<String, Value>) -> crate::registry::ToolResult {
    if result.get("ok") == Some(&Value::Bool(false)) {
        let message = match result.get("error") {
            Some(value) if truthy(value) => python_str(value),
            _ => "Herdr no pudo completar la acción.".to_string(),
        };
        return crate::registry::ToolResult::err(message);
    }
    crate::registry::ToolResult::ok(Value::Object(result))
}

/// `str(args.get(key) or "").strip()`.
pub fn text_field(args: &Map<String, Value>, key: &str) -> String {
    py_strip(&text_or_empty(args.get(key))).to_string()
}

/// `bool(args.get(key))` de Python para el JSON.
pub fn truthy_field(args: &Map<String, Value>, key: &str) -> bool {
    args.get(key).is_some_and(truthy)
}

fn hook_text(hook: &TextFn) -> String {
    catch_unwind(AssertUnwindSafe(|| hook())).unwrap_or_default()
}

/// `_user_text`: el texto del usuario en minúsculas; un callback no rompe la herramienta.
pub fn user_text(callbacks: &HerdrCallbacks) -> String {
    hook_text(&callbacks.get_last_user_text).to_lowercase()
}

fn any_word(text: &str, words: &[&str]) -> bool {
    words.iter().any(|word| text.contains(word))
}

pub fn wants_notify(text: &str) -> bool {
    any_word(text, &NOTIFY_WORDS)
}

pub fn wants_deliver(text: &str) -> bool {
    any_word(text, &DELIVER_WORDS)
}

pub fn wants_close(text: &str) -> bool {
    any_word(text, &CLOSE_WORDS)
}

/// `_shown`: cómo se nombra un agente ante el usuario: su pestaña, nunca la
/// dirección («t51»).
pub fn shown(address: &str, label_for: &dyn Fn(&str) -> String) -> String {
    let mut label = String::new();
    if !address.is_empty() {
        label = catch_unwind(AssertUnwindSafe(|| label_for(address)))
            .map(|text| py_strip(&text).to_string())
            .unwrap_or_default();
    }
    if label.is_empty() {
        "el agente indicado".to_string()
    } else {
        label
    }
}

#[cfg(test)]
mod tests;
