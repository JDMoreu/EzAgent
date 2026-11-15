//! Port of `Messenger in src/ezagent/voice/llm.py` (F4-07/F4-08).
//!
//! El mensajero guarda la conversación, arma cada petición del proveedor, corre
//! el bucle de herramientas y deja el historial válido cuando un turno se corta.
//! El proveedor se elige por `llm_api_style` (OpenAI o Anthropic, F4-06) y el
//! núcleo agéntico entra por [`AgentRuntimeApi`]: `ez-llm` no puede depender de
//! `ez-agent` porque la dependencia va en el otro sentido.
//!
//! Concurrencia: `ask_stream` corre el turno en un hilo propio (`ezagent-turn`)
//! y entrega los fragmentos por un canal; `cancel_turn` corta ese turno desde
//! otro hilo. Nada de async y nada bloquea al que llama.

mod constants;
mod history;
pub mod notices;
mod parity;
mod recovery;
mod request;
mod tool_loop;

pub use constants::*;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use ez_core::config::{int_value, VoiceConfig};
use ez_core::pyvalue::text_or_empty;
use serde_json::{json, Value};

pub use request::LlmClient;
pub use request::{
    max_tokens_with, notice, remaining_tools, seal_tool_calls, user_content, with_volatile,
};
pub use tool_loop::{tool_ack, AskMode, AskStream};

use history::HistoryState;
use request::build_client;

/// Fallo del núcleo agéntico: sustituye a la excepción del runtime de Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    /// `type(exc).__name__`.
    pub kind: String,
    /// `str(exc)`.
    pub message: String,
}

impl RuntimeFailure {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RuntimeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RuntimeFailure {}

/// Lo que el `Messenger` usa del `AgentRuntime` de `ez-agent`.
///
/// F4-09 lo implementa en `ez-agent`; aquí solo se declara para no invertir la
/// dependencia. Cada método lleva el nombre del método Python equivalente.
pub trait AgentRuntimeApi: Send + Sync {
    /// `runtime.begin_turn(text)`: reinicia el presupuesto del turno y la memoria.
    fn begin_turn(&self, text: &str) -> Result<(), RuntimeFailure>;

    /// `runtime.system_prompt()`.
    fn system_prompt(&self) -> String;

    /// `runtime.prompt_parts()`. Por defecto todo el prompt es estable.
    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        Ok((self.system_prompt(), String::new()))
    }

    /// `runtime.tools_for(text)`: esquemas de herramientas para este turno.
    fn tools_for(&self, text: &str) -> Result<Vec<Value>, RuntimeFailure>;

    /// `runtime.registry.schemas(names=list(BASE_TOOLS), limit=14)`.
    fn base_tool_schemas(&self) -> Result<Vec<Value>, RuntimeFailure>;

    /// `runtime.remaining_tool_calls()`; `None` si el runtime no lleva presupuesto.
    fn remaining_tool_calls(&self) -> Option<i64> {
        None
    }

    /// `runtime.execute(name, raw_arguments)`; el valor es el `ToolResult` serializado.
    fn execute(&self, name: &str, raw_arguments: &str) -> Result<Value, RuntimeFailure>;

    /// `runtime.render_result(result)`: texto del resultado para el modelo.
    fn render_result(&self, result: Value) -> String;

    /// `runtime.record_assistant(text)`: memoria de la conversación.
    fn record_assistant(&self, text: &str) {
        let _ = text;
    }

    /// `runtime.refresh()`: relee permisos y skills.
    fn refresh(&self) {}

    /// `runtime.set_confirm_actions(enabled)`.
    fn set_confirm_actions(&self, enabled: bool) {
        let _ = enabled;
    }

    /// `runtime.cancel_confirmation()`.
    fn cancel_confirmation(&self) {}

    /// `runtime.pending_notify_address()`.
    fn pending_notify_address(&self) -> Option<String> {
        None
    }
}

/// Identidad y señal de cancelación de un turno concreto.
#[derive(Debug, Default)]
pub struct TurnToken {
    cancelled: AtomicBool,
}

impl TurnToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Fallo de un turno: cancelación, plazo agotado, proveedor o núcleo agéntico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnError {
    /// `TurnCancelled`: la app lo descarta en silencio.
    Cancelled(String),
    /// `LLMTurnTimeout`: el turno agotó su presupuesto total.
    Timeout(String),
    /// Fallo del proveedor (HTTP, red, flujo cortado).
    Llm(crate::errors::LlmError),
    /// Fallo del núcleo agéntico.
    Runtime(RuntimeFailure),
}

impl TurnError {
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::Cancelled(message.into())
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled(message) | Self::Timeout(message) => formatter.write_str(message),
            Self::Llm(error) => formatter.write_str(&error.message),
            Self::Runtime(failure) => formatter.write_str(&failure.message),
        }
    }
}

