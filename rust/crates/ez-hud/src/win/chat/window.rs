//! Port of `src/ezagent/voice/hud/chat_ui.py` (F5-15): the chat panel.
//!
//! `ChatPanel` owns the lifecycle (open, animate, close), resizing and the
//! routing of mouse and keyboard; `ChatWindow` is the layered window that
//! presents it. The content (header, messages, input) is painted by
//! [`ChatView`](crate::model::chat::view::ChatView); here it is composed with
//! the morph of the bar shape into the panel.
//!
//! The connection with the app is a trait ([`ChatUi`]) for the state the panel
//! reads and the event sink ([`ChatEvents`]) for the callbacks `app.py`
//! implements (send, voice, settings, new conversation, switch, delete,
//! confirm, open and reveal). F6 wires the real HUD through them.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use ndarray::Array3;
use parking_lot::Mutex;

use ez_core::metrics::monotonic;

use crate::geometry::{Edges, Rect, MARGIN};
use crate::model::chat::view::ChatView;
use crate::paint::chat::FileInfo;
use crate::paint::hotkey_field::parts_for;
use crate::tokens::palette_version;

use super::compose::{self, Frame};
use anim::{step, update_live};

mod actions;
pub mod anim;
mod chrome;
mod input;
mod lifecycle;
#[cfg(test)]
mod tests;

pub use chrome::ChatWindow;

/// Cerrar es más rápido que abrir.
pub const CLOSE_SECS: f64 = 0.20;

/// Poses that keep the orb and the status line alive.
pub const ACTIVE_POSES: [&str; 4] = ["listen", "think", "speak", "ask"];

/// Rótulo de estado por pose, copiado literal del Python.
pub const STATUS_TEXT: [(&str, &str); 5] = [
    ("listen", "Escuchando…"),
    ("think", "Pensando…"),
    ("speak", "Hablando…"),
    ("ask", "Esperando tu respuesta"),
    ("error", "Algo salió mal"),
];

/// Rótulo de estado por pose, copiado literal del Python.
pub fn status_text(pose: &str) -> &'static str {
    STATUS_TEXT
        .iter()
        .find(|(name, _)| *name == pose)
        .map_or("", |(_, text)| *text)
}

/// `_hotkey_label`: the shortcut painted in the empty state.
pub fn hotkey_label(spec: &str) -> String {
    let parts = if spec.is_empty() {
        Vec::new()
    } else {
        parts_for(spec)
    };
    if parts.is_empty() {
        String::from("Ctrl+Alt+T")
    } else {
        parts.join("+")
    }
}

/// Current panel animation (`_Anim.state`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimState {
    /// Closed and torn down.
    #[default]
    Closed,
    /// Growing from the bar shape into the panel.
    Opening,
    /// Fully open.
    Open,
    /// Shrinking back into the bar shape.
    Closing,
}

/// `_Anim`: state, start instant and eased progress (0 = shape, 1 = panel).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anim {
    /// Lifecycle state.
    pub state: AnimState,
    /// Start instant (`time.monotonic()` seconds).
    pub t0: f64,
    /// Eased progress.
    pub p: f64,
}

/// `_Resize`: edges being dragged plus the pointer and panel at the start.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resize {
    edges: Edges,
    cursor: (i32, i32),
    rect: Rect,
}

