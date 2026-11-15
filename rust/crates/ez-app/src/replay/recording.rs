//! Registrador de las costuras de la app (`RecordingHud` e `instrument_chat`).
//!
//! El estado que ve el usuario se anota solo cuando cambia, igual que el doble
//! de Python: `_set_state` repite el mismo estado a propósito y la barra vuelve
//! a fijar `in_call` en cada transición.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;

use crate::app::core::AppTrace;

use super::trace::Trace;

#[derive(Default)]
struct HudState {
    state: Option<String>,
    in_call: Option<bool>,
    muted: Option<bool>,
    confirm: bool,
}

/// Puente entre los ganchos de `ez-app` y la traza del arnés.
pub struct Recorder {
    trace: Arc<Trace>,
    chat_file: String,
    hud: Mutex<HudState>,
}

impl Recorder {
    /// Registrador sobre `trace`; el nombre de `chat.jsonl` sale del historial.
    pub fn new(trace: Arc<Trace>, chat_path: Option<&Path>) -> Arc<Self> {
        Arc::new(Self {
            trace,
            chat_file: chat_path.and_then(Path::file_name).map_or_else(
                || "chat.jsonl".to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            hud: Mutex::new(HudState::default()),
        })
    }

    /// Traza compartida (el arnés también escribe en ella).
    pub fn trace(&self) -> &Arc<Trace> {
        &self.trace
    }

    /// `tool.call`: el Python lo registra en el hilo de la respuesta.
    pub fn record_tool_call(&self, name: &str, args: &str) {
        self.trace.record_as(
            "ezagent-answer",
            "tool.call",
            json!({"name": name, "args": args}),
        );
    }

    /// `tool.result`, en el mismo hilo que la llamada.
    pub fn record_tool_result(&self, name: &str, ok: bool, outcome: &str) {
        self.trace.record_as(
            "ezagent-answer",
            "tool.result",
            json!({"name": name, "ok": ok, "outcome": outcome}),
        );
    }
}

impl AppTrace for Recorder {
    fn hud_show(&self, state: &str) {
        let mut hud = self.hud.lock();
        if hud.state.as_deref() == Some(state) {
            return;
        }
        hud.state = Some(state.to_string());
        drop(hud);
        self.trace.record("hud.show", json!({"state": state}));
    }

    fn hud_hide(&self) {
        self.trace.record("hud.hide", json!({}));
    }

    fn hud_in_call(&self, value: bool) {
        let mut hud = self.hud.lock();
        if hud.in_call == Some(value) {
            return;
        }
        hud.in_call = Some(value);
        drop(hud);
        self.trace.record("hud.in_call", json!({"value": value}));
    }

    fn hud_muted(&self, value: bool) {
        let mut hud = self.hud.lock();
        if hud.muted == Some(value) {
            return;
        }
        hud.muted = Some(value);
        drop(hud);
        self.trace.record("hud.muted", json!({"value": value}));
    }

    fn hud_notice(&self) {
        self.trace.record("hud.notice", json!({}));
    }

    fn hud_confirm(&self, summary: &str, strong: bool) {
        self.hud.lock().confirm = true;
        self.trace
            .record("hud.confirm", json!({"summary": summary, "strong": strong}));
    }

    fn hud_confirm_done(&self) {
        let mut hud = self.hud.lock();
        if !hud.confirm {
            return;
        }
        hud.confirm = false;
        drop(hud);
        self.trace.record("hud.confirm_done", json!({}));
    }

    fn hud_testing(&self, value: bool) {
        self.trace.record("hud.testing", json!({"value": value}));
    }

    fn hud_close(&self) {
        self.trace.record("hud.close", json!({}));
    }

    fn chat_user(&self, text: &str, source: &str) {
        self.trace
            .record("chat.user", json!({"text": text, "source": source}));
    }

    fn chat_assistant(&self, text: &str) {
        self.trace.record("chat.assistant", json!({"text": text}));
    }

    fn chat_cancel(&self, text: &str) {
        self.trace.record("chat.cancel", json!({"text": text}));
    }

    fn chat_fail(&self, text: &str) {
        self.trace.record("chat.fail", json!({"text": text}));
    }

    fn chat_note(&self, text: &str) {
        self.trace.record("chat.note", json!({"text": text}));
    }

    fn chat_confirm(&self, summary: &str, strong: bool) {
        self.trace.record(
            "chat.confirm",
            json!({"summary": summary, "strong": strong}),
        );
    }

    fn chat_confirm_done(&self) {
        self.trace.record("chat.confirm_done", json!({}));
    }

    fn chat_file_write(&self, role: &str) {
        self.trace
            .record("file.write", json!({"name": self.chat_file, "role": role}));
    }

    fn config_file_write(&self, name: &str) {
        self.trace.record("file.write", json!({"name": name}));
    }

    fn degraded(&self, what: &str) {
        self.trace.record(&format!("degraded.{what}"), json!({}));
    }

    fn stt_transcript(&self, text: &str) {
        self.trace.record("stt.transcript", json!({"text": text}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_transitions_are_recorded_once() {
        let trace = Trace::new(Path::new("/tmp/corrida"));
        let recorder = Recorder::new(Arc::clone(&trace), None);
        recorder.hud_in_call(false);
        recorder.hud_in_call(false);
        recorder.hud_show("idle");
        recorder.hud_show("idle");
        recorder.hud_show("listening");
        recorder.hud_confirm("Borro la nota", true);
        recorder.hud_confirm_done();
        recorder.hud_confirm_done();
        recorder.chat_file_write("user");
        let events: Vec<String> = trace
            .lines()
            .iter()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|item| {
                item.get("event")
                    .and_then(|event| event.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(
            events,
            vec![
                "hud.in_call",
                "hud.show",
                "hud.show",
                "hud.confirm",
                "hud.confirm_done",
                "file.write",
            ]
        );
    }
}
