//! `ChatUi` for the real HUD: bar geometry, theme and chat model (F6-08).

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use ez_chat::session::ChatModel;
use ez_hud::model::chat::view::ChatView;
use ez_hud::win::chat::ChatUi;
use ndarray::Array3;

use crate::app::core::state::Inner;
use crate::app::core::{AppHud, VoiceApp};

use super::host::HudHost;
use super::input::HudInput;
use super::model::HudModel;

/// `hud.chat_model`, `hud.chat_roots`, `hud.chat_hotkey` and the bar state in
/// `app.py`, behind `ez-hud`'s seam.
///
/// DECISION: `hud` is weak because `DropUI` owns the chat panel, which owns
/// this value; a strong `Arc` would leak the HUD (Python's cycle collector
/// breaks the same cycle at exit).
pub(super) struct HudChatUi {
    hud: Weak<AppHud>,
    chat: Arc<ChatModel>,
    inner: Weak<Inner>,
    hotkey: String,
    roots: Option<Vec<PathBuf>>,
    base_dirs: Vec<PathBuf>,
}

impl HudChatUi {
    /// Reads the media roots now, as `_chat_media_roots` does in `__init__`.
    pub(super) fn new(app: &VoiceApp, hotkey: String) -> Self {
        let (roots, base_dirs) = app.chat_media_roots();
        Self {
            hud: Arc::downgrade(&app.inner.ui.hud),
            chat: Arc::clone(app.inner.chat.chat.inner()),
            inner: Arc::downgrade(&app.inner),
            hotkey,
            roots: Some(roots),
            base_dirs,
        }
    }
}

impl ChatUi for HudChatUi {
    fn scale(&self) -> f64 {
        self.hud.upgrade().map_or(1.0, |hud| hud.scale())
    }

    fn bar_scale(&self) -> f64 {
        // `ui.bs`: escala de pantalla por el tamaño elegido en Ajustes.
        self.hud.upgrade().map_or(1.0, |hud| hud.bs())
    }

    fn form_size(&self) -> (f64, f64) {
        self.hud.upgrade().map_or((0.0, 0.0), |hud| hud.form_size())
    }

    fn form_anchor(&self) -> Option<(f64, f64)> {
        self.hud.upgrade().and_then(|hud| hud.form_anchor())
    }

    fn layout(&self) -> &str {
        self.hud.upgrade().map_or("pill", |hud| hud.layout())
    }

    fn reduced(&self) -> bool {
        self.hud
            .upgrade()
            .is_none_or(|hud| hud.motion() == "reduced" || !ez_hud::anim::animations_enabled())
    }

    fn theme_name(&self) -> String {
        self.hud
            .upgrade()
            .map_or_else(|| "dark".to_string(), |hud| hud.theme_name())
    }

    fn chat_pose(&self) -> String {
        self.hud
            .upgrade()
            .map_or_else(|| "idle".to_string(), |hud| hud.chat_pose())
    }

    fn in_call(&self) -> bool {
        self.inner
            .upgrade()
            .is_some_and(|inner| inner.session.in_call.load(Ordering::SeqCst))
    }

    fn chat_hotkey(&self) -> String {
        self.hotkey.clone()
    }

    fn chat_size(&self) -> Option<(f64, f64)> {
        self.hud.upgrade().and_then(|hud| hud.chat_size())
    }

    fn remember_chat_size(&self, width: f64, height: f64) {
        if let Some(hud) = self.hud.upgrade() {
            hud.remember_chat_size(width, height);
        }
    }

    fn chat_body_rgb(&self) -> [u8; 3] {
        self.hud
            .upgrade()
            .map_or([0, 0, 0], |hud| hud.chat_body_rgb())
    }

    fn chat_fluid(&self, width: usize, height: usize, now: f64) -> Option<Array3<f32>> {
        let width = i32::try_from(width).unwrap_or(i32::MAX);
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        let buffer = self.hud.upgrade()?.chat_fluid(width, height, now)?;
        let columns = buffer.width;
        let rows = buffer.height;
        // The buffer is already row-major `(rows, columns, 3)`: wrapping it
        // avoids rebuilding 1.9 M values per frame (the morph repaints).
        Array3::from_shape_vec((rows, columns, 3), buffer.data).ok()
    }

    fn set_form_hidden(&self, hidden: bool) {
        if let Some(hud) = self.hud.upgrade() {
            hud.set_form_hidden(hidden);
        }
    }

    fn set_chat_open(&self, open: bool) {
        if let Some(hud) = self.hud.upgrade() {
            hud.set_chat_open(open);
        }
    }

    fn set_scale(&self, scale: f64) {
        if let Some(hud) = self.hud.upgrade() {
            hud.set_scale(scale);
        }
    }

    fn make_view(&self) -> Option<Box<ChatView>> {
        let view = ChatView::new(
            Box::new(HudModel::new(Arc::clone(&self.chat))),
            Box::new(HudHost),
            Box::new(HudInput::new()),
        );
        Some(Box::new(
            view.with_paths(self.roots.clone(), self.base_dirs.clone()),
        ))
    }
}
