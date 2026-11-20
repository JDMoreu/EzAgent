//! Port of `src/ezagent/voice/app.py 2451-2657` (F6-06a).
//!
//! Atajos locales, acciones del HUD, menú embebido y ocultar la barra. La
//! clasificación de `_fast_command` vive en [`crate::fast`] (F3-16); aquí se
//! ejecuta su resultado contra la app. Los efectos que pertenecen a otras
//! tareas entran por [`SettingsEffects`] con el nombre del Python y F6-07 los
//! cablea al `VoiceApp` real.

use std::sync::atomic::Ordering;

use ez_audio::capture::list_input_devices;
use ez_audio::wake::DEFAULT_WAKE_PHRASE;
use ez_core::metrics::monotonic;
use ez_core::pyvalue::{text_or_empty, truthy};
use serde_json::Value;

use super::core::{cfg_float, log, AppEvent, VoiceApp};
use crate::fast::{self, FastHost, FastOutcome};

/// Operaciones de `VoiceApp` que pertenecen a otras tareas.
///
/// `_fast_command` y `_hud_action_ui` las llaman con el nombre del Python en
/// snake_case. Salvo `set_mic_muted` (estado mínimo que esta tarea necesita),
/// los valores por omisión no hacen nada: el cableado real lo completa cada
/// tarea dueña (`_speak`, `_set_state`, `_handle_hotkey`, `_refresh_activity`…).
pub trait SettingsEffects: Send + Sync {
    /// `self._speak(text, kind)` (F6-04).
    fn speak(&self, _text: &str, _kind: &str) {}
    /// `self._set_state(state)` (F6-04).
    fn set_state(&self, _state: &str) {}
    /// `self._set_mic_muted(muted, source)` (F6-07).
    fn set_mic_muted(&self, _muted: bool, _source: &str) {}
    /// `self._resume_listening_from_mic()` (F6-02).
    fn resume_listening_from_mic(&self) {}
    /// `self._toggle_chat()` (F6-05).
    fn toggle_chat(&self) {}
    /// `self._handle_hotkey(event)` (F6-04).
    fn handle_hotkey(&self, _event: &str) {}
    /// `_refresh_activity(self)` (F6-07).
    fn refresh_activity(&self) {}
}

impl SettingsEffects for VoiceApp {
    fn speak(&self, text: &str, kind: &str) {
        self.speak(text, kind);
    }

    fn set_state(&self, state: &str) {
        self.set_state(state);
    }

    fn set_mic_muted(&self, muted: bool, source: &str) {
        self.set_mic_muted(muted, source);
    }

    fn resume_listening_from_mic(&self) {
        self.resume_listening_from_mic();
    }

    fn toggle_chat(&self) {
        self.toggle_chat();
    }

    fn handle_hotkey(&self, event: &str) {
        self.handle_hotkey(event);
    }

    fn refresh_activity(&self) {
        self.refresh_activity();
    }
}

/// `self._pause_message()`: lo genera el código, nunca el LLM (F6-07).
fn pause_message(app: &VoiceApp) -> String {
    let cfg = app.config();
    if !cfg.get("voice_wake").is_none_or(truthy) {
        return "En pausa. Toca el micrófono para volver.".to_string();
    }
    let phrase = text_or_empty(cfg.get("wake_phrase"));
    let phrase = if phrase.is_empty() {
        DEFAULT_WAKE_PHRASE.to_string()
    } else {
        phrase
    };
    format!("En pausa. Di «{phrase}» cuando me necesites.")
}

/// `self._close_action()`: solo `pause` y `hangup`; lo demás es `pause` (F6-07).
fn close_action(app: &VoiceApp) -> String {
    let value = text_or_empty(app.config().get("close_action")).to_lowercase();
    match value.as_str() {
        "pause" | "hangup" => value,
        _ => "pause".to_string(),
    }
}

/// `self._agent_label(agent)`: etiqueta del perfil o el nombre dictado.
fn agent_label(app: &VoiceApp, agent: &str) -> String {
    let profile_label = app
        .inner
        .agents
        .herdr
        .resolve_agent(agent)
        .and_then(|key| {
            app.config()
                .get("agent_profiles")
                .and_then(Value::as_object)
                .and_then(|profiles| profiles.get(&key))
                .and_then(Value::as_object)
                .and_then(|profile| profile.get("label"))
                .map(|label| text_or_empty(Some(label)))
        })
        .unwrap_or_default();
    if !profile_label.is_empty() {
        return profile_label;
    }
    if agent.is_empty() {
        "El agente".to_string()
    } else {
        agent.to_string()
    }
}

/// `VoiceApp` visto por `_fast_command` (`FastHost`) sin duplicar la app.
struct AppFastHost<'a> {
    app: &'a VoiceApp,
}

impl FastHost for AppFastHost<'_> {
    fn close_action(&self) -> String {
        close_action(self.app)
    }

    fn last_summary(&self) -> String {
        self.app.last_summary()
    }

    fn pause_message(&self) -> String {
        pause_message(self.app)
    }

    fn pending_tasks(&self) -> Vec<serde_json::Map<String, Value>> {
        self.app.inner.agents.tasks.all()
    }

    fn label_for(&self, address: &str) -> String {
        self.app.inner.agents.herdr.label_for(address)
    }

    fn agent_label(&self, agent: &str) -> String {
        agent_label(self.app, agent)
    }

    fn ack_context(&self) -> Option<(f64, Option<String>, bool)> {
        let speech = &self.app.inner.speech;
        // Se consume el tipo: un segundo acuse seguido no vuelve a activar el atajo.
        let kind = speech.last_outbound_kind.write().take();
        kind.map(|kind| {
            let seconds = monotonic() - *speech.last_outbound_done_at.read();
            let question = *speech.last_outbound_question.read();
            (seconds, Some(kind), question)
        })
    }

    fn ack_index(&self) -> usize {
        self.app
            .inner
            .speech
            .ack_index
            .fetch_add(1, Ordering::SeqCst)
    }

    fn ack_window_secs(&self) -> f64 {
        let cfg = self.app.config();
        cfg_float(&cfg, "ack_fast_window_secs", fast::ACK_WINDOW_SECS)
    }

    fn subagent_status(&self, name: &str) -> Option<String> {
        Some(self.app.subagent_status_phrase(name))
    }

    fn subagents_overview(&self) -> Option<String> {
        Some(self.app.subagents_overview_phrase())
    }
}

