//! Port of the tool loop of `Messenger in src/ezagent/voice/llm.py` (F4-08).
//!
//! `ask_stream` abre un hilo por turno y entrega los fragmentos por un canal;
//! el bucle alterna respuesta del modelo y ejecución de herramientas, avisa una
//! vez si una herramienta lenta no se anunció y deja el historial válido si el
//! turno se corta.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, TryLockError};
use std::time::{Duration, Instant};

use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{Map, Value};

use super::{
    Messenger, RuntimeFailure, TurnError, TurnToken, BUSY_POLL_SECS, BUSY_WAIT_SECS, EMPTY_ANSWER,
};

mod execution;
#[cfg(test)]
pub(crate) mod tests;
mod turn;

/// Modo del turno: voz (respuestas cortas) o chat (markdown y textos largos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AskMode {
    /// `mode="voice"`.
    #[default]
    Voice,
    /// `mode="chat"`.
    Chat,
}

impl AskMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Voice => "voice",
            Self::Chat => "chat",
        }
    }
}

/// Fragmentos de un turno, cancelables con [`Messenger::cancel_turn`].
///
/// Si se suelta a mitad, el turno se corta y el historial queda reparado, como
/// el `GeneratorExit` del generador de Python.
///
/// DECISION: el ítem es `Result<String, TurnError>` (no `String`) para que los
/// fallos del proveedor y la cancelación no se pierdan en silencio al iterar.
pub struct AskStream {
    receiver: Receiver<Result<String, TurnError>>,
    messenger: Messenger,
    token: Arc<TurnToken>,
    generation: u64,
    finished: bool,
}

impl Iterator for AskStream {
    type Item = Result<String, TurnError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.recv() {
            Ok(Ok(text)) => Some(Ok(text)),
            Ok(Err(error)) => {
                self.finished = true;
                Some(Err(error))
            }
            Err(_) => {
                self.finished = true;
                None
            }
        }
    }
}

impl Drop for AskStream {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.token.cancel();
        let shared = &self.messenger.shared;
        if shared
            .turn_generation
            .load(std::sync::atomic::Ordering::SeqCst)
            == self.generation
        {
            shared
                .turn_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let active = super::lock(&shared.active_stream).clone();
        if let Some(active) = active.filter(|active| Arc::ptr_eq(&active.token, &self.token)) {
            if let Ok(mut stream) = active.stream.try_lock() {
                stream.close();
            }
        }
    }
}

/// `_AGENT_NAMES`: nombre visible de los agentes delegados.
const AGENT_NAMES: [(&str, &str); 3] = [
    ("claude", "Claude Code"),
    ("opencode", "OpenCode"),
    ("codex", "Codex"),
];

/// Frase corta que anuncia una herramienta lenta, o `None` si no hace falta.
pub fn tool_ack(name: &str, raw_args: &str) -> Option<String> {
    let raw = if raw_args.is_empty() { "{}" } else { raw_args };
    let args = match pyloads::loads(raw) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    match name {
        "web_search" => Some("Busco en la web.".to_string()),
        "fetch_page" => Some("Abro la página.".to_string()),
        "run_skill_script" => Some("Ejecuto el script.".to_string()),
        "delegate_to_agent" => {
            let action = args
                .get("action")
                .filter(|value| truthy(value))
                .map_or_else(String::new, python_str);
            if action == "send" {
                return Some("Le escribo al agente.".to_string());
            }
            if action != "run" {
                return None;
            }
            let agent = args
                .get("agent")
                .filter(|value| truthy(value))
                .map_or_else(String::new, python_str);
            let agent = py_strip(&agent).to_lowercase();
            let label = AGENT_NAMES
                .iter()
                .find(|(key, _)| *key == agent.as_str())
                .map_or(agent.as_str(), |(_, label)| *label);
            if label.is_empty() {
                Some("Se lo paso a un agente.".to_string())
            } else {
                Some(format!("Se lo paso a {label}."))
            }
        }
        _ => None,
    }
}

impl Messenger {
    /// `ask`: respuesta completa ya limpia, o el mensaje de respaldo.
    pub fn ask(&self, text: &str) -> Result<String, TurnError> {
        let answer: String = self
            .ask_stream(text, AskMode::Voice)
            .collect::<Result<_, _>>()?;
        let answer = py_strip(&answer);
        if answer.is_empty() || answer == EMPTY_ANSWER {
            return Ok("No he podido completar la consulta.".to_string());
        }
        Ok(answer.to_string())
    }

