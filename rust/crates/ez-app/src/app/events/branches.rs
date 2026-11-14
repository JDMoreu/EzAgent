//! Ramas de `_handle_event` con lógica propia (F6-03).

use std::sync::atomic::Ordering;

use ez_core::metrics::{format_stt, monotonic};
use ez_core::pyvalue::head_chars;

use super::super::core::{log, VoiceApp, LISTENING, THINKING};
use super::api::PauseApi;
use super::{confirm_pending, in_call, muted, stripped, turn_gen};

impl VoiceApp {
    /// `("busy_notice", gen, mode)`.
    pub(super) fn event_busy_notice(&self, generation: i64) {
        if generation != turn_gen(self) {
            return;
        }
        let text = "Termino lo anterior y sigo.";
        self.inner.chat.chat.add_note(text);
        // El interruptor del chat también silencia el aviso hablado.
        if !self.inner.chat.chat.voice_replies() {
            return;
        }
        self.speak_busy(text, generation);
    }

    /// `("busy_done", gen)`.
    pub(super) fn event_busy_done(&self, generation: i64) {
        self.inner.speech.busy_speaking.clear();
        let barged = self.inner.speech.busy_barged.swap(false, Ordering::SeqCst);
        // Si el barge-in ya cortó la frase, el audio reinyectado manda.
        if generation != turn_gen(self) || barged {
            return;
        }
        self.clear_echo_guards();
        if self.state() != LISTENING {
            self.inner.audio.reset_capture.set();
        }
    }

    /// `("chat_mute", muted)`.
    pub(super) fn event_chat_mute(&self, value: bool) {
        if !in_call(self) && !value {
            self.resume_listening_from_mic();
        } else if in_call(self) {
            self.set_mic_muted(value, "chat");
        }
    }

    /// `("transcribed", text, gen)`.
    pub(super) fn event_transcribed(&self, text: &str, generation: i64) {
        let text = stripped(text);
        // Una transcripción vieja no resuelve la confirmación vigente.
        if !in_call(self) || generation != turn_gen(self) {
            return;
        }
        if !text.is_empty() {
            self.news_seen();
        }
        if confirm_pending(self) && !text.is_empty() {
            self.answer_confirmation(&text);
            return;
        }
        if text.is_empty() {
            self.set_state(LISTENING);
            return;
        }
        self.log_turn_timing();
        log(format!("Tú: {text}"));
        let fast_t0 = monotonic();
        if self.fast_command(&text) {
            let handled_ms = (monotonic() - fast_t0) * 1000.0;
            self.inner
                .runtime
                .latency
                .lock()
                .record_command("fast_command", handled_ms);
            log(format!("Comando local: {handled_ms:.0} ms"));
            return;
        }
        self.set_state(THINKING);
        self.show_voice_text(&text);
        let app = self.clone();
        self.start_worker("ezagent-answer", move || {
            app.answer_worker(&text, generation)
        });
    }

    /// Respuesta hablada a la confirmación pendiente, sin cerrar el turno.
    fn answer_confirmation(&self, text: &str) {
        let token = self.inner.turn.confirm_token.read().clone();
        let token = if token.is_empty() { None } else { Some(token) };
        let agent = self.inner.agents.agent.lock();
        let result = agent
            .as_ref()
            .and_then(|agent| agent.submit_confirmation(text, token.as_deref()));
        drop(agent);
        if result.is_none() {
            self.speak_same_turn("No te entendí. Responde «sí» o «no».", "confirm");
        }
    }

    /// `log(f"Tiempos: {format_stt(...)}")` cuando hay marcas del turno.
    fn log_turn_timing(&self) {
        let timing = self.inner.turn.turn_timing.lock().clone();
        if let Some(timing) = timing {
            log(format!("Tiempos: {}", format_stt(&timing.report())));
        }
    }

    /// `("answer_done", gen, outcome)`.
    pub(super) fn event_answer_done(&self, generation: i64, outcome: &str) {
        if generation != turn_gen(self) {
            return;
        }
        let timing = self.inner.turn.turn_timing.lock().take();
        if let Some(timing) = timing {
            timing.mark("turn_end", None);
            self.record_latency(&timing, outcome);
        }
        *self.inner.speech.speaking_kind.write() = None;
        self.clear_echo_guards();
        self.inner.audio.reset_capture.set();
        self.watch_pending_notify();
        if self
            .inner
            .turn
            .close_after_answer
            .swap(false, Ordering::SeqCst)
        {
            if self.close_listening() {
                self.pump_announcements();
            }
            return;
        }
        self.finish_turn();
    }

