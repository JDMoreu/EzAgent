//! Port of `src/ezagent/voice/app.py 1674-1860` (F6-04).
//!
//! Estados de la barra, voz del turno, confirmación por voz y avisos de agentes.
//! `_start_worker`, `_drain_workers`, `_signal_tts_stop` y `_advance_turn` ya
//! están portados en F6-01 y se reutilizan.

#[cfg(test)]
pub(crate) mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use ez_chat::store::Source;
use ez_core::metrics::TurnTimings;
use ez_hud::win::drop::VoidCallback;
use parking_lot::Mutex;
use serde_json::{Map, Value};

use super::core::state::Flag;
use super::core::{
    advance_turn, cfg_text, log, AppEvent, VoiceApp, IDLE, LISTENING, SPEAKING, THINKING,
};
use super::events::{confirm_pending, in_call, turn_gen};

/// `round(value, 1)` de Python (redondeo a par).
fn round1(value: f64) -> f64 {
    (value * 10.0).round_ties_even() / 10.0
}

/// Texto que se añade al resumen de la confirmación (`_show_confirmation`).
fn confirmation_hint(strong: bool) -> &'static str {
    if strong {
        " Di «confirmo» y el número para aceptar."
    } else {
        " ¿Confirmas?"
    }
}

/// `SpeakerApi` pide `&AtomicBool` y el estado guarda `Flag` (= Event).
///
/// Refleja el evento en el átomo mientras dura la locución; sin este puente, un
/// barge-in no cortaría el audio a mitad de frase.
// DECISION: un hilo corto (`ezagent-tts-stop`) por locución, porque no se
// puede tocar `Flag` ni `SpeakerApi` desde esta tarea.
pub(crate) struct StopBridge {
    atomic: Arc<AtomicBool>,
    done: Arc<Flag>,
    monitor: Mutex<Option<JoinHandle<()>>>,
}

impl StopBridge {
    pub(crate) fn new(flag: &Arc<Flag>) -> Self {
        let atomic = Arc::new(AtomicBool::new(flag.is_set()));
        let done = Arc::new(Flag::new());
        let watch_flag = Arc::clone(flag);
        let watch_atomic = Arc::clone(&atomic);
        let watch_done = Arc::clone(&done);
        let monitor = std::thread::Builder::new()
            .name("ezagent-tts-stop".to_string())
            .spawn(move || {
                while !watch_done.is_set() {
                    if watch_flag.wait_timeout(0.02) {
                        watch_atomic.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            })
            .map_err(|error| log(format!("No pude crear el hilo ezagent-tts-stop: {error}")))
            .ok();
        Self {
            atomic,
            done,
            monitor: Mutex::new(monitor),
        }
    }

    pub(crate) fn atomic(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.atomic)
    }
}

impl Drop for StopBridge {
    fn drop(&mut self) {
        self.done.set();
        if let Some(handle) = self.monitor.lock().take() {
            let _ = handle.join();
        }
    }
}

impl VoiceApp {
    /// `_set_state`: estado visible, barra y título de la bandeja.
    pub fn set_state(&self, state: &str) {
        *self.inner.session.state.write() = state.to_string();
        if state == LISTENING {
            self.inner
                .session
                .ignore_utterance
                .store(false, Ordering::SeqCst);
            self.inner
                .turn
                .post_tts_ignore
                .store(false, Ordering::SeqCst);
        }
        if state == IDLE {
            self.inner.ui.hud.set_in_call(in_call(self));
            self.inner.ui.hud.set_mic_level(0.0);
            if !self.inner.config.hud_hidden.load(Ordering::SeqCst) {
                self.inner.ui.hud.show(IDLE);
                self.inner.ui.hud.draw(0.0, Some(IDLE));
            }
        } else {
            if self.inner.config.hud_hidden.swap(false, Ordering::SeqCst) {
                self.inner
                    .config
                    .cfg
                    .write()
                    .set("hud_hidden", Value::Bool(false));
                // DECISION: se guarda en el archivo de la app, como F6-01.
                if let Err(error) = self.inner.config.cfg.read().save_key(
                    "hud_hidden",
                    Value::Bool(false),
                    Some(&self.inner.config.config_file),
                ) {
                    log(format!("No pude guardar hud_hidden: {error}"));
                }
            }
            self.inner.ui.hud.set_in_call(in_call(self));
            self.inner.ui.hud.show(state);
            self.inner.ui.hud.draw(0.0, Some(state));
        }
        self.update_tray_title();
    }

    /// `_set_idle`.
    pub fn set_idle(&self) {
        self.set_state(IDLE);
    }

    /// `_bump_turn`: invalida el turno vigente y suelta la red del LLM.
    pub fn bump_turn(&self) -> i64 {
        advance_turn(self)
    }

    /// `_record_latency`: metadatos del turno para las estadísticas.
    pub fn record_latency(&self, timing: &TurnTimings, outcome: &str) {
        let meta = self.latency_meta(timing, outcome);
        self.inner
            .runtime
            .latency
            .lock()
            .record(timing, Some(&meta));
    }

