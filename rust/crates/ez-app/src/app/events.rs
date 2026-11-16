//! Port of `src/ezagent/voice/app.py 1126-1673` (F6-03).
//!
//! `_tick`, `_handle_hotkey`, `_show_bar_if_hidden` y el `match` de
//! `_handle_event`. Los métodos de otras tareas se piden a los traits de
//! [`api`]; cada uno documenta a qué método inherente sustituye (F6-07 cablea
//! el conjunto).

mod api;
mod branches;
pub use api::{ChatApi, MenuApi, PauseApi, SettingsApi, TrayApi, TurnApi};

#[cfg(test)]
pub(crate) mod support;
#[cfg(test)]
mod tests;

use std::sync::atomic::Ordering;

use serde_json::Value;

use super::core::{log, AppEvent, VoiceApp, IDLE, SPEAKING};
use ez_core::pyvalue::py_strip;

/// `self.in_call`.
pub(crate) fn in_call(app: &VoiceApp) -> bool {
    app.inner.session.in_call.load(Ordering::SeqCst)
}

/// `self.muted`.
pub(crate) fn muted(app: &VoiceApp) -> bool {
    app.inner.session.muted.load(Ordering::SeqCst)
}

/// `self._turn_gen`.
pub(crate) fn turn_gen(app: &VoiceApp) -> i64 {
    app.inner.turn.turn_gen.load(Ordering::SeqCst)
}

/// `agent is not None and agent.confirm_pending`.
pub(crate) fn confirm_pending(app: &VoiceApp) -> bool {
    app.inner
        .agents
        .agent
        .lock()
        .as_ref()
        .is_some_and(|agent| agent.confirm_pending())
}

impl VoiceApp {
    /// `_tick`, sin reprogramarse: `core::callbacks::schedule_tick` repite.
    pub fn tick(&self) {
        while let Some(event) = self.inner.ui.hotkey_events.get_nowait() {
            guarded(|| self.handle_hotkey(&event));
        }
        while let Some(event) = self.inner.ui.events.get_nowait() {
            guarded(|| self.handle_event(event));
        }
        while let Some(event) = self.inner.ui.menu_events.get_nowait() {
            guarded(|| {
                // `_handle_menu_event` devuelve el error al llamador (F6-06b).
                if let Err(error) = self.handle_menu_event(&event) {
                    tracing::warn!("Error en menú: {error}");
                }
            });
        }
        // Red de seguridad: un aviso encolado no se queda atascado.
        guarded(|| self.pump_announcements());
    }

    /// `_handle_hotkey`.
    pub fn handle_hotkey(&self, event: &str) {
        log(format!("Hotkey: {event}"));
        match event {
            "toggle" => {
                if in_call(self) {
                    self.hang_up();
                } else {
                    self.start_call();
                }
            }
            "cancel" => self.cancel_hotkey(),
            "repeat" => {
                let summary = self.last_summary();
                if !summary.is_empty() {
                    self.speak(&summary, "answer");
                } else if self.state() == IDLE {
                    self.speak("No hay nada que repetir todavía.", "answer");
                }
            }
            "mute" => self.set_mic_muted(!muted(self), "atajo"),
            "menu" => {
                self.show_bar_if_hidden();
                self.toggle_menu();
            }
            "chat" => {
                self.show_bar_if_hidden();
                self.toggle_chat();
            }
            _ => {}
        }
    }

    /// `_show_bar_if_hidden`: si estaba oculta, la barra vuelve a mostrarse.
    pub fn show_bar_if_hidden(&self) {
        if !self.inner.config.hud_hidden.load(Ordering::SeqCst) {
            return;
        }
        self.inner.config.hud_hidden.store(false, Ordering::SeqCst);
        {
            let mut cfg = self.inner.config.cfg.write();
            cfg.set("hud_hidden", Value::Bool(false));
            if let Err(error) = cfg.save_key(
                "hud_hidden",
                Value::Bool(false),
                Some(&self.inner.config.config_file),
            ) {
                log(format!("No pude guardar hud_hidden: {error}"));
            }
        }
        self.inner.ui.hud.show(IDLE);
    }

