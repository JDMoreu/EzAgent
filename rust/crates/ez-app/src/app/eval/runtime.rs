//! Runtime y mensajero reales, con la ejecución de herramientas simulada
//! (`_RecordingRuntime` de `provider.py`).

use std::path::Path;
use std::sync::Arc;

use ez_agent::registry::ToolResult;
use ez_agent::runtime::{AgentRuntime, HerdrApi, RuntimeCallbacks};
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use ez_llm::messenger::{AgentRuntimeApi, RuntimeFailure};
use serde_json::Value;

/// `(resultado simulado por el eval)`: nada que el eval registre se ejecuta.
const SIMULATED_RESULT: &str = "(resultado simulado por el eval)";

/// `Herdr` visto por el núcleo agéntico, igual que en `--ask`.
struct EvalHerdr(Arc<Herdr>);

impl HerdrApi for EvalHerdr {
    fn catalog_text(&self) -> String {
        ez_herdr::driver::AgentDriver::new(Arc::clone(&self.0), self.0.config()).catalog_text()
    }

    fn context_text(&self) -> String {
        self.0.context_text()
    }

    fn list_agents(&self) -> Vec<Value> {
        self.0
            .list_agents(10.0)
            .map(|agents| agents.into_iter().map(Value::Object).collect())
            .unwrap_or_default()
    }
}

/// Configuración y runtime de un caso; los dos comparten la carpeta del caso.
pub struct Case {
    pub voice: VoiceConfig,
    pub runtime: AgentRuntime,
}

impl Case {
    /// `_build_runtime`: `VoiceConfig.load()` y `AgentRuntime` con el
    /// `permissions.toml` del caso (su carpeta es el `app_dir`).
    pub fn build(case_dir: &Path) -> Result<Self, String> {
        let voice = VoiceConfig::load(None)?;
        let herdr = Arc::new(Herdr::new(voice.clone()));
        let api: Arc<dyn HerdrApi> = Arc::new(EvalHerdr(Arc::clone(&herdr)));
        let runtime = AgentRuntime::new(
            voice.clone(),
            Some(api),
            RuntimeCallbacks::default(),
            Some(&case_dir.join("permissions.toml")),
        )?;
        // El Python construye `delegate_to_agent` y `voice_control` dentro del
        // runtime; en Rust el grupo de Herdr se inyecta (F4-09).
        runtime.add_tool_group(Arc::new(move |_: &AgentRuntime| {
            ez_agent::tools_herdr::build_herdr_tools(
                Arc::clone(&herdr),
                ez_agent::tools_herdr::HerdrCallbacks::default(),
            )
        }));
        Ok(Self { voice, runtime })
    }
}

/// `_RecordingRuntime`: el runtime real, salvo `execute`, que responde un
/// resultado simulado. El Python apunta las llamadas y nunca las usa.
pub struct RecordingRuntime(AgentRuntime);

impl RecordingRuntime {
    pub fn new(inner: AgentRuntime) -> Self {
        Self(inner)
    }
}

impl AgentRuntimeApi for RecordingRuntime {
    fn begin_turn(&self, text: &str) -> Result<(), RuntimeFailure> {
        AgentRuntimeApi::begin_turn(&self.0, text)
    }

    fn system_prompt(&self) -> String {
        AgentRuntimeApi::system_prompt(&self.0)
    }

    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        AgentRuntimeApi::prompt_parts(&self.0)
    }

    fn tools_for(&self, text: &str) -> Result<Vec<Value>, RuntimeFailure> {
        AgentRuntimeApi::tools_for(&self.0, text)
    }

    fn base_tool_schemas(&self) -> Result<Vec<Value>, RuntimeFailure> {
        AgentRuntimeApi::base_tool_schemas(&self.0)
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        AgentRuntimeApi::remaining_tool_calls(&self.0)
    }

    fn execute(&self, _name: &str, _raw_arguments: &str) -> Result<Value, RuntimeFailure> {
        Ok(ToolResult::ok(Value::String(SIMULATED_RESULT.to_string())).to_dict())
    }

    fn render_result(&self, result: Value) -> String {
        AgentRuntimeApi::render_result(&self.0, result)
    }

    fn record_assistant(&self, text: &str) {
        AgentRuntimeApi::record_assistant(&self.0, text);
    }

    fn refresh(&self) {
        AgentRuntimeApi::refresh(&self.0);
    }

    fn set_confirm_actions(&self, enabled: bool) {
        AgentRuntimeApi::set_confirm_actions(&self.0, enabled);
    }

    fn cancel_confirmation(&self) {
        AgentRuntimeApi::cancel_confirmation(&self.0);
    }

    fn pending_notify_address(&self) -> Option<String> {
        AgentRuntimeApi::pending_notify_address(&self.0)
    }
}