    /// Metadatos que `_record_latency` adjunta al turno.
    fn latency_meta(&self, timing: &TurnTimings, outcome: &str) -> Map<String, Value> {
        let cfg = self.config();
        let rounds = {
            let slot = self.inner.agents.messenger.lock();
            slot.as_ref()
                .map_or(0, |messenger| messenger.last_rounds().len())
        };
        let mut meta = Map::new();
        meta.insert(
            "provider".to_string(),
            Value::String(cfg_text(&cfg, "llm_provider", "")),
        );
        meta.insert(
            "model".to_string(),
            Value::String(cfg_text(&cfg, "llm_model", "")),
        );
        meta.insert("rounds".to_string(), Value::from(rounds));
        meta.insert("outcome".to_string(), Value::String(outcome.to_string()));
        if let Some(first_ack) = timing.report().get("first_ack_ms") {
            meta.insert("first_ack_ms".to_string(), Value::from(round1(*first_ack)));
        }
        meta
    }

    /// `_new_tts_stop`: corta el TTS anterior y el aviso de espera en curso.
    pub fn new_tts_stop(&self) -> Arc<Flag> {
        let mut slot = self.inner.speech.tts_stop.lock();
        if let Some(stop) = slot.as_ref() {
            stop.set();
        }
        if let Some(stop) = self.inner.speech.busy_stop.lock().as_ref() {
            stop.set();
        }
        let stop = Arc::new(Flag::new());
        *slot = Some(Arc::clone(&stop));
        stop
    }

    /// `_speak`: corta el turno anterior y dice `text` en `ezagent-tts`.
    pub fn speak(&self, text: &str, kind: &str) {
        if text.is_empty() {
            return;
        }
        let generation = advance_turn(self);
        self.start_speech(text, kind, generation);
    }

    /// `_speak_same_turn`: habla sin invalidar el turno que espera herramientas.
    pub fn speak_same_turn(&self, text: &str, kind: &str) {
        if text.is_empty() {
            return;
        }
        let generation = turn_gen(self);
        self.start_speech(text, kind, generation);
    }

    fn start_speech(&self, text: &str, kind: &str, generation: i64) {
        let stop = self.new_tts_stop();
        *self.inner.speech.speaking_kind.write() = Some(kind.to_string());
        self.set_state(SPEAKING);
        let app = self.clone();
        let text = text.to_string();
        self.start_worker("ezagent-tts", move || {
            app.tts_worker(&text, stop, generation);
        });
    }

    /// `_show_confirmation`: fila Sí/No en la barra y lectura del resumen.
    pub fn show_confirmation(&self, summary: &str, strong: bool) {
        // El evento puede llegar después de que el usuario ya respondió.
        if !confirm_pending(self) {
            return;
        }
        self.inner.chat.chat.ask_confirm(summary, strong);
        let token = self.inner.turn.confirm_token.read().clone();
        let yes_events = Arc::clone(&self.inner.ui.events);
        let no_events = Arc::clone(&self.inner.ui.events);
        let yes_token = token.clone();
        let yes: VoidCallback = Arc::new(move || {
            yes_events.put(AppEvent::ConfirmClick(true, Some(yes_token.clone())));
        });
        let no: VoidCallback = Arc::new(move || {
            no_events.put(AppEvent::ConfirmClick(false, Some(token.clone())));
        });
        // DECISION: `show_confirmation` no devuelve error; el try/except del
        // Python solo registraba el fallo del HUD.
        self.inner
            .ui
            .hud
            .show_confirmation(summary, strong, Some(yes), Some(no));
        let hint = confirmation_hint(strong);
        self.speak_same_turn(&format!("{summary}{hint}"), "confirm");
    }

    /// `_queue_announcement`: encola un aviso y prueba a decirlo ya.
    pub fn queue_announcement(&self, text: &str, reopen_call: bool) {
        if text.is_empty() {
            return;
        }
        self.inner
            .agents
            .announce_queue
            .lock()
            .push((text.to_string(), reopen_call));
        self.pump_announcements();
    }

    /// `_pump_announcements`: dice el siguiente aviso si no hay voz ni turno.
    pub fn pump_announcements(&self) {
        if self.inner.session.user_speaking.load(Ordering::SeqCst)
            || matches!(self.state().as_str(), THINKING | SPEAKING)
            || confirm_pending(self)
        {
            return;
        }
        let next = {
            let mut queue = self.inner.agents.announce_queue.lock();
            if queue.is_empty() {
                return;
            }
            queue.remove(0)
        };
        if next.1 && !in_call(self) {
            self.start_call();
        }
        self.speak(&next.0, "announce");
    }

    /// `_show_voice_text`: una sola caja por frase; si continúa, la actualiza.
    pub fn show_voice_text(&self, text: &str) {
        let merge_id = self.inner.turn.merge_user_id.lock().take();
        if let Some(merge_id) = merge_id {
            if self.inner.chat.chat.revise_user(merge_id, text) {
                *self.inner.turn.last_voice_user_id.lock() = Some(merge_id);
                return;
            }
        }
        let id = self.inner.chat.chat.add_user(text, Source::Voice).id;
        *self.inner.turn.last_voice_user_id.lock() = Some(id);
    }

    /// `_tts_worker`: dice una frase y avisa que terminó.
    fn tts_worker(&self, text: &str, stop: Arc<Flag>, generation: i64) {
        let bridge = StopBridge::new(&stop);
        let hud = Arc::clone(&self.inner.ui.hud);
        let mut on_level = |level: f64| hud.draw(level, Some(SPEAKING));
        let atomic = bridge.atomic();
        let outcome = self
            .inner
            .speech
            .speaker
            .speak(text, &atomic, Some(&mut on_level), None);
        if let Err(error) = outcome {
            self.events().put(AppEvent::Error(
                format!("Kokoro: {error}"),
                Some(generation),
            ));
            return;
        }
        self.events().put(AppEvent::SpeakDone(generation));
    }
}
