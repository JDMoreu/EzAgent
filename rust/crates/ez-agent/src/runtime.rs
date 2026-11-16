//! Port of `src/ezagent/agent/runtime.py` (F4-09).
//!
//! El runtime une configuración, registro de herramientas, política,
//! confirmación, auditoría, memoria y skills en un objeto que el mensajero de
//! voz puede usar sin conocer cada herramienta. No crea ventanas ni habla:
//! publica intenciones en los callbacks que le entrega la aplicación.
//!
//! Concurrencia: todas las copias de [`AgentRuntime`] comparten el mismo estado
//! (`Arc`). El hilo del turno ejecuta herramientas y confirmaciones; la UI
//! responde desde el suyo con `submit_confirmation*`. Los callbacks se invocan
//! fuera de los cerrojos del runtime.
//!
//! DECISION: `herdr` y `callbacks.swarm` son traits locales (`HerdrApi`,
//! `SwarmStack`) porque los crates dueños (`ez-herdr`, `ez-swarm`) aún no
//! existen y `ez-agent` no puede depender de ellos. Un `None` reproduce el
//! `getattr(..., None)` del Python.
//!
//! DECISION: los grupos `build_herdr_tools` y `build_swarm_tools` pertenecen a
//! F4-12b/F4-13b y hoy son esqueletos vacíos; se inyectan con
//! [`AgentRuntime::add_tool_group`] sin tocar este archivo.

mod execute;
mod selection;
mod state;
#[cfg(test)]
mod tests;
mod tools;

use std::path::Path;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use serde_json::Value;

use crate::confirm::ConfirmationManager;
use crate::registry::ToolSpec;
use state::Inner;

pub use selection::{select_tool_names, SelectOptions};

/// Herramientas que escriben en disco.
pub const WRITE_TOOLS: [&str; 7] = [
    "create_dir",
    "write_file",
    "copy_path",
    "move_path",
    "open_path",
    "undo_last",
    "run_skill_script",
];

/// Base estable de cinco herramientas de uso real.
pub const BASE_TOOLS: [&str; 5] = [
    "voice_control",
    "web_search",
    "fetch_page",
    "read_file",
    "delegate_to_agent",
];

/// Leer la pantalla de un agente no convierte en peligroso controlarlo.
const TAINT_FREE_TOOLS: [&str; 3] = ["delegate_to_agent", "voice_control", "swarm"];

/// Lo que el runtime usa de Herdr (adaptador, catálogo y ventanas).
pub trait HerdrApi: Send + Sync {
    fn catalog_text(&self) -> String {
        String::new()
    }

    fn context_text(&self) -> String {
        String::new()
    }

    fn list_agents(&self) -> Vec<Value> {
        Vec::new()
    }
}

/// Modo enjambre (`ezagent.swarm.stack.SwarmStack`).
pub trait SwarmStack: Send + Sync {
    fn active(&self) -> bool;
}

/// Puerta de confirmación; `ConfirmationManager` la implementa y las pruebas
/// pueden sustituirla igual que el Python asigna `runtime.confirm`.
pub trait ConfirmationGate: Send + Sync {
    fn ask(&self, summary: &str, strong: bool, require_number: Option<i64>) -> bool;
    fn pending(&self) -> bool;
    fn pending_token(&self) -> Option<String>;
    fn arm(&self, token: Option<&str>);
    fn submit_text(&self, text: &str, token: Option<&str>) -> Option<bool>;
    fn submit_click(&self, yes: bool);
    fn cancel(&self, reason: &str);
}

impl ConfirmationGate for ConfirmationManager {
    fn ask(&self, summary: &str, strong: bool, require_number: Option<i64>) -> bool {
        ConfirmationManager::ask(self, summary, strong, require_number)
    }

    fn pending(&self) -> bool {
        ConfirmationManager::pending(self)
    }

    fn pending_token(&self) -> Option<String> {
        ConfirmationManager::pending_token(self)
    }