impl VoiceApp {
    /// `VoiceApp._normalize`: minúsculas, sin tildes ni signos.
    pub fn normalize(text: &str) -> String {
        fast::normalize(text)
    }

    /// `_fast_command`: ejecuta el atajo local y devuelve si lo atendió.
    pub fn fast_command(&self, text: &str) -> bool {
        self.fast_command_with(text, self)
    }

    /// Igual que [`Self::fast_command`] con efectos intercambiables (pruebas).
    pub fn fast_command_with(&self, text: &str, effects: &dyn SettingsEffects) -> bool {
        self.fast_command_outcome_with(text, effects).handled
    }

    /// Como [`Self::fast_command`] pero devuelve el resultado para saber si fue
    /// un acuse y registrar `ack_fast` en lugar de `fast_command`.
    pub fn fast_command_outcome(&self, text: &str) -> FastOutcome {
        self.fast_command_outcome_with(text, self)
    }

    fn fast_command_outcome_with(&self, text: &str, effects: &dyn SettingsEffects) -> FastOutcome {
        let outcome = fast::fast_command(text, &AppFastHost { app: self });
        self.apply_fast_outcome(&outcome, effects);
        outcome
    }

    fn apply_fast_outcome(&self, outcome: &FastOutcome, effects: &dyn SettingsEffects) {
        if let Some(muted) = outcome.mic {
            effects.set_mic_muted(muted, if muted { "voz" } else { "" });
        }
        if let Some(close) = outcome.close {
            self.inner
                .turn
                .close_after_answer
                .store(true, Ordering::SeqCst);
            *self.inner.turn.close_mode.write() = close.as_str().to_string();
        }
        if outcome.list_agents {
            self.start_agents_worker();
        }
        if outcome.reveal_activity {
            effects.refresh_activity();
            self.inner.ui.hud.show_activity();
        }
        if let Some((kind, text)) = &outcome.speech {
            *self.inner.speech.last_outbound_question.write() = text.trim_end().ends_with('?');
            effects.speak(text, kind);
        }
    }

    /// Hilo `ezagent-list-agents`: Herdr nunca bloquea el hilo de la interfaz.
    fn start_agents_worker(&self) {
        let app = self.clone();
        self.start_worker(fast::LIST_AGENTS_THREAD, move || {
            let report = match app.inner.agents.herdr.list_agents(20.0) {
                Ok(agents) => fast::agents_report(&agents),
                Err(error) => {
                    log(format!("No pude listar agentes: {}", error.message()));
                    fast::AGENTS_UNAVAILABLE.to_string()
                }
            };
            app.events().put(AppEvent::Say(report));
        });
    }

    /// `_on_hud_action`: el WndProc entrega el clic y `_tick` lo aplica.
    pub fn on_hud_action(&self, action: &str) {
        log(format!("HUD: {action}"));
        self.inner
            .ui
            .events
            .put(AppEvent::HudAction(action.to_string()));
    }

    /// `_hud_action_ui`: aplica en el hilo de la interfaz lo que pidió el HUD.
    pub fn hud_action_ui(&self, action: &str) {
        match action {
            "mute" => {
                let muted = !self.inner.session.muted.load(Ordering::SeqCst);
                self.set_mic_muted(muted, "boton");
            }
            "listen" => self.resume_listening_from_mic(),
            "panel" => self.toggle_menu(),
            "chat" => self.toggle_chat(),
            "repeat" => self.handle_hotkey("repeat"),
            "hangup" => self.handle_hotkey("cancel"),
            "hide" => self.hide_hud(true),
            _ => {}
        }
    }

    /// `_toggle_menu`: el menú vive dentro de la barra; no hay panel Tk.
    pub fn toggle_menu(&self) {
        self.inner.ui.hud.toggle_menu();
    }

    /// `hide_hud`: oculta la barra y recuerda que quedó oculta.
    pub fn hide_hud(&self, close_eyes: bool) {
        // `drop.py` recibe `close_eyes` pero no lo usa; se conserva la firma.
        let _ = close_eyes;
        self.inner.config.hud_hidden.store(true, Ordering::SeqCst);
        let saved = {
            let mut cfg = self.inner.config.cfg.write();
            cfg.set("hud_hidden", Value::Bool(true));
            cfg.save_key(
                "hud_hidden",
                Value::Bool(true),
                Some(&self.inner.config.config_file),
            )
        };
        if let Err(error) = saved {
            log(format!("No pude guardar hud_hidden: {error}"));
        }
        self.inner.ui.hud.hide();
        log("Isla oculta hasta la próxima llamada.");
    }

    /// `_refresh_devices`: enumera los micrófonos para el menú.
    pub fn refresh_devices(&self) {
        // `list_input_devices` ya devuelve una lista vacía si falla el host.
        // La enumeración (PortAudio) se hace fuera del cerrojo para que el menú
        // no espere a que termine.
        let devices = list_input_devices();
        *self.inner.config.devices_cache.lock() = devices;
    }
}

#[cfg(test)]
mod tests;
