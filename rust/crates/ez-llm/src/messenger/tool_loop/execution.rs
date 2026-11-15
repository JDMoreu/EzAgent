//! Ejecución de herramientas y reparación del historial del `Messenger`
//! (F4-07/F4-08).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ez_core::pyjson::{dumps, dumps_with, DumpOptions};
use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{json, Value};

use crate::tool_json::repair_structure;

use super::super::request::remaining_tools;
use super::super::{
    AgentRuntimeApi, Messenger, RuntimeFailure, TurnError, TurnToken, CANCELLED_TOOL_RESULT,
};

pub(super) fn call_name(call: &Value) -> &str {
    call["function"]["name"].as_str().unwrap_or("")
}

pub(super) fn call_arguments(call: &Value) -> &str {
    call["function"]["arguments"].as_str().unwrap_or("")
}

pub(super) fn append_text(value: &mut Value, extra: &str) {
    let current = value.as_str().unwrap_or("");
    *value = Value::String(format!("{current}{extra}"));
}

impl Messenger {
    /// `_run_tool`: presupuesto, argumentos completos, caché de lecturas web y ejecución.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_tool(
        &self,
        runtime: Option<&Arc<dyn AgentRuntimeApi>>,
        call: &Value,
        complete: bool,
        cache: &mut HashMap<(String, String), String>,
        allow: bool,
        token: &TurnToken,
    ) -> Result<(String, bool), TurnError> {
        self.check_cancel(token)?;
        if !allow || remaining_tools(runtime) == Some(0) {
            return Ok((
                dumps(&json!({"ok": false, "error": "Finaliza con los resultados disponibles."})),
                true,
            ));
        }
        let Some(runtime) = runtime else {
            return Err(TurnError::Runtime(RuntimeFailure::new(
                "RuntimeError",
                "el núcleo agéntico no está disponible",
            )));
        };
        if !complete {
            return Ok((
                dumps(&json!({
                    "ok": false,
                    "error": "Los argumentos de la herramienta llegaron incompletos \
                (la respuesta se cortó). Repite la llamada con menos texto."
                })),
                false,
            ));
        }
        let name = call_name(call).to_string();
        let raw = call_arguments(call).to_string();
        let key = if name == "web_search" || name == "fetch_page" {
            let parsed = pyloads::loads(&raw).map_err(|error| {
                TurnError::Runtime(RuntimeFailure::new("JSONDecodeError", error.to_string()))
            })?;
            let options = DumpOptions {
                sort_keys: true,
                ensure_ascii: false,
                ..DumpOptions::default()
            };
            Some((name.clone(), dumps_with(&parsed, &options)))
        } else {
            None
        };
        if let Some(key) = &key {
            // DECISION (del Python): solo se reutilizan lecturas web idénticas
            // del turno; nunca escrituras, confirmaciones ni comandos.
            if let Some(cached) = cache.get(key) {
                return Ok((cached.clone(), true));
            }
        }
        let content = runtime.render_result(runtime.execute(&name, &raw)?);
        if let Some(key) = key {
            cache.insert(key, content.clone());
        }
        Ok((content, false))
    }

    /// `_record_answer`: guarda la respuesta y la pasa a la memoria del runtime.
    pub(super) fn record_answer(&self, content: &str, runtime: Option<&Arc<dyn AgentRuntimeApi>>) {
        self.history()
            .messages
            .push(json!({"role": "assistant", "content": content}));
        if let Some(runtime) = runtime {
            if !content.is_empty() {
                runtime.record_assistant(content);
            }
        }
    }

    /// `_close_interrupted`: deja el historial válido tras un corte.
    ///
    /// DECISION: sin el `try/except` del Python; `repair_structure` no puede
    /// fallar en Rust, así que no hay nada que atrapar.
    pub fn close_interrupted(
        &self,
        partial: &[String],
        open_calls: &[Value],
        assistant_appended: bool,
    ) {
        let mut history = self.history();
        if !partial.is_empty() && !assistant_appended {
            let joined = partial.join("");
            let text = py_strip(&joined);
            if !text.is_empty() {
                history.messages.push(json!({
                    "role": "assistant",
                    "content": format!("{text} [interrumpido]"),
                }));
            }
        }
        if !open_calls.is_empty() {
            let mut have: HashSet<String> = history
                .messages
                .iter()
                .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
                .map(|message| {
                    message
                        .get("tool_call_id")
                        .filter(|value| truthy(value))
                        .map_or_else(String::new, python_str)
                })
                .collect();
            for call in open_calls {
                let call_id = call
                    .get("id")
                    .filter(|value| truthy(value))
                    .map_or_else(String::new, python_str);
                if !call_id.is_empty() && !have.contains(&call_id) {
                    history.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": CANCELLED_TOOL_RESULT,
                    }));
                    have.insert(call_id);
                }
            }
        }
        repair_structure(&mut history.messages);
    }
}