    fn arm(&self, token: Option<&str>) {
        ConfirmationManager::arm(self, token);
    }

    fn submit_text(&self, text: &str, token: Option<&str>) -> Option<bool> {
        ConfirmationManager::submit_text(self, text, token)
    }

    fn submit_click(&self, yes: bool) {
        ConfirmationManager::submit_click(self, yes);
    }

    fn cancel(&self, reason: &str) {
        ConfirmationManager::cancel(self, reason);
    }
}

pub type TextFn = Arc<dyn Fn(&str) + Send + Sync>;
pub type TextProducer = Arc<dyn Fn() -> String + Send + Sync>;
pub type ConfirmAskFn = Arc<dyn Fn(&str, bool, Option<&str>) + Send + Sync>;
pub type VoidFn = Arc<dyn Fn() + Send + Sync>;
pub type MuteFn = Arc<dyn Fn(i64) + Send + Sync>;
pub type MuteToggleFn = Arc<dyn Fn(bool) + Send + Sync>;
pub type RememberTaskFn = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;
pub type SpecFn = Arc<dyn Fn(&Value) + Send + Sync>;

/// Constructor de un grupo adicional de herramientas (herdr, enjambre…).
pub type ToolGroupBuilder = Arc<dyn Fn(&AgentRuntime) -> Vec<ToolSpec> + Send + Sync>;

fn ignore_text(_: &str) {}
fn ignore_bool(_: bool) {}
fn ignore_minutes(_: i64) {}
fn ignore_void() {}
fn ignore_task(_: &str, _: &str, _: &str) {}
fn ignore_spec(_: &Value) {}
fn empty_text() -> String {
    String::new()
}

/// Puentes entre el runtime y la aplicación de voz.
#[derive(Clone)]
pub struct RuntimeCallbacks {
    pub watch: TextFn,
    pub request_close: VoidFn,
    pub mic_mute: MuteToggleFn,
    pub last_summary: TextProducer,
    pub mute: MuteFn,
    pub get_last_user_text: TextProducer,
    pub remember_task: RememberTaskFn,
    pub enqueue_chain: SpecFn,
    pub deliver_answer: TextFn,
    pub prewarm_chain: SpecFn,
    pub async_launch: bool,
    pub pending_tasks: TextProducer,
    pub swarm: Option<Arc<dyn SwarmStack>>,
    /// DECISION: el Python acepta callbacks de 2 o 3 argumentos; aquí siempre
    /// recibe el token (la UI antigua simplemente lo ignora).
    pub on_confirm_ask: Option<ConfirmAskFn>,
    pub on_confirm_done: Option<VoidFn>,
    pub on_warn: Option<TextFn>,
}

impl Default for RuntimeCallbacks {
    fn default() -> Self {
        Self {
            watch: Arc::new(ignore_text),
            request_close: Arc::new(ignore_void),
            mic_mute: Arc::new(ignore_bool),
            last_summary: Arc::new(empty_text),
            mute: Arc::new(ignore_minutes),
            get_last_user_text: Arc::new(empty_text),
            remember_task: Arc::new(ignore_task),
            enqueue_chain: Arc::new(ignore_spec),
            deliver_answer: Arc::new(ignore_text),
            prewarm_chain: Arc::new(ignore_spec),
            async_launch: false,
            pending_tasks: Arc::new(empty_text),
            swarm: None,
            on_confirm_ask: None,
            on_confirm_done: None,
            on_warn: None,
        }
    }
}

/// Palabras de `pending_notify_address` (`str.lower()` + `in`).
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

fn uuid_hex() -> String {
    // `uuid4().hex`: 32 dígitos hexadecimales sin guiones.
    format!("{:032x}", uuid::Uuid::new_v4().as_u128())
}

/// Fachada del núcleo agéntico para el mensajero de voz.
#[derive(Clone)]
pub struct AgentRuntime {
    pub(crate) inner: Arc<Inner>,
}