/// State the panel reads from the bar and the app (`ChatPanel.ui` in Python).
///
/// DECISION: `ez-hud` cannot depend on the app or on `ez-chat`, so the model,
/// parser and text field arrive already wrapped in a [`ChatView`] built by
/// [`ChatUi::make_view`]. The factory must move them in, not share non-Send
/// state with the caller, because the view is later touched from the window
/// thread under the panel lock.
pub trait ChatUi: Send + Sync {
    /// Bar scale (`ui.s`): DPI factor times the chosen UI scale.
    fn scale(&self) -> f64;
    /// Shape scale (`ui.bs`).
    fn bar_scale(&self) -> f64;
    /// Logical size of the colour shape (sphere or pill).
    fn form_size(&self) -> (f64, f64);
    /// Centre of the colour shape in physical screen pixels.
    fn form_anchor(&self) -> Option<(f64, f64)>;
    /// `"circle"` or `"pill"`.
    fn layout(&self) -> &str;
    /// Reduced motion: fade instead of morph.
    fn reduced(&self) -> bool;
    /// Resolved theme name (`_theme_name`).
    fn theme_name(&self) -> String;
    /// Current bar pose (`chat_pose`).
    fn chat_pose(&self) -> String;
    /// A call is in progress (`_in_call`).
    fn in_call(&self) -> bool;
    /// Shortcut specification (`chat_hotkey` in `voice.json`).
    fn chat_hotkey(&self) -> String;
    /// Last panel size remembered in `voice.json`.
    fn chat_size(&self) -> Option<(f64, f64)>;
    /// Saves the panel size after resizing.
    fn remember_chat_size(&self, width: f64, height: f64);
    /// Body colour of the panel (palette `BODY`).
    fn chat_body_rgb(&self) -> [u8; 3];
    /// Marea fluid stretched to `width`×`height` in 0..1 (`chat_fluid`).
    fn chat_fluid(&self, width: usize, height: usize, now: f64) -> Option<Array3<f32>>;
    /// Tells the bar that the panel is covering the shape (`set_form_hidden`).
    fn set_form_hidden(&self, hidden: bool);
    /// Tells the bar that the chat is open (`set_chat_open`).
    fn set_chat_open(&self, open: bool);
    /// Applies a new DPI scale (`on_dpi_changed`).
    fn set_scale(&self, scale: f64);
    /// Builds the view with its model, host, input, paths and hotkey label.
    ///
    /// It may be called again on a new open; return a new adapter over the
    /// same shared model instead of consuming one-shot state.
    fn make_view(&self) -> Option<Box<ChatView>>;
}

/// Callbacks the panel raises for `app.py`.
///
/// DECISION: Python called methods on the HUD object directly; the port sends
/// these events so `ez-hud` stays free of the app and the wiring (F6) decides
/// how to deliver them (channel, queue or direct call).
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    /// `on_chat_send`: the user sent this text.
    Send(String),
    /// `on_chat_voice`: voice replies turned on or off.
    Voice(bool),
    /// `on_chat_mic`: microphone turned on or off.
    Mic(bool),
    /// `on_chat_menu`: open the settings menu.
    Settings,
    /// `on_chat_new`: start a new conversation.
    New,
    /// `on_chat_stop`: stop the running answer.
    Stop,
    /// `on_chat_switch`: show this saved conversation.
    Switch(i64),
    /// `on_chat_delete`: delete this saved conversation.
    Delete(i64),
    /// `on_chat_confirm`: answer the pending confirmation.
    Confirm(bool),
    /// `media.open_file`: open the document or image.
    Open(FileInfo),
    /// `media.reveal_file`: show the file in the explorer.
    Reveal(FileInfo),
}

/// Sink of the panel callbacks.
pub trait ChatEvents: Send + Sync {
    /// Delivers one callback to the app.
    fn emit(&self, event: ChatEvent);
}

/// A [`ChatView`] moved behind a `Send` boundary.
///
/// SAFETY: the view is created on one thread, moved in whole and afterwards
/// only touched while `PanelState`'s mutex is held (render and window hooks
/// both take it). [`ChatUi::make_view`] transfers ownership; it must not keep
/// or clone non-`Send` internals of the model it passes.
pub(crate) struct SendView(pub(crate) Box<ChatView>);

// SAFETY: the factory moves the view in; every later access happens under
// `PanelState`'s mutex, so no non-Send internals are shared across threads.
unsafe impl Send for SendView {}

#[cfg(test)]
impl SendView {
    /// Moves the view back out (the factory still owns it in tests).
    pub(crate) fn into_inner(self) -> Box<ChatView> {
        self.0
    }
}