    /// `ask_stream`: streaming de la respuesta, incluidas las herramientas.
    ///
    /// Un turno nuevo invalida el anterior y espera a que suelte el lock. Si la
    /// espera supera [`BUSY_WAIT_SECS`], avisa una vez por `on_busy` y sigue
    /// esperando. `mode="chat"` permite markdown y respuestas largas, y no
    /// añade acuses hablados.
    pub fn ask_stream(&self, text: &str, mode: AskMode) -> AskStream {
        self.cancel_turn();
        let generation = self
            .shared
            .turn_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let token = Arc::new(TurnToken::new());
        *super::lock(&self.shared.turn_token) = Some(token.clone());
        // DECISION: un hilo por turno (`ezagent-turn`, el mismo nombre que usa
        // el Python) en vez de una máquina de estados; el que llama no se bloquea.
        let (sender, receiver) = mpsc::channel();
        let error_sender = sender.clone();
        let messenger = self.clone();
        let thread_token = token.clone();
        let thread_text = text.to_string();
        let spawned = std::thread::Builder::new()
            .name("ezagent-turn".to_string())
            .spawn(move || {
                messenger.run_worker(thread_text, mode, thread_token, generation, sender);
            });
        if let Err(error) = spawned {
            token.cancel();
            let _ = error_sender.send(Err(TurnError::Runtime(RuntimeFailure::new(
                "RuntimeError",
                format!("No pude iniciar el turno: {error}"),
            ))));
            self.clear_turn_token(&token);
        }
        AskStream {
            receiver,
            messenger: self.clone(),
            token,
            generation,
            finished: false,
        }
    }

    /// Hilo del turno: espera el lock, corre el turno y limpia el estado.
    fn run_worker(
        &self,
        text: String,
        mode: AskMode,
        token: Arc<TurnToken>,
        generation: u64,
        out: Sender<Result<String, TurnError>>,
    ) {
        let guard = match self.acquire_turn(generation, mode) {
            Ok(guard) => guard,
            Err(error) => {
                self.clear_turn_token(&token);
                let _ = out.send(Err(error));
                return;
            }
        };
        let current = self
            .shared
            .turn_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let outcome = if current == generation {
            self.drive_turn(&text, mode, &token, &out)
        } else {
            Err(TurnError::cancelled("turno cancelado mientras esperaba"))
        };
        self.history().turn_active = false;
        self.apply_deferred_history();
        *super::lock(&self.shared.active_stream) = None;
        self.clear_turn_token(&token);
        drop(guard);
        if let Err(error) = outcome {
            let _ = out.send(Err(error));
        }
    }

    fn clear_turn_token(&self, token: &Arc<TurnToken>) {
        let mut slot = super::lock(&self.shared.turn_token);
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            *slot = None;
        }
    }

    fn generation_changed(&self, generation: u64) -> bool {
        self.shared
            .turn_generation
            .load(std::sync::atomic::Ordering::SeqCst)
            != generation
    }

    /// `_turn_lock.acquire(timeout=BUSY_WAIT_SECS)` y sondeo posterior.
    fn acquire_turn(
        &self,
        generation: u64,
        mode: AskMode,
    ) -> Result<std::sync::MutexGuard<'_, ()>, TurnError> {
        if let Ok(guard) = self.shared.turn_lock.try_lock() {
            return Ok(guard);
        }
        let cancelled = || TurnError::cancelled("turno cancelado mientras esperaba");
        let wait_until = Instant::now() + Duration::from_secs_f64(BUSY_WAIT_SECS);
        while Instant::now() < wait_until {
            if self.generation_changed(generation) {
                return Err(cancelled());
            }
            std::thread::sleep(Duration::from_millis(25));
            match self.shared.turn_lock.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        if self.generation_changed(generation) {
            return Err(cancelled());
        }
        self.notify_busy(mode);
        loop {
            if self.generation_changed(generation) {
                return Err(cancelled());
            }
            std::thread::sleep(Duration::from_secs_f64(BUSY_POLL_SECS));
            match self.shared.turn_lock.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => {}
            }
        }
    }

    fn notify_busy(&self, mode: AskMode) {
        let callback = super::lock(&self.shared.on_busy).clone();
        if let Some(callback) = callback {
            callback(mode.name());
        }
    }
}