impl AgentRuntime {
    /// `AgentRuntime.__init__`: configuración, memoria, skills, diario y
    /// registro de herramientas.
    pub fn new(
        voice_config: VoiceConfig,
        herdr: Option<Arc<dyn HerdrApi>>,
        callbacks: RuntimeCallbacks,
        config_path: Option<&Path>,
    ) -> Result<Self, String> {
        state::build(voice_config, herdr, callbacks, config_path)
    }

    pub fn config(&self) -> ez_core::agent_config::AgentConfig {
        state::read(&self.inner.config).clone()
    }

    /// Sustituye la configuración (equivalente a mutar `runtime.config`).
    /// Cambia la configuración y reconstruye lo que depende de ella, como la
    /// recarga de `permissions.toml`: las herramientas de archivos guardan su
    /// copia al construirse y, sin esto, seguían con los límites viejos (el
    /// Python lee la configuración viva en cada llamada).
    pub fn set_config(&self, config: ez_core::agent_config::AgentConfig) {
        *state::write(&self.inner.config) = config;
        self.rebuild_prompt();
        self.rebuild_confirm();
        self.rebuild_tools();
    }

    pub fn voice_config(&self) -> VoiceConfig {
        state::read(&self.inner.voice_config).clone()
    }

    pub fn set_voice_config(&self, config: VoiceConfig) {
        *state::write(&self.inner.voice_config) = config;
    }

    pub fn audit(&self) -> Arc<crate::audit::AuditLog> {
        Arc::clone(&self.inner.audit)
    }

    /// Callbacks guardados (el Python expone `runtime.callbacks`).
    pub fn callbacks(&self) -> RuntimeCallbacks {
        self.inner.callbacks.clone()
    }

    pub fn herdr(&self) -> Option<Arc<dyn HerdrApi>> {
        self.inner.herdr.clone()
    }

    pub fn session_id(&self) -> &str {
        &self.inner.session_id
    }

    pub fn turn_id(&self) -> String {
        state::lock(&self.inner.session).turn_id.clone()
    }

    pub fn tainted(&self) -> bool {
        state::lock(&self.inner.session).tainted
    }

    pub fn confirm_nondestructive(&self) -> bool {
        state::lock(&self.inner.session).confirm_nondestructive
    }

    pub fn last_agent_address(&self) -> Option<String> {
        state::lock(&self.inner.session).last_agent_address.clone()
    }

    pub fn set_last_agent_address(&self, address: Option<&str>) {
        state::lock(&self.inner.session).last_agent_address = address.map(str::to_string);
    }

    pub fn memory(&self) -> Arc<crate::memory::MemoryStore> {
        Arc::clone(&self.inner.memory)
    }

    pub fn skills(&self) -> Arc<crate::skills::SkillLibrary> {
        Arc::clone(&self.inner.skills)
    }

    pub fn journal(&self) -> Arc<crate::journal::Journal> {
        Arc::clone(&self.inner.journal)
    }

    pub fn registry(&self) -> Arc<crate::registry::ToolRegistry> {
        Arc::clone(&state::read(&self.inner.registry))
    }

    pub fn set_registry(&self, registry: Arc<crate::registry::ToolRegistry>) {
        *state::write(&self.inner.registry) = registry;
    }

    pub fn confirm_gate(&self) -> Option<Arc<dyn ConfirmationGate>> {
        state::read(&self.inner.confirm).clone()
    }

    pub fn set_confirm_gate(&self, gate: Option<Arc<dyn ConfirmationGate>>) {
        *state::write(&self.inner.confirm) = gate;
    }

    /// Añade un grupo de herramientas y reconstruye el registro.
    pub fn add_tool_group(&self, builder: ToolGroupBuilder) {
        state::write(&self.inner.tool_groups).push(builder);
        self.rebuild_tools();
    }

    /// `_watch_agent`: el último agente vigilado sostiene los avisos.
    pub fn watch_agent(&self, address: &str) {
        self.set_last_agent_address(Some(address));
        (self.inner.callbacks.watch)(address);
    }

