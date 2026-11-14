//! Port of the context recovery of `Messenger in src/ezagent/voice/llm.py` (F4-07).
//!
//! Abre el stream con reintentos cuando el servidor rechaza los argumentos de
//! herramienta del historial, mantiene la conversación dentro de la ventana de
//! contexto y pide el resumen de lo antiguo al propio modelo.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use ez_core::pyvalue::{head_chars, py_strip};
use serde_json::{json, Value};

use crate::context;
use crate::openai::ChatStream;
use crate::tool_json::sanitize_history;

use super::request::{is_tool_error, with_volatile};
use super::{ActiveStream, Messenger, RuntimeFailure, TurnError, TurnToken, TURN_TIMEOUT_TEXT};

/// Aplica el plazo del turno al `timeout` de la petición.
fn apply_deadline(
    request: &mut Value,
    timeout: f64,
    deadline: Option<Instant>,
) -> Result<(), TurnError> {
    let Some(deadline) = deadline else {
        return Ok(());
    };
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .as_secs_f64();
    if remaining <= 0.0 {
        return Err(TurnError::Timeout(TURN_TIMEOUT_TEXT.to_string()));
    }
    request["timeout"] = json!(timeout.min(remaining).max(1.0));
    Ok(())
}

impl Messenger {
    /// `_open_stream`: abre la petición y repara el historial si el servidor
    /// rechaza los argumentos de herramientas.
    pub(crate) fn open_stream(
        &self,
        request: &mut Value,
        token: &TurnToken,
        deadline: Option<Instant>,
    ) -> Result<Box<dyn ChatStream>, TurnError> {
        let extra_messages = request
            .get("_notices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let volatile = request
            .get("_volatile")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(map) = request.as_object_mut() {
            map.shift_remove("_notices");
            map.shift_remove("_volatile");
        }
        let client = self.client();
        let timeout = self.settings().request_timeout();
        for attempt in 0..3 {
            self.check_cancel(token)?;
            apply_deadline(request, timeout, deadline)?;
            match client.stream(request) {
                Ok(stream) => return Ok(stream),
                Err(error) => {
                    if token.is_cancelled() {
                        return Err(TurnError::cancelled("turno cancelado por el usuario"));
                    }
                    if attempt == 0 && is_tool_error(&error) {
                        tracing::warn!(
                            "El servidor rechazó argumentos de herramienta del historial: {error}"
                        );
                        let fixed = {
                            let mut history = self.history();
                            sanitize_history(&mut history.messages)
                        };
                        // El Python muta los mismos dicts que ya viajaron en la
                        // petición; aquí se reconstruye la lista corregida.
                        let mut messages = with_volatile(&self.messages(), &volatile);
                        messages.extend(extra_messages.iter().cloned());
                        request["messages"] = json!(messages);
                        tracing::info!("Reparé {} llamada(s) del historial y reintento.", fixed);
                        continue;
                    }
                    if attempt == 1 && is_tool_error(&error) {
                        self.strip_tool_history();
                        let mut messages = with_volatile(&self.messages(), &volatile);
                        messages.extend(extra_messages.iter().cloned());
                        request["messages"] = json!(messages);
                        tracing::warn!(
                            "Último recurso: reintento sin las llamadas a herramientas anteriores."
                        );
                        continue;
                    }
                    return Err(TurnError::Llm(error));
                }
            }
        }
        Err(TurnError::Runtime(RuntimeFailure::new(
            "RuntimeError",
            "No pude abrir el stream del proveedor.",
        )))
    }

    /// `_strip_tool_history`: quita llamadas y resultados; queda lo conversado.
    pub(crate) fn strip_tool_history(&self) {
        let mut history = self.history();
        let mut kept: Vec<Value> = Vec::new();
        if let Some(system) = history.messages.first() {
            kept.push(system.clone());
        }
        for message in history.messages.iter().skip(1) {
            if message.get("role").and_then(Value::as_str) == Some("tool") {
                continue;
            }
            if message.get("tool_calls").is_some_and(|calls| {
                !calls.is_null() && calls.as_array().is_none_or(|items| !items.is_empty())
            }) {
                let content = message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !py_strip(&content).is_empty() {
                    kept.push(json!({"role": "assistant", "content": content}));
                }
                continue;
            }
            kept.push(message.clone());
        }
        history.messages = kept;
    }

    /// `_manage_context`: mantiene la conversación dentro de la ventana configurada.
    pub(crate) fn manage_context(
        &self,
        tools: &[Value],
        max_tokens: i64,
        token: &Arc<TurnToken>,
        volatile: &str,
    ) -> Result<(), TurnError> {
        let settings = self.settings();
        let budget = context::budget_tokens(&json!(settings.context_tokens));
        let schemas = if tools.is_empty() { None } else { Some(tools) };
        let mut reserve = context::reserve_tokens(schemas, &json!(max_tokens));
        if !py_strip(volatile).is_empty() {
            reserve += context::estimate_tokens(&[json!({"role": "user", "content": volatile})]);
        }
        let needs_clipping = {
            let history = self.history();
            context::needs_clipping(&history.messages, budget)
        };
        if needs_clipping {
            let mut history = self.history();
            context::clip_old_tool_results(&mut history.messages, 1);
        }
        let needs_compaction = {
            let history = self.history();
            context::needs_compaction(&history.messages, budget, reserve)
        };
        if needs_compaction {
            self.compact(budget, reserve, token)?;
        }
        Ok(())
    }

    /// `_compact`: resume lo antiguo y deja solo las vueltas recientes.
    pub(crate) fn compact(
        &self,
        budget: i64,
        reserve: i64,
        token: &Arc<TurnToken>,
    ) -> Result<(), TurnError> {
        let (old, recent) = {
            let history = self.history();
            context::fit_recent(&history.messages, budget, reserve)
        };
        if old.is_empty() {
            // Una sola vuelta (enorme): no hay nada que resumir; como último
            // recurso se recortan incluso los resultados recientes.
            let mut history = self.history();
            context::clip_old_tool_results(&mut history.messages, 0);
            return Ok(());
        }
        let before = {
            let history = self.history();
            context::estimate_tokens(&history.messages)
        };
        self.notify_context("Compactando la conversación para no llenar el contexto…");
        let summary = match self.summarize(&old, token) {
            Ok(summary) => summary,
            Err(TurnError::Cancelled(message)) => return Err(TurnError::Cancelled(message)),
            Err(error) => {
                tracing::warn!("No pude resumir la conversación: {error}");
                String::new()
            }
        };
        {
            let mut history = self.history();
            if !summary.is_empty() {
                history.summary = summary.clone();
            }
            let system = history.messages.first().cloned();
            history.messages = system.into_iter().chain(recent).collect();
        }
        let after = {
            let history = self.history();
            context::estimate_tokens(&history.messages)
        };
        tracing::info!(
            "Contexto compactado: {} -> {}",
            context::describe(before, budget),
            context::describe(after, budget)
        );
        self.notify_context(if summary.is_empty() {
            "No pude resumir lo anterior; lo más antiguo se olvidó para seguir."
        } else {
            "Conversación compactada: lo anterior queda resumido."
        });
        Ok(())
    }

    /// `_summarize`: pide al propio modelo un resumen de `old`.
    pub(crate) fn summarize(
        &self,
        old: &[Value],
        token: &Arc<TurnToken>,
    ) -> Result<String, TurnError> {
        let previous = self.summary();
        let messages = context::summary_messages(old, &previous);
        let text = self.complete_text(&messages, context::SUMMARY_TOKENS as u32, token)?;
        Ok(head_chars(py_strip(&text), 6000).to_string())
    }

    /// `_complete_text`: una respuesta de texto sin herramientas.
    pub(crate) fn complete_text(
        &self,
        messages: &[Value],
        max_tokens: u32,
        token: &Arc<TurnToken>,
    ) -> Result<String, TurnError> {
        let settings = self.settings();
        let caps = self.caps(&settings);
        let mut request = json!({
            "model": settings.model,
            "messages": messages,
            "max_tokens": max_tokens.min(caps.max_output),
            "stream": true,
        });
        if caps.temperature {
            request["temperature"] =
                if settings.provider == "anthropic" && !settings.anthropic_style() {
                    json!(1.0)
                } else {
                    json!(0.2)
                };
        }
        let stream = self.client().stream(&request).map_err(TurnError::Llm)?;
        let active = Arc::new(ActiveStream {
            token: token.clone(),
            stream: Mutex::new(stream),
        });
        *super::lock(&self.shared.active_stream) = Some(active.clone());
        let outcome = self.drain_text_stream(&active, token);
        {
            let mut slot = super::lock(&self.shared.active_stream);
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &active))
            {
                *slot = None;
            }
        }
        super::lock(&active.stream).close();
        outcome
    }

    /// Lee el streaming entero de una respuesta de texto.
    fn drain_text_stream(
        &self,
        active: &Arc<ActiveStream>,
        token: &Arc<TurnToken>,
    ) -> Result<String, TurnError> {
        let mut parts = String::new();
        loop {
            if token.is_cancelled() {
                return Err(TurnError::cancelled("turno cancelado por el usuario"));
            }
            match super::lock(&active.stream).next() {
                None => return Ok(parts),
                Some(Ok(chunk)) => {
                    if let Some(content) = chunk.content {
                        parts.push_str(&content);
                    }
                }
                Some(Err(error)) => {
                    return Err(if token.is_cancelled() {
                        TurnError::cancelled("turno cancelado por el usuario")
                    } else {
                        TurnError::Llm(error)
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use ez_core::pyvalue::py_strip;
    use serde_json::{json, Value};

    use super::super::tool_loop::tests::{messenger_with, text, FakeClient, Script};
    use super::*;
    use crate::errors::LlmError;

    fn token() -> Arc<TurnToken> {
        Arc::new(TurnToken::new())
    }

    fn requests(client: &FakeClient) -> Vec<Value> {
        client
            .requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[test]
    fn strip_tool_history_keeps_the_conversation() {
        let (messenger, _) = messenger_with(Vec::new(), None, &[]);
        messenger.set_messages(vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "u"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "c1", "function": {"name": "x", "arguments": "{}"}}
            ]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "r"}),
            json!({"role": "assistant", "content": "a"}),
        ]);
        messenger.strip_tool_history();
        let messages = messenger.messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"], "u");
        assert_eq!(messages[2]["content"], "a");
    }

    #[test]
    fn open_stream_repairs_broken_arguments_and_retries() {
        let (messenger, client) = messenger_with(
            vec![
                Script::Fail(LlmError::http(
                    500,
                    "Tool call arguments failed to parse as JSON",
                )),
                Script::Chunks(vec![text("ok")]),
            ],
            None,
            &[],
        );
        messenger.set_messages(vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "u"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "c1", "function": {"name": "x", "arguments": "{"}}
            ]}),
        ]);
        let mut request = json!({"model": "m", "messages": []});
        let stream = messenger
            .open_stream(&mut request, &token(), None)
            .expect("reintento correcto");
        drop(stream);
        assert_eq!(requests(&client).len(), 2);
        let retried = requests(&client)[1]["messages"][2]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("argumentos del reintento")
            .to_string();
        assert!(serde_json::from_str::<Value>(&retried).is_ok());
        let arguments = messenger.messages()[2]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("argumentos")
            .to_string();
        assert!(serde_json::from_str::<Value>(&arguments).is_ok());
    }

    #[test]
    fn open_stream_last_resort_drops_the_tool_history() {
        let error = || {
            Script::Fail(LlmError::http(
                500,
                "Tool call arguments failed to parse as JSON",
            ))
        };
        let (messenger, client) = messenger_with(
            vec![error(), error(), Script::Chunks(vec![text("ok")])],
            None,
            &[],
        );
        messenger.set_messages(vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "u"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "c1", "function": {"name": "x", "arguments": "{}"}}
            ]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "r"}),
        ]);
        let mut request = json!({"model": "m", "messages": []});
        let stream = messenger
            .open_stream(&mut request, &token(), None)
            .expect("último recurso");
        drop(stream);
        assert_eq!(requests(&client).len(), 3);
        assert!(messenger
            .messages()
            .iter()
            .all(|message| message["role"] != "tool"));
    }

    #[test]
    fn open_stream_with_an_expired_deadline_times_out() {
        let (messenger, _) = messenger_with(Vec::new(), None, &[]);
        let mut request = json!({"model": "m", "messages": []});
        let deadline = Instant::now() - Duration::from_secs(1);
        let error = messenger
            .open_stream(&mut request, &token(), Some(deadline))
            .err()
            .expect("plazo vencido");
        assert!(matches!(error, TurnError::Timeout(_)));
    }

    #[test]
    fn complete_text_reads_a_streamed_reply() {
        let (messenger, client) =
            messenger_with(vec![Script::Chunks(vec![text("1"), text("2")])], None, &[]);
        let reply = messenger
            .complete_text(&[json!({"role": "user", "content": "x"})], 8, &token())
            .expect("respuesta");
        assert_eq!(reply, "12");
        let requests = requests(&client);
        assert_eq!(requests[0]["max_tokens"], 8);
        assert_eq!(requests[0]["temperature"], 0.2);
    }

    #[test]
    fn manage_context_clips_old_tool_results() {
        let (messenger, _) =
            messenger_with(Vec::new(), None, &[("llm_context_tokens", json!(4000))]);
        let long = "x".repeat(20_000);
        messenger.set_messages(vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "antiguo"}),
            json!({"role": "tool", "tool_call_id": "c1", "content": long}),
            json!({"role": "assistant", "content": "a"}),
            json!({"role": "user", "content": "reciente"}),
        ]);
        messenger
            .manage_context(&[], 100, &token(), "")
            .expect("contexto");
        let content = messenger.messages()[2]["content"]
            .as_str()
            .expect("resultado")
            .to_string();
        assert!(
            content.contains("… [recortado: 18500 caracteres]"),
            "contenido recortado: {content}"
        );
    }

    #[test]
    fn compact_summarizes_and_notifies() {
        let (messenger, client) = messenger_with(
            vec![Script::Chunks(vec![text("resumen del pasado")])],
            None,
            &[],
        );
        let mut messages = vec![json!({"role": "system", "content": "s"})];
        for index in 0..6 {
            messages
                .push(json!({"role": "user", "content": format!("{index}{}", "x".repeat(2000))}));
            messages.push(json!({"role": "assistant", "content": "respuesta"}));
        }
        messenger.set_messages(messages);
        let notes: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let collected = notes.clone();
        messenger.set_on_context_event(Arc::new(move |text: &str| {
            collected
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(text.to_string());
        }));
        messenger.compact(4000, 100, &token()).expect("compactar");
        assert_eq!(messenger.summary(), "resumen del pasado");
        let messages = messenger.messages();
        assert!(messages.len() < 13);
        assert_eq!(messages[0]["role"], "system");
        let last = messages.last().expect("último mensaje");
        assert!(py_strip(last["content"].as_str().unwrap_or("")).len() <= 2001);
        assert_eq!(requests(&client).len(), 1);
        let notes = notes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("Compactando"));
        assert!(notes[1].contains("Conversación compactada"));
    }
}
