//! Ganchos de la app para el arnés: delega en los reales y anota el vigilante.
//!
//! `LiveHooks` conecta cada callback con su método de `VoiceApp`; aquí se envuelve
//! para registrar `watcher.event` antes de entregarlo, igual que el parche de
//! `run_py.py` sobre `app.watcher.on_event`.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::app::core::{AppHooks, VoiceApp};
use crate::app::live::LiveHooks;

use super::recording::Recorder;

/// Ganchos reales con la traza de eventos del vigilante.
pub struct ReplayHooks {
    live: LiveHooks,
    recorder: Arc<Recorder>,
}

impl ReplayHooks {
    /// Envuelve los ganchos reales de `app`.
    pub fn new(app: &VoiceApp, recorder: Arc<Recorder>) -> Self {
        Self {
            live: LiveHooks::new(app),
            recorder,
        }
    }
}

impl AppHooks for ReplayHooks {
    fn watcher_event(&self, address: &str, agent: &str, state: &str, reason: &str) {
        self.recorder.trace().record(
            "watcher.event",
            serde_json::json!({"agent": agent, "state": state, "reason": reason}),
        );
        self.live.watcher_event(address, agent, state, reason);
    }

    fn should_announce(&self, address: &str) -> bool {
        self.live.should_announce(address)
    }

    fn tick(&self) {
        self.live.tick();
    }

    fn start_call(&self) {
        self.live.start_call();
    }

    fn resume_pending_tasks(&self) {
        self.live.resume_pending_tasks();
    }

    fn activity_tick(&self) {
        self.live.activity_tick();
    }

    fn fetch_llm_models(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        api_style: &str,
        force: bool,
    ) {
        self.live
            .fetch_llm_models(provider, base_url, api_key, api_style, force);
    }

    fn refresh_devices(&self) {
        self.live.refresh_devices();
    }

    fn create_tray(&self) {
        // El Python sustituye `_create_tray` por `None`: sin icono de bandeja.
    }

    fn update_tray_title(&self) {
        // Sin bandeja no hay título que actualizar.
    }

    fn chat_media_roots(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        self.live.chat_media_roots()
    }

    fn watch(&self, text: &str) {
        self.live.watch(text);
    }

    fn request_close(&self) {
        self.live.request_close();
    }

    fn mic_mute(&self, muted: bool) {
        self.live.mic_mute(muted);
    }

    fn mute(&self, minutes: i64) {
        self.live.mute(minutes);
    }

    fn last_user_text(&self) -> String {
        self.live.last_user_text()
    }

    fn remember_task(&self, address: &str, agent: &str, task: &str) {
        self.live.remember_task(address, agent, task);
    }

    fn enqueue_chain(&self, spec: &Value) {
        self.live.enqueue_chain(spec);
    }

    fn deliver_answer(&self, answer: &str) {
        self.live.deliver_answer(answer);
    }

    fn prewarm_chain(&self, spec: &Value) {
        self.live.prewarm_chain(spec);
    }
}