    /// `_launch_failed`: un lanzamiento asíncrono no llegó a buen puerto.
    pub fn launch_failed(&self, address: &str, agent: &str, error: &str) {
        self.inner.audit_warn(&format!(
            "El lanzamiento de {agent} ({address}) falló: {error}"
        ));
        self.inner.warn(ez_core::pyvalue::py_strip(&format!(
            "No pude terminar de abrir {agent}. {error}"
        )));
    }

    /// `_get_last_user_text`: lo que dijo el usuario en este turno.
    pub fn get_last_user_text(&self) -> String {
        let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.inner.callbacks.get_last_user_text)()
        }))
        .unwrap_or_default();
        if !value.is_empty() {
            return value;
        }
        state::lock(&self.inner.session).last_user_text.clone()
    }

    /// Los tres últimos mensajes del usuario con su instante (10.4).
    pub fn recent_user_texts(&self) -> Vec<(String, f64)> {
        state::lock(&self.inner.session).recent_user_texts.clone()
    }

    /// Agente que corre en una dirección de Herdr (vacío si no se sabe).
    pub fn agent_at(&self, address: &str) -> String {
        if address.is_empty() {
            return String::new();
        }
        let Some(herdr) = &self.inner.herdr else {
            return String::new();
        };
        for item in std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| herdr.list_agents()))
            .unwrap_or_default()
        {
            let Some(object) = item.as_object() else {
                continue;
            };
            let known = object
                .get("address")
                .filter(|value| ez_core::pyvalue::truthy(value))
                .map_or_else(String::new, ez_core::pyvalue::python_str);
            if known == address {
                return object
                    .get("agent")
                    .filter(|value| ez_core::pyvalue::truthy(value))
                    .map_or_else(String::new, ez_core::pyvalue::python_str);
            }
        }
        String::new()
    }

    /// `begin_turn`: reinicia el presupuesto y guarda el texto del usuario.
    pub fn begin_turn(&self, text: &str) {
        {
            let mut session = state::lock(&self.inner.session);
            session.turn_id = uuid_hex();
            session.last_user_text = text.to_string();
            if !text.is_empty() {
                session
                    .recent_user_texts
                    .push((text.to_string(), ez_core::pytime::timestamp()));
                let excess = session.recent_user_texts.len().saturating_sub(3);
                session.recent_user_texts.drain(..excess);
            }
            session.tainted = false;
            session.calls_this_turn = 0;
            session.level1_writes = 0;
            session.carry_tools = std::mem::take(&mut session.next_carry);
        }
        self.refresh();
        let _ = self.inner.memory.record(
            "user",
            text,
            Some(&self.inner.session_id),
            &crate::memory::MemoryTimestamp::Now,
        );
    }

    pub fn record_assistant(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let _ = self.inner.memory.record(
            "assistant",
            text,
            Some(&self.inner.session_id),
            &crate::memory::MemoryTimestamp::Now,
        );
    }

    /// `system_prompt`: nunca falla; si el constructor falla queda el respaldo.
    pub fn system_prompt(&self) -> String {
        let built = {
            let mut guard = state::lock(&self.inner.prompt);
            match guard.as_mut() {
                None => None,
                Some(builder) => {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.build(None)))
                        .ok()
                }
            }
        };
        built.unwrap_or_else(|| {
            "Eres EzAgent, un agente autónomo para Windows. Responde corto y directo, en español neutro latinoamericano.".to_string()
        })
    }

    /// `prompt_parts`: prefijo estable y datos volátiles.
    pub fn prompt_parts(&self) -> (String, String) {
        let built = {
            let mut guard = state::lock(&self.inner.prompt);
            match guard.as_mut() {
                None => None,
                Some(builder) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    builder.build_parts(None)
                }))
                .ok(),
            }
        };
        built.unwrap_or_else(|| (self.system_prompt(), String::new()))
    }

    /// `close`: el `AuditLog` de Rust no mantiene descriptores abiertos.
    pub fn close(&self) {}

    pub fn remaining_tool_calls(&self) -> i64 {
        let config = self.config();
        let calls = state::lock(&self.inner.session).calls_this_turn;
        (config.general.max_tool_calls_per_turn - calls).max(0)
    }

    // ------------------------------------------------------- confirmación
    pub fn confirm_pending(&self) -> bool {
        self.confirm_gate().is_some_and(|gate| gate.pending())
    }

    pub fn pending_confirmation_token(&self) -> Option<String> {
        self.confirm_gate().and_then(|gate| gate.pending_token())
    }

    /// Rearma el plazo de la confirmación pendiente (F48).
    pub fn arm_confirmation(&self) {
        if let Some(gate) = self.confirm_gate() {
            gate.arm(None);
        }
    }

    pub fn submit_confirmation(&self, text: &str, token: Option<&str>) -> Option<bool> {
        self.confirm_gate()
            .and_then(|gate| gate.submit_text(text, token))
    }

    pub fn submit_confirmation_click(&self, yes: bool) {
        if let Some(gate) = self.confirm_gate() {
            gate.submit_click(yes);
        }
    }

    pub fn cancel_confirmation(&self) {
        if let Some(gate) = self.confirm_gate() {
            gate.cancel("");
        }
    }

    pub fn set_confirm_actions(&self, enabled: bool) {
        state::lock(&self.inner.session).confirm_nondestructive = enabled;
    }

    // ------------------------------------------------------------- avisos
    pub fn pending_notify_address(&self) -> Option<String> {
        let text = self.get_last_user_text().to_lowercase();
        let wants = NOTIFY_WORDS.iter().any(|word| text.contains(word));
        if !wants {
            return None;
        }
        self.last_agent_address()
            .filter(|address| !address.is_empty())
    }
}