impl std::error::Error for TurnError {}

impl From<crate::errors::LlmError> for TurnError {
    fn from(error: crate::errors::LlmError) -> Self {
        Self::Llm(error)
    }
}

impl From<RuntimeFailure> for TurnError {
    fn from(failure: RuntimeFailure) -> Self {
        Self::Runtime(failure)
    }
}

/// Callback que recibe un texto (avisos de contexto y de turno ocupado).
pub type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Callbacks que el Python guarda en el constructor. Hoy no los usa el
/// `Messenger` (el Python tampoco), pero se conservan para el cableado de la app.
#[derive(Clone, Default)]
pub struct MessengerCallbacks {
    pub last_summary: Option<Arc<dyn Fn() -> String + Send + Sync>>,
    pub watch: Option<TextCallback>,
    pub mute: Option<Arc<dyn Fn(i64) + Send + Sync>>,
    pub request_close: Option<Arc<dyn Fn() + Send + Sync>>,
    pub mic_mute: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

/// Vista de la configuración que usa un turno; se toma con el lock de `VoiceConfig`.
#[derive(Debug, Clone)]
pub struct LlmSettings {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub api_style: String,
    pub temperature: Value,
    pub max_tokens: i64,
    pub context_tokens: i64,
    pub reasoning_effort: String,
    pub timeout_secs: f64,
    pub turn_timeout_secs: f64,
    pub max_tool_rounds: i64,
}

fn float_value(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Value::String(text) => ez_core::pyvalue::py_strip(text).parse().ok(),
        _ => None,
    }
}

impl LlmSettings {
    pub fn from_config(cfg: &VoiceConfig) -> Self {
        Self {
            provider: text_or_empty(cfg.get("llm_provider")),
            model: text_or_empty(cfg.get("llm_model")),
            base_url: text_or_empty(cfg.get("llm_base_url")),
            api_key: text_or_empty(cfg.get("api_key")),
            api_style: text_or_empty(cfg.get("llm_api_style")),
            temperature: cfg.get("llm_temperature").cloned().unwrap_or(json!(0.0)),
            max_tokens: int_value(cfg.get("llm_max_tokens").unwrap_or(&Value::Null), 320),
            context_tokens: int_value(
                cfg.get("llm_context_tokens").unwrap_or(&Value::Null),
                32_000,
            ),
            reasoning_effort: text_or_empty(cfg.get("llm_reasoning_effort")),
            timeout_secs: float_value(cfg.get("llm_timeout_secs")).unwrap_or(30.0),
            turn_timeout_secs: float_value(cfg.get("llm_turn_timeout_secs")).unwrap_or(120.0),
            max_tool_rounds: int_value(cfg.get("llm_max_tool_rounds").unwrap_or(&Value::Null), 8),
        }
    }

    pub(crate) fn anthropic_style(&self) -> bool {
        self.api_style == "anthropic"
    }

    /// `_request_timeout`: 1-600 s, 30 si no es un número válido.
    pub fn request_timeout(&self) -> f64 {
        if (1.0..=600.0).contains(&self.timeout_secs) {
            self.timeout_secs
        } else {
            30.0
        }
    }

    /// `_turn_timeout`: 1-3600 s, 120 si no es un número válido.
    pub fn turn_timeout(&self) -> f64 {
        if (1.0..=3600.0).contains(&self.turn_timeout_secs) {
            self.turn_timeout_secs
        } else {
            120.0
        }
    }

    /// `_tool_round_limit`: entre 1 y 12 vueltas, 8 si no es un entero válido.
    pub fn tool_round_limit(&self) -> usize {
        self.max_tool_rounds.clamp(1, 12) as usize
    }
}

/// Flujo activo del turno, para poder cerrarlo desde el hilo que cancela.
pub(crate) struct ActiveStream {
    pub token: Arc<TurnToken>,
    pub stream: Mutex<Box<dyn crate::openai::ChatStream>>,
}

struct Shared {
    cfg: Mutex<VoiceConfig>,
    runtime: Option<Arc<dyn AgentRuntimeApi>>,
    client: RwLock<Arc<dyn LlmClient>>,
    history: Mutex<HistoryState>,
    turn_lock: Mutex<()>,
    turn_token: Mutex<Option<Arc<TurnToken>>>,
    turn_generation: AtomicU64,
    active_stream: Mutex<Option<Arc<ActiveStream>>>,
    last_rounds: Mutex<Vec<String>>,
    on_context_event: Mutex<Option<TextCallback>>,
    on_busy: Mutex<Option<TextCallback>>,
    callbacks: MessengerCallbacks,
}

