//! Guion del escenario (`Driver` de `parity/replay/run_py.py`).
//!
//! El despacho de pasos es una tabla de [`Step`]: cada manejador reconoce su
//! clave del YAML y devuelve una espera ([`Pending`]). Los pasos de F7-01b/F7-01c
//! (setup, hc, agente, enjambre, chat, menú) se añaden a la tabla sin tocar el
//! bucle; un paso desconocido falla con su representación.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ez_core::config::truthy;
use ez_core::pyvalue::python_str;
use serde_json::{json, Map, Value};

use crate::app::core::VoiceApp;

use super::mic::ReplayMic;
use super::speaker::ReplaySpeaker;
use super::trace::Trace;

mod steps;

pub use steps::{handlers, Step};

/// Qué hay que esperar tras aplicar un paso.
pub enum Pending {
    /// El paso no espera (`_always`).
    Ready,
    /// `wait_state`.
    State(String),
    /// `wait_requests`.
    Requests(usize),
    /// `wait_speech`: frase reproducida y la barra ya no habla.
    Speech(String),
    /// `wait_transcript`.
    Transcript(String),
    /// `sleep`.
    Sleep(Instant),
    /// `wait_quiet`: plazo y desde cuándo está en calma.
    Quiet {
        deadline: Instant,
        since: Option<Instant>,
    },
    /// Predicado de un paso de F7-01b/F7-01c (`_predicate` del Python).
    Check(Box<dyn FnMut() -> Result<bool, String> + Send>),
}

/// Ejecuta los pasos del YAML; cada paso puede esperar una condición.
pub struct Driver {
    pub app: VoiceApp,
    pub mic: Arc<ReplayMic>,
    pub speaker: Arc<ReplaySpeaker>,
    pub trace: Arc<Trace>,
    pub requests_path: PathBuf,
    steps: Vec<Value>,
    index: usize,
    pending: Option<Pending>,
    done: bool,
}

impl Driver {
    /// Guion pendiente sobre la app ya arrancada.
    pub fn new(
        app: VoiceApp,
        mic: Arc<ReplayMic>,
        speaker: Arc<ReplaySpeaker>,
        trace: Arc<Trace>,
        requests_path: PathBuf,
        scenario: &Value,
    ) -> Self {
        let steps = scenario
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Self {
            app,
            mic,
            speaker,
            trace,
            requests_path,
            steps,
            index: 0,
            pending: None,
            done: false,
        }
    }

    /// `driver.done`.
    pub fn done(&self) -> bool {
        self.done
    }

    /// Una vuelta de `mainloop`: aplica un paso o comprueba la espera vigente.
    pub fn tick(&mut self, handlers: &[Box<dyn Step>]) -> Result<(), String> {
        if self.done {
            return Ok(());
        }
        if let Some(mut pending) = self.pending.take() {
            if !self.poll(&mut pending)? {
                self.pending = Some(pending);
            }
            return Ok(());
        }
        loop {
            let Some(step) = self.steps.get(self.index).cloned() else {
                self.done = true;
                return Ok(());
            };
            self.index += 1;
            let map = step
                .as_object()
                .cloned()
                .ok_or_else(|| format!("paso mal formado: {}", py_repr(&step)))?;
            let handler = handlers
                .iter()
                .find(|handler| handler.matches(&map))
                .ok_or_else(|| format!("paso desconocido: {}", py_repr(&step)))?;
            self.trace.record(
                "driver.step",
                json!({"index": self.index, "step": handler.describe(&map)}),
            );
            let pending = handler.apply(self, &map)?;
            // Los pasos instantáneos se encadenan, como el `mainloop` del Python.
            if matches!(pending, Some(Pending::Ready)) && handler.immediate() {
                continue;
            }
            self.pending = pending;
            return Ok(());
        }
    }