    /// `_handle_event`: el `match` sobre las tuplas de `self.events`.
    pub fn handle_event(&self, event: AppEvent) {
        match event {
            AppEvent::HudAction(action) => self.hud_action_ui(&action),
            AppEvent::TrayAction(action) => self.tray_action_ui(&action),
            AppEvent::MenuAction(action) => self.menu_action_ui(&action),
            AppEvent::HotkeyCapture(active) => self.suspend_hotkeys(active),
            AppEvent::ActivityOpen(key) => self.open_activity(&key),
            AppEvent::ActivityDismiss(key) => self.dismiss_activity(&key),
            AppEvent::ChatSend(text) => {
                self.news_seen();
                self.chat_send(&text);
            }
            AppEvent::AudioSpeechStart(level) => self.on_speech_start(level),
            AppEvent::AudioBargeIn(level) => self.on_barge_in(level),
            AppEvent::AudioUtterance(pcm, rate) => self.on_utterance(pcm, rate),
            AppEvent::Say(text) => self.speak(&text, "answer"),
            other => self.handle_dialogue_event(other),
        }
    }

    /// Eventos del turno: avisos, chat, transcripción y respuestas.
    fn handle_dialogue_event(&self, event: AppEvent) {
        match event {
            AppEvent::BusyNotice(generation, _mode) => self.event_busy_notice(generation),
            AppEvent::BusyDone(generation) => self.event_busy_done(generation),
            AppEvent::ChatNew => self.chat_new(),
            AppEvent::ChatSwitch(session) => self.chat_switch(session),
            AppEvent::ChatDelete(session) => self.chat_delete(session),
            AppEvent::ChatStop => self.chat_stop(),
            AppEvent::ChatMute(muted) => self.event_chat_mute(muted),
            AppEvent::Transcribed(text, generation) => self.event_transcribed(&text, generation),
            AppEvent::Speaking(generation) => {
                if generation == turn_gen(self) {
                    self.set_state(SPEAKING);
                }
            }
            AppEvent::AnswerDone(generation, outcome) => {
                self.event_answer_done(generation, &outcome);
            }
            other => self.handle_result_event(other),
        }
    }

    /// Resultados del turno, avisos del sistema y cierre.
    fn handle_result_event(&self, event: AppEvent) {
        match event {
            AppEvent::SwarmNotice(text, asks) => self.event_swarm_notice(&text, asks),
            AppEvent::DoneAnswer {
                address,
                agent,
                text,
                task,
            } => self.handle_done_answer(&address, &agent, &text, &task),
            AppEvent::DonePrompt {
                address,
                agent,
                reason,
                output,
                task,
            } => self.handle_done_prompt(&address, &agent, &reason, &output, &task),
            AppEvent::SpeakDone(generation) => self.event_speak_done(generation),
            AppEvent::LlmModelsReady(provider, models) => {
                self.fix_model_name(&provider, &models);
            }
            AppEvent::Error(message, generation) => self.event_error(&message, generation),
            AppEvent::TestDone(old_muted, old_barge) => {
                self.event_test_done(old_muted, old_barge);
            }
            AppEvent::ConfirmAsk(summary, strong, token) => {
                *self.inner.turn.confirm_token.write() = token.unwrap_or_default();
                self.show_confirmation(&summary, strong);
            }
            AppEvent::ConfirmDone => {
                self.inner.chat.chat.clear_confirm();
                self.inner.ui.hud.hide_confirmation();
            }
            AppEvent::ConfirmClick(yes, token) => {
                self.event_confirm_click(yes, token.as_deref());
            }
            AppEvent::AgentWarn(message) => {
                log(format!("Núcleo: {message}"));
                // Durante una llamada no se interrumpe el turno por un aviso.
                if !in_call(self) {
                    self.speak(&message, "answer");
                }
            }
            AppEvent::CloseRequest(mode) => {
                self.inner
                    .turn
                    .close_after_answer
                    .store(true, Ordering::SeqCst);
                *self.inner.turn.close_mode.write() = mode;
            }
            AppEvent::Wake => self.on_wake(),
            AppEvent::Hide => {}
            AppEvent::Raw(values) => self.handle_raw(&values),
            _ => {}
        }
    }

    /// `("hud_error", texto)`, la única tupla sin variante propia.
    fn handle_raw(&self, values: &[Value]) {
        let Some(kind) = values.first().and_then(Value::as_str) else {
            return;
        };
        if kind != "hud_error" {
            return;
        }
        if let Some(message) = values.get(1) {
            log(format!("HUD: {}", ez_core::config::python_str(message)));
        }
    }
}

/// `text.strip()` del Python.
pub(crate) fn stripped(text: &str) -> String {
    py_strip(text).to_string()
}

/// `try/except` de `_tick`: un manejador roto no detiene los demás.
// DECISION: Rust no tiene excepciones; un pánico del callback del bucle de la
// interfaz se captura por evento para que la cola siga vaciándose.
fn guarded(action: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
}
