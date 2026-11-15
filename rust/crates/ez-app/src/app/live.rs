//! Cableado final de los ganchos de la app y pintor real de iconos (F6-07).
//!
//! `LiveHooks` implementa [`AppHooks`] delegando en los métodos reales de
//! [`VoiceApp`]; guarda un `Weak<Inner>` para no crear un ciclo con el estado
//! que a su vez guarda los ganchos. [`LiveIcons`] conecta el rasterizador de
//! `ez-gfx` con la vista rápida del HUD.

use std::sync::{Arc, Weak};

use ez_hud::paint::peek::{IconPainter, PeekBuffer};
use ez_hud::tokens::Color;
use serde_json::Value;

use super::core::state::Inner;
use super::core::{AppHooks, VoiceApp};

/// Pintor de iconos de producción: `ez_gfx::icons::draw_icon`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveIcons;

impl IconPainter for LiveIcons {
    fn draw_icon(
        &self,
        buf: &mut PeekBuffer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Color,
        scale: f64,
    ) {
        let channels = color.channels();
        let color = match color.alpha() {
            Some(alpha) => ez_gfx::icons::Color::Rgba(channels[0], channels[1], channels[2], alpha),
            None => ez_gfx::icons::Color::Rgb(channels[0], channels[1], channels[2]),
        };
        // Un nombre desconocido devuelve error; el HUD no dibuja nada, igual
        // que el `False` del Python.
        let _ = ez_gfx::icons::draw_icon(
            &mut buf.data,
            buf.width,
            buf.height,
            name,
            cx,
            cy,
            size,
            color,
            scale,
            1.0,
        );
    }
}

/// Ganchos reales del arranque; `None` en los dobles que usan `NullHooks`.
pub struct LiveHooks {
    inner: Weak<Inner>,
}

impl LiveHooks {
    pub fn new(app: &VoiceApp) -> Self {
        Self {
            inner: Arc::downgrade(&app.inner),
        }
    }

    fn app(&self) -> Option<VoiceApp> {
        self.inner.upgrade().map(|inner| VoiceApp { inner })
    }
}

impl AppHooks for LiveHooks {
    fn watcher_event(&self, address: &str, agent: &str, state: &str, reason: &str) {
        if let Some(app) = self.app() {
            app.on_watcher_event(address, agent, state, reason);
        }
    }

    fn should_announce(&self, address: &str) -> bool {
        self.app().is_some_and(|app| app.should_announce(address))
    }

    fn tick(&self) {
        if let Some(app) = self.app() {
            app.tick();
        }
    }

    fn start_call(&self) {
        if let Some(app) = self.app() {
            app.start_call();
        }
    }

    fn resume_pending_tasks(&self) {
        if let Some(app) = self.app() {
            app.resume_pending_tasks();
        }
    }

    fn activity_tick(&self) {
        if let Some(app) = self.app() {
            app.activity_tick();
        }
    }

    fn fetch_llm_models(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        api_style: &str,
        force: bool,
    ) {
        if let Some(app) = self.app() {
            app.fetch_llm_models(provider, base_url, api_key, api_style, force);
        }
    }

    fn refresh_devices(&self) {
        if let Some(app) = self.app() {
            app.refresh_devices();
        }
    }

    fn create_tray(&self) {
        if let Some(app) = self.app() {
            let _ = app.create_tray();
        }
    }

    fn update_tray_title(&self) {
        if let Some(app) = self.app() {
            app.update_tray_title();
        }
    }

    fn chat_media_roots(&self) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
        self.app().map_or_else(
            || (vec![ez_core::dirs::home()], Vec::new()),
            |app| app.chat_media_roots(),
        )
    }

    fn watch(&self, text: &str) {
        if let Some(app) = self.app() {
            app.watch(text);
        }
    }

    fn request_close(&self) {
        if let Some(app) = self.app() {
            app.request_close();
        }
    }

    fn mic_mute(&self, muted: bool) {
        if let Some(app) = self.app() {
            app.llm_mic_mute(muted);
        }
    }

    fn mute(&self, minutes: i64) {
        if let Some(app) = self.app() {
            app.mute(minutes);
        }
    }

    fn last_user_text(&self) -> String {
        self.app()
            .map_or_else(String::new, |app| app.last_user_text())
    }

    fn remember_task(&self, address: &str, agent: &str, task: &str) {
        if let Some(app) = self.app() {
            app.remember_task(address, agent, task);
        }
    }

    fn enqueue_chain(&self, spec: &Value) {
        if let (Some(app), Some(spec)) = (self.app(), spec.as_object()) {
            app.enqueue_chain(spec);
        }
    }

    fn deliver_answer(&self, answer: &str) {
        if let Some(app) = self.app() {
            app.deliver_answer(answer);
        }
    }

    fn prewarm_chain(&self, spec: &Value) {
        if let (Some(app), Some(spec)) = (self.app(), spec.as_object()) {
            app.start_prelaunch(spec);
        }
    }
}

#[cfg(test)]
mod tests;