impl ez_llm::messenger::AgentRuntimeApi for AgentRuntime {
    fn begin_turn(&self, text: &str) -> Result<(), ez_llm::messenger::RuntimeFailure> {
        AgentRuntime::begin_turn(self, text);
        Ok(())
    }

    fn system_prompt(&self) -> String {
        AgentRuntime::system_prompt(self)
    }

    fn prompt_parts(&self) -> Result<(String, String), ez_llm::messenger::RuntimeFailure> {
        Ok(AgentRuntime::prompt_parts(self))
    }

    fn tools_for(&self, text: &str) -> Result<Vec<Value>, ez_llm::messenger::RuntimeFailure> {
        Ok(AgentRuntime::tools_for(self, text))
    }

    fn base_tool_schemas(&self) -> Result<Vec<Value>, ez_llm::messenger::RuntimeFailure> {
        let names: Vec<String> = BASE_TOOLS.iter().map(|name| (*name).to_string()).collect();
        Ok(self.registry().schemas(Some(&names), 14))
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        Some(AgentRuntime::remaining_tool_calls(self))
    }

    fn execute(
        &self,
        name: &str,
        raw_arguments: &str,
    ) -> Result<Value, ez_llm::messenger::RuntimeFailure> {
        Ok(AgentRuntime::execute(self, name, &Value::String(raw_arguments.to_string())).to_dict())
    }

    fn render_result(&self, result: Value) -> String {
        crate::registry::ToolRegistry::render_for_llm(&state::tool_result_from_value(&result), 2000)
    }

    fn record_assistant(&self, text: &str) {
        AgentRuntime::record_assistant(self, text);
    }

    fn refresh(&self) {
        AgentRuntime::refresh(self);
    }

    fn set_confirm_actions(&self, enabled: bool) {
        AgentRuntime::set_confirm_actions(self, enabled);
    }

    fn cancel_confirmation(&self) {
        AgentRuntime::cancel_confirmation(self);
    }

    fn pending_notify_address(&self) -> Option<String> {
        AgentRuntime::pending_notify_address(self)
    }
}
