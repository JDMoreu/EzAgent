//! Envoltorio de `AgentRuntimeApi` que anota `tool.call` y `tool.result`.
//!
//! Espejo de `instrument_runtime` de `parity/replay/run_py.py`: se registra la
//! llamada antes de ejecutar y el desenlace después, con `ok` y `outcome` del
//! `ToolResult` serializado.

use std::sync::Arc;

use ez_llm::messenger::{AgentRuntimeApi, RuntimeFailure};

use serde_json::Value;

use super::recording::Recorder;

/// Runtime que delega y anota cada herramienta.
pub struct TracedRuntime {
    inner: Arc<dyn AgentRuntimeApi>,
    recorder: Arc<Recorder>,
}

impl TracedRuntime {
    /// Envuelve el runtime real con el registrador del arnés.
    pub fn new(inner: Arc<dyn AgentRuntimeApi>, recorder: Arc<Recorder>) -> Self {
        Self { inner, recorder }
    }
}

impl AgentRuntimeApi for TracedRuntime {
    fn begin_turn(&self, text: &str) -> Result<(), RuntimeFailure> {
        self.inner.begin_turn(text)
    }

    fn system_prompt(&self) -> String {
        self.inner.system_prompt()
    }

    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        self.inner.prompt_parts()
    }

    fn tools_for(&self, text: &str) -> Result<Vec<Value>, RuntimeFailure> {
        self.inner.tools_for(text)
    }

    fn base_tool_schemas(&self) -> Result<Vec<Value>, RuntimeFailure> {
        self.inner.base_tool_schemas()
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        self.inner.remaining_tool_calls()
    }

    fn execute(&self, name: &str, raw_arguments: &str) -> Result<Value, RuntimeFailure> {
        self.recorder.record_tool_call(name, raw_arguments);
        let result = self.inner.execute(name, raw_arguments)?;
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let outcome = result
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.recorder.record_tool_result(name, ok, outcome);
        Ok(result)
    }

    fn render_result(&self, result: Value) -> String {
        self.inner.render_result(result)
    }

    fn record_assistant(&self, text: &str) {
        self.inner.record_assistant(text);
    }

    fn refresh(&self) {
        self.inner.refresh();
    }

    fn set_confirm_actions(&self, enabled: bool) {
        self.inner.set_confirm_actions(enabled);
    }

    fn cancel_confirmation(&self) {
        self.inner.cancel_confirmation();
    }

    fn pending_notify_address(&self) -> Option<String> {
        self.inner.pending_notify_address()
    }
}