    /// `("speak_done", gen)`.
    pub(super) fn event_speak_done(&self, generation: i64) {
        if generation != turn_gen(self) {
            return;
        }
        let kind_before = self.inner.speech.speaking_kind.write().take();
        if kind_before.as_deref() == Some("confirm") {
            if let Some(agent) = self.inner.agents.agent.lock().as_ref() {
                agent.arm_confirmation();
            }
        }
        self.clear_echo_guards();
        self.inner.audio.reset_capture.set();
        if self
            .inner
            .turn
            .close_after_answer
            .swap(false, Ordering::SeqCst)
        {
            if in_call(self) {
                self.close_listening();
            }
            self.pump_announcements();
            return;
        }
        self.finish_turn();
    }

    /// `("swarm_notice", text, asks)`.
    pub(super) fn event_swarm_notice(&self, text: &str, asks: bool) {
        log(format!("Enjambre: {text}"));
        // Una pregunta no se silencia: el enjambre espera la respuesta.
        if asks {
            self.news_arrived("swarm:ask", "enjambre", text, "blocked");
        }
        let muted_until = *self.inner.session.mute_until.lock();
        if asks || ez_core::pytime::timestamp() >= muted_until {
            self.queue_announcement(text, asks);
        }
    }

    /// `("error", message[, gen])`.
    pub(super) fn event_error(&self, message: &str, generation: Option<i64>) {
        let message = head_chars(message, 320);
        if let Some(generation) = generation {
            if generation != turn_gen(self) {
                log(format!("Error obsoleto del turno {generation}: {message}"));
                return;
            }
        }
        log(format!("Error: {message}"));
        if message.starts_with("Kokoro:") {
            // No reintentar con el mismo TTS que acaba de fallar.
            self.set_state("error");
            return;
        }
        let lower = message.to_lowercase();
        if lower.starts_with("no puedo usar el micrófono") || lower.starts_with("micrófono") {
            self.hang_up();
            self.speak(message, "answer");
        } else if in_call(self) {
            self.speak(message, "answer");
        } else {
            self.set_idle();
            self.speak(message, "answer");
        }
    }

    /// `("test_done", old_muted, old_barge)`.
    pub(super) fn event_test_done(&self, old_muted: bool, old_barge: f64) {
        self.inner
            .config
            .cfg
            .write()
            .set("barge_in", serde_json::Value::Bool(old_barge != 0.0));
        if !old_muted && muted(self) {
            self.set_mic_muted(false, "");
        }
        self.inner.ui.hud.set_testing(false);
        if in_call(self) {
            self.set_state(LISTENING);
        } else {
            self.set_idle();
        }
    }

    /// `("confirm_click", yes[, token])`.
    pub(super) fn event_confirm_click(&self, yes: bool, token: Option<&str>) {
        self.signal_tts_stop();
        let agent = self.inner.agents.agent.lock();
        let Some(agent) = agent.as_ref() else {
            return;
        };
        let pending = agent.pending_confirmation_token().unwrap_or_default();
        let token = token.unwrap_or_default();
        if !token.is_empty() && !pending.is_empty() && token != pending {
            log("Confirmación atrasada ignorada.");
            return;
        }
        agent.submit_confirmation_click(yes);
    }

    /// Red de seguridad de `answer_done`: registra el último agente lanzado.
    fn watch_pending_notify(&self) {
        let notify = self
            .inner
            .agents
            .messenger
            .lock()
            .as_ref()
            .and_then(|messenger| messenger.pending_notify_address());
        let Some(address) = notify else {
            return;
        };
        if !self.inner.agents.watched.lock().contains(&address) {
            self.watch(&address);
        }
    }

    /// El turno terminó: escuchar de nuevo o quedar en reposo.
    fn finish_turn(&self) {
        if in_call(self) {
            self.set_state(LISTENING);
        } else {
            self.set_idle();
        }
        self.pump_announcements();
    }

    /// `_ignore_utterance`, `_post_tts_ignore` y la ventana de eco.
    fn clear_echo_guards(&self) {
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
        *self.inner.turn.post_tts_until.lock() = monotonic() + 0.25;
    }
}