/// `Messenger` del Python: historial, peticiones, herramientas y cancelación.
///
/// Es `Clone` y barato: todas las copias comparten el mismo estado, así que se
/// puede usar desde el hilo del turno, el de la UI de chat y el que cancela.
#[derive(Clone)]
pub struct Messenger {
    shared: Arc<Shared>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Identificador opaco de sesión para `x-opencode-session` (`uuid4().hex` en Python).
///
/// DECISION: `uuid` no es dependencia de `ez-llm` y no se puede tocar el
/// `Cargo.toml`; nanos + pid + contador cumple la única propiedad que importa
/// (opaco y distinto por turno).
fn unique_hex() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:032x}{:08x}{count:016x}", std::process::id())
}

impl Messenger {
    /// Recrea el `Messenger.__init__`: cliente del proveedor e historial con el
    /// prompt de sistema vigente.
    pub fn new(
        cfg: VoiceConfig,
        callbacks: MessengerCallbacks,
        runtime: Option<Arc<dyn AgentRuntimeApi>>,
    ) -> Self {
        let client = build_client(&LlmSettings::from_config(&cfg));
        let messenger = Self {
            shared: Arc::new(Shared {
                cfg: Mutex::new(cfg),
                runtime,
                client: RwLock::new(client),
                history: Mutex::new(HistoryState::new()),
                turn_lock: Mutex::new(()),
                turn_token: Mutex::new(None),
                turn_generation: AtomicU64::new(0),
                active_stream: Mutex::new(None),
                last_rounds: Mutex::new(Vec::new()),
                on_context_event: Mutex::new(None),
                on_busy: Mutex::new(None),
                callbacks,
            }),
        };
        let system = json!({"role": "system", "content": messenger.system_prompt()});
        messenger.replace_history(vec![system], "", "");
        messenger
    }

    /// Copia de la configuración vigente (el Python expone `messenger.cfg`).
    pub fn config(&self) -> VoiceConfig {
        lock(&self.shared.cfg).clone()
    }

    /// Sustituye la configuración; el equivalente a mutar `messenger.cfg` y
    /// llamar a `reload_llm` en el Python.
    ///
    /// DECISION: `VoiceConfig` se guarda dentro del `Messenger` (el Python
    /// comparte el objeto); la app sincroniza con `apply_config` + `config`.
    pub fn apply_config(&self, cfg: VoiceConfig) {
        *lock(&self.shared.cfg) = cfg;
    }

    pub fn settings(&self) -> LlmSettings {
        LlmSettings::from_config(&lock(&self.shared.cfg))
    }

    /// Cambia el cliente del proveedor (en el Python se asigna `messenger.client`).
    pub fn set_client(&self, client: Arc<dyn LlmClient>) {
        *write_lock(&self.shared.client) = client;
    }

    pub(crate) fn client(&self) -> Arc<dyn LlmClient> {
        read_lock(&self.shared.client).clone()
    }

    /// Callbacks guardados en el constructor (los usa el cableado de la app).
    pub fn callbacks(&self) -> MessengerCallbacks {
        self.shared.callbacks.clone()
    }

    /// `last_rounds`: desglose del último turno (una entrada por llamada al LLM).
    pub fn last_rounds(&self) -> Vec<String> {
        lock(&self.shared.last_rounds).clone()
    }

    /// `on_context_event`: aviso de compactación para el chat.
    pub fn set_on_context_event(&self, callback: TextCallback) {
        *lock(&self.shared.on_context_event) = Some(callback);
    }

    /// `on_busy`: aviso de que el turno anterior sigue vivo; recibe el modo.
    pub fn set_on_busy(&self, callback: TextCallback) {
        *lock(&self.shared.on_busy) = Some(callback);
    }

    /// `runtime` del Python (solo lectura para el resto del crate).
    pub(crate) fn runtime(&self) -> Option<Arc<dyn AgentRuntimeApi>> {
        self.shared.runtime.clone()
    }

    /// `cancel_turn`: invalida el turno vigente y despierta una lectura bloqueada.
    pub fn cancel_turn(&self) {
        if let Some(token) = lock(&self.shared.turn_token).clone() {
            token.cancel();
        }
        self.shared.turn_generation.fetch_add(1, Ordering::SeqCst);
        let stream = lock(&self.shared.active_stream).clone();
        if let Some(stream) = stream {
            // DECISION: `try_lock` para no bloquear al que cancela; si el hilo
            // del turno está en una lectura, el cierre queda para el siguiente
            // fragmento (reqwest::blocking tampoco interrumpe un read).
            if let Ok(mut guard) = stream.stream.try_lock() {
                guard.close();
            }
        }
        if let Some(runtime) = self.runtime() {
            runtime.cancel_confirmation();
        }
    }