impl Deref for SendView {
    type Target = ChatView;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Panel state guarded by the lock Python got from the GIL.
struct PanelState {
    anim: Anim,
    view: Option<SendView>,
    form: Rect,
    base: Option<(compose::Sig, Frame)>,
    content: compose::ContentCache,
    finished: bool,
    resize: Option<Resize>,
    act: f64,
    pal_seen: i64,
    last: f64,
}

impl PanelState {
    fn new() -> Self {
        Self {
            anim: Anim::default(),
            view: None,
            form: (0.0, 0.0, 0.0, 0.0),
            base: None,
            content: compose::ContentCache::default(),
            finished: false,
            resize: None,
            act: 0.0,
            pal_seen: palette_version(),
            last: 0.0,
        }
    }
}

/// Opens, animates and closes the chat; routes input and performs view actions.
pub struct ChatPanel {
    ui: Arc<dyn ChatUi>,
    events: Arc<dyn ChatEvents>,
    state: Mutex<PanelState>,
    window: Mutex<Option<Arc<ChatWindow>>>,
}

impl ChatPanel {
    /// Panel for `ui`; callbacks go to `events`.
    pub fn new(ui: Arc<dyn ChatUi>, events: Arc<dyn ChatEvents>) -> Self {
        Self {
            ui,
            events,
            state: Mutex::new(PanelState::new()),
            window: Mutex::new(None),
        }
    }

    // ------------------------------------------------------------ geometry
    fn margin(&self) -> i32 {
        (MARGIN * self.ui.scale()).round_ties_even() as i32
    }

    /// `_panel`: physical panel rectangle on screen.
    fn panel(&self, window: &ChatWindow) -> Rect {
        let margin = f64::from(self.margin());
        let (x, y) = window.position();
        let (w, h) = window.size();
        (
            f64::from(x) + margin,
            f64::from(y) + margin,
            f64::from(x) + f64::from(w) - margin,
            f64::from(y) + f64::from(h) - margin,
        )
    }

    fn logical_size(&self, window: &ChatWindow) -> (f64, f64) {
        let panel = self.panel(window);
        (
            (panel.2 - panel.0) / self.ui.scale(),
            (panel.3 - panel.1) / self.ui.scale(),
        )
    }

    // ----------------------------------------------------------- drawing
    /// Reconfigures the view when the theme or the custom palette changed.
    fn refresh_view(
        &self,
        view: &mut ChatView,
        pal_seen: &mut i64,
        base: &mut Option<(compose::Sig, Frame)>,
        content: &mut compose::ContentCache,
    ) {
        let version = palette_version();
        let theme = self.ui.theme_name();
        if view.g.theme != theme || *pal_seen != version {
            *pal_seen = version;
            let (width, height) = (view.g.width, view.g.height);
            self.configure_view(view, width, height);
            *base = None;
            *content = compose::ContentCache::default();
        }
    }

    /// `render`: the panel frame for the window, BGRA premultiplied.
    pub fn render(&self) -> Frame {
        let Some(window) = self.window.lock().clone() else {
            return Frame::new(1, 1);
        };
        let mut state = self.state.lock();
        let PanelState {
            anim,
            view,
            form,
            base,
            content,
            resize,
            act,
            pal_seen,
            last,
            finished,
            ..
        } = &mut *state;
        let Some(view) = view.as_mut() else {
            return Frame::new(1, 1);
        };
        self.refresh_view(view, pal_seen, base, content);
        let now = monotonic();
        let p = step(anim, now, self.ui.reduced(), finished);
        update_live(self.ui.as_ref(), view, act, last, now);
        if matches!(anim.state, AnimState::Opening | AnimState::Open) {
            self.ui.set_form_hidden(true);
        }
        let interval = frame_interval(anim.state, view, resize.is_some(), *act, now);
        window.set_interval(interval);
        let (x, y) = window.position();
        let (width, height) = window.size();
        let window_rect = compose::WindowRect {
            x,
            y,
            width: width.max(1) as usize,
            height: height.max(1) as usize,
        };
        let mut scene = compose::Scene {
            ui: self.ui.as_ref(),
            view,
            anim: anim.state,
            form: *form,
            act: *act,
            now,
        };
        if anim.state == AnimState::Open {
            compose::stable(&mut scene, base, content, window_rect)
        } else {
            compose::compose(&mut scene, p, true, window_rect, content)
        }
    }
}

/// `_interval`: 60 fps while animating or working, 30 fps at rest.
fn frame_interval(anim: AnimState, view: &mut ChatView, resizing: bool, act: f64, now: f64) -> f64 {
    let moving =
        matches!(anim, AnimState::Opening | AnimState::Closing) || view.animating(now) || resizing;
    1.0 / if moving || act > 0.02 { 60.0 } else { 30.0 }
}