    /// Evalúa una espera; `true` cuando el guion puede seguir.
    fn poll(&mut self, pending: &mut Pending) -> Result<bool, String> {
        match pending {
            Pending::Ready => Ok(true),
            Pending::State(wanted) => Ok(self.app.state() == *wanted),
            Pending::Requests(wanted) => Ok(self.request_count() >= *wanted),
            Pending::Speech(text) => {
                Ok(self.speaker.finished(text) && self.app.state() != "speaking")
            }
            Pending::Transcript(wanted) => Ok(self
                .trace
                .with_event("stt.transcript")
                .iter()
                .any(|item| item.get("text").and_then(Value::as_str) == Some(wanted))),
            Pending::Sleep(deadline) => Ok(Instant::now() >= *deadline),
            Pending::Quiet { deadline, since } => self.poll_quiet(*deadline, since),
            Pending::Check(check) => check(),
        }
    }

    /// `wait_quiet`: listening, chat libre, micrófono en calma y sin hilos de turno.
    fn poll_quiet(&self, deadline: Instant, since: &mut Option<Instant>) -> Result<bool, String> {
        let now = Instant::now();
        if now > deadline {
            let recent: Vec<String> = self
                .trace
                .events()
                .iter()
                .rev()
                .take(8)
                .filter_map(|item| {
                    item.get("event")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            return Err(format!(
                "wait_quiet agotó su plazo (state={}, busy={}, mic={}, recientes={})",
                py_repr(&Value::String(self.app.state())),
                if self.app.inner.chat.chat.busy() {
                    "True"
                } else {
                    "False"
                },
                self.mic.diagnostic(),
                py_repr(&Value::from(recent))
            ));
        }
        let good = self.app.state() == "listening"
            && !self.app.inner.chat.chat.busy()
            && self.mic.idle()
            && !self.speaker.busy()
            && !self.busy_workers();
        if !good {
            *since = None;
            return Ok(false);
        }
        let began = *since.get_or_insert(now);
        Ok(now.duration_since(began).as_secs_f64() >= 0.25)
    }

    /// Hilos de turno aún vivos (`threading.enumerate` del Python).
    fn busy_workers(&self) -> bool {
        const BUSY: [&str; 5] = [
            "ezagent-answer",
            "ezagent-stt",
            "ezagent-tts",
            "ezagent-speech",
            "ezagent-wake",
        ];
        self.app.inner.runtime.workers.lock().iter().any(|handle| {
            !handle.is_finished()
                && handle
                    .thread()
                    .name()
                    .is_some_and(|name| BUSY.iter().any(|prefix| name.starts_with(prefix)))
        })
    }

    /// Líneas de `requests.jsonl` del falso LLM.
    fn request_count(&self) -> usize {
        std::fs::read_to_string(&self.requests_path)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    /// Estado final que lee el arnés (como `Driver.snapshot`).
    pub fn snapshot(&self) -> Snapshot {
        let messages = self
            .app
            .inner
            .chat
            .chat
            .snapshot()
            .1
            .into_iter()
            .map(|message| (message.role.as_str().to_string(), message.text))
            .collect();
        Snapshot {
            state: self.app.state(),
            in_call: self
                .app
                .inner
                .session
                .in_call
                .load(std::sync::atomic::Ordering::SeqCst),
            muted: self
                .app
                .inner
                .session
                .muted
                .load(std::sync::atomic::Ordering::SeqCst),
            messages,
            speech: texts(&self.trace, "speech.say"),
            transcripts: texts(&self.trace, "stt.transcript"),
            config: self.config_snapshot(),
            history: self.history_snapshot(),
            swarm: self.swarm_snapshot(),
        }
    }

    /// `snap["config"]`: adjustes visibles del menú.
    fn config_snapshot(&self) -> Map<String, Value> {
        const KEYS: [&str; 9] = [
            "voice",
            "speed",
            "llm_provider",
            "llm_model",
            "llm_api_style",
            "hotkey",
            "chat_voice",
            "smart_turn",
            "silence_secs",
        ];
        let cfg = self.app.config();
        let mut values = Map::new();
        for key in KEYS {
            values.insert(
                key.to_string(),
                cfg.get(key).cloned().unwrap_or(Value::Null),
            );
        }
        values
    }

    /// `snap["history"]`: `sesión:rol:texto` de todas las conversaciones.
    fn history_snapshot(&self) -> Vec<String> {
        let chat = &self.app.inner.chat.chat;
        let Some(store) = chat.store() else {
            return Vec::new();
        };
        let mut sessions = store.sessions();
        sessions.sort_by_key(|info| info.session);
        let mut out = Vec::new();
        for info in sessions {
            for entry in store.entries(Some(info.session)) {
                out.push(format!(
                    "{}:{}:{}",
                    entry.session,
                    entry.role.as_str(),
                    entry.text
                ));
            }
        }
        out
    }

    /// `snap["swarm"]`: vista del enjambre para las aserciones.
    fn swarm_snapshot(&self) -> Option<Value> {
        if self.app.inner.agents.swarm.lock().is_none() || super::swarm_c::swarm_id().is_empty() {
            return None;
        }
        match super::swarm_c::view() {
            Ok(view) => Some(view),
            Err(error) => {
                tracing::warn!("No pude leer el enjambre para la traza: {error}");
                None
            }
        }
    }
}

/// Instantánea final del guion (`Driver.snapshot`).
pub struct Snapshot {
    pub state: String,
    pub in_call: bool,
    pub muted: bool,
    pub messages: Vec<(String, String)>,
    pub speech: Vec<String>,
    pub transcripts: Vec<String>,
    pub config: Map<String, Value>,
    pub history: Vec<String>,
    pub swarm: Option<Value>,
}

impl Snapshot {
    /// Mensajes de un rol, en orden.
    pub fn texts_for(&self, role: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|(candidate, _)| candidate == role)
            .map(|(_, text)| text.clone())
            .collect()
    }

    /// `final` del escenario: estado, llamada y silencio.
    pub fn matches_final(&self, final_spec: &Map<String, Value>) -> Vec<String> {
        let mut errors = Vec::new();
        let values = [
            ("state", Value::String(self.state.clone())),
            ("in_call", Value::Bool(self.in_call)),
            ("muted", Value::Bool(self.muted)),
        ];
        for (key, value) in values {
            if let Some(expected) = final_spec.get(key) {
                if expected != &value {
                    errors.push(format!(
                        "final {key}={} != {}",
                        super::expect::py_repr(&value),
                        super::expect::py_repr(expected)
                    ));
                }
            }
        }
        errors
    }
}

fn texts(trace: &Trace, event: &str) -> Vec<String> {
    trace
        .with_event(event)
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// `repr(dict)` de un paso: `{'clave': valor}`.
fn py_repr(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("'{key}': {}", py_repr(item)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::String(text) => format!("'{text}'"),
        other => python_str(other),
    }
}

/// `Pending::Ready` para el paso que no espera.
pub fn ready() -> Option<Pending> {
    Some(Pending::Ready)
}

/// ¿El paso tiene esta clave con un valor "verdadero" de Python?
pub fn flag(step: &Map<String, Value>, key: &str) -> bool {
    step.get(key).is_some_and(truthy)
}

/// Valor escalar de un paso, como texto.
pub fn text(step: &Map<String, Value>, key: &str) -> String {
    step.get(key).map(python_str).unwrap_or_default()
}

/// `_describe` del Python para el paso que maneja `key`.
pub fn describe_first(step: &Map<String, Value>, key: &str) -> String {
    format!(
        "{key}:{}",
        step.get(key).map(python_str).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_step_reports_python_repr() {
        let value = json!({"hud": {"action": "chat_send", "text": "hola"}});
        assert_eq!(
            py_repr(&value),
            "{'hud': {'action': 'chat_send', 'text': 'hola'}}"
        );
        let number = json!({"wait_quiet": 25});
        assert_eq!(py_repr(&number), "{'wait_quiet': 25}");
    }

    #[test]
    fn describe_matches_the_python_text() {
        let step = json!({"wait_state": "listening"});
        let map = step.as_object().expect("objeto");
        assert_eq!(describe_first(map, "wait_state"), "wait_state:listening");
        let sleep = json!({"sleep": 0.5});
        assert_eq!(
            describe_first(sleep.as_object().expect("objeto"), "sleep"),
            "sleep:0.5"
        );
    }
}