    /// `reload_llm`: recrea el cliente tras cambiar proveedor, modelo, URL o clave.
    pub fn reload_llm(&self) {
        self.cancel_turn();
        let client = build_client(&self.settings());
        let old = {
            let mut slot = write_lock(&self.shared.client);
            std::mem::replace(&mut *slot, client)
        };
        if !Arc::ptr_eq(&old, &self.client()) {
            old.close();
        }
        if let Some(runtime) = self.runtime() {
            runtime.refresh();
        }
        {
            let mut history = self.history();
            history.last_agent_address = None;
        }
        let system = json!({"role": "system", "content": self.system_prompt()});
        self.replace_history(vec![system], "", "");
    }

    /// `close`: corta el turno activo y cierra el transporte del proveedor.
    pub fn close(&self) {
        self.cancel_turn();
        self.client().close();
    }

    /// `_system_prompt`: sin núcleo agéntico se conserva una conversación mínima.
    pub fn system_prompt(&self) -> String {
        match self.runtime() {
            // DECISION (del Python): sin núcleo agéntico no se permiten
            // herramientas que no existen.
            None => "Eres el asistente de voz de EzAgent. El núcleo agéntico no está disponible, \
así que no puedes usar herramientas ni lanzar agentes. Solo puedes responder conversando, en \
español neutro, en una o dos frases cortas y sin markdown. Si te piden una acción, di con \
claridad que no puedes ejecutarla en este momento."
                .to_string(),
            Some(runtime) => runtime.system_prompt(),
        }
    }

    /// `set_confirm_actions`: configuración, runtime y prompt de sistema al día.
    pub fn set_confirm_actions(&self, enabled: bool) {
        lock(&self.shared.cfg).set("confirm_actions", json!(enabled));
        if let Some(runtime) = self.runtime() {
            runtime.set_confirm_actions(enabled);
        }
        let prompt = self.system_prompt();
        let mut history = self.history();
        let can_update = !history.turn_active
            && history.messages.first().is_some_and(|message| {
                message.get("role").and_then(Value::as_str) == Some("system")
            });
        if can_update {
            // Durante un turno el prompt ya viajó; el siguiente lo rehace.
            history.messages[0]["content"] = Value::String(prompt);
        }
    }

    /// `_notify_context`: aviso best-effort de compactación.
    pub(crate) fn notify_context(&self, text: &str) {
        let callback = lock(&self.shared.on_context_event).clone();
        if let Some(callback) = callback {
            callback(text);
        }
    }

    /// `_prompt_parts`: `(prompt estable, datos volátiles)`.
    pub fn prompt_parts(&self, runtime: &Arc<dyn AgentRuntimeApi>) -> (String, String) {
        match runtime.prompt_parts() {
            Ok(parts) => parts,
            // DECISION (del Python): si el runtime no separa, todo es estable.
            Err(_) => (runtime.system_prompt(), String::new()),
        }
    }

    /// `_check_cancel`.
    pub fn check_cancel(&self, token: &TurnToken) -> Result<(), TurnError> {
        if token.is_cancelled() {
            return Err(TurnError::cancelled("turno cancelado por el usuario"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ez_core::pyjson::dumps_with;
    use ez_core::pyjson::DumpOptions;

    #[test]
    fn cancelled_result_matches_json_dumps() {
        let options = DumpOptions::utf8();
        assert_eq!(
            dumps_with(
                &json!({"ok": false, "error": "Cancelado o interrumpido por el usuario."}),
                &options
            ),
            CANCELLED_TOOL_RESULT
        );
    }

    #[test]
    fn settings_fall_back_when_values_are_not_numbers() {
        let mut cfg = VoiceConfig::new();
        cfg.set("llm_timeout_secs", json!("nada"));
        cfg.set("llm_turn_timeout_secs", json!(0));
        cfg.set("llm_max_tool_rounds", json!(99));
        let settings = LlmSettings::from_config(&cfg);
        assert_eq!(settings.request_timeout(), 30.0);
        assert_eq!(settings.turn_timeout(), 120.0);
        assert_eq!(settings.tool_round_limit(), 12);
        assert_eq!(settings.max_tokens, 320);
        assert_eq!(settings.context_tokens, 32_000);
    }

    #[test]
    fn turn_token_is_cancelled_once_only() {
        let token = TurnToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        assert_eq!(
            TurnError::cancelled("turno cancelado por el usuario").to_string(),
            "turno cancelado por el usuario"
        );
    }

    #[test]
    fn unique_hex_is_opaque_and_changes() {
        let first = unique_hex();
        let second = unique_hex();
        assert_ne!(first, second);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }
}
