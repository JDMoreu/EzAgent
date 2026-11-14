//! Port of `src/ezagent/voice/hud/chat_view.py` (F5-14).
//!
//! State and drawing of the chat window, without Win32: the view paints into a
//! `float32` premultiplied layer and returns *actions* («send», «close»,
//! «copy»…) instead of touching the app, the clipboard or the window. Header
//! and footer live in the sibling modules.
//!
//! DECISION: `ez-hud` does not depend on `ez-chat`, so the message history and
//! the text field arrive through the [`ChatModel`] and [`TextField`] traits and
//! the messages are the local [`ChatMessage`]/[`SessionInfo`] values. F5-15/F6
//! wire the `ez-chat` types with small adapters.

pub mod canvas;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ez_core::metrics::monotonic;
use ez_core::pyvalue::head_chars;
use ez_gfx::sdf::Layer;
use ez_gfx::text::{Align, FontStyle};
use ez_gfx::Rgba;

use crate::tokens::{ColorMap, Rect};

pub use super::footer::{
    break_line, input_box_height, BTN, HINT_H, INPUT_LINE, INPUT_MAX_LINES, INPUT_MIN, PAD,
};
pub use super::header::{DELETE_SECS, HEADER_H};
pub use super::scroll::{Scroll, SCROLL_STEP};

mod draw;
mod entries;
mod interaction;
mod types;

#[cfg(test)]
pub(crate) mod test_support;

use entries::{Cached, Geometry};
pub use types::{
    Action, ActionData, ChatMessage, ChatModel, ConfirmCard, Mode, Move, Role, SessionInfo,
    TextField, ViewHost,
};
pub(crate) use types::{Deleting, Zone, ZoneData, GFX_ICONS};

/// Space between messages and after the last one.
pub const GAP: f64 = 14.0;
/// Hits that carry their row in the hover key (`session:5`).
pub const KEYED: [&str; 4] = ["session", "delete_ask", "delete_yes", "delete_no"];
/// Logical width of the copy icon zone.
pub const COPY_W: f64 = 30.0;
/// Free space right of the last line so the icon fits.
pub const COPY_SLOT: f64 = 26.0;
/// Extra row under the text when the icon does not fit on the last line.
pub const COPY_ROW: f64 = 24.0;
/// Maximum time between two rich layouts of a streaming message (seconds).
pub const STREAM_REFRESH: f64 = 0.05;

/// State and drawing of the chat window.
pub struct ChatView {
    /// Chat model.
    pub model: Box<dyn ChatModel>,
    /// Markdown parser and file lookups.
    pub(crate) host: Box<dyn ViewHost>,
    /// Input box.
    pub input: Box<dyn TextField>,
    /// Allowed read folders (`None` = all).
    pub roots: Option<Vec<PathBuf>>,
    /// Folders relative links resolve against.
    pub base_dirs: Vec<PathBuf>,
    /// Shortcut painted in the empty state.
    pub hotkey_label: String,
    /// Scroll of the message list and of the history.
    pub scroll: Scroll,
    /// Which screen is shown.
    pub mode: Mode,
    /// Call in progress (mic and speaker buttons).
    pub in_call: bool,
    /// History row waiting for confirmation.
    pub(crate) pending_delete: Option<i64>,
    /// History row fading out after its deletion.
    pub(crate) deleting: Option<Deleting>,
    /// Hover key, if any.
    pub hover: Option<String>,
    /// Size, scale, theme and palette of the last `configure`.
    pub(crate) g: Geometry,
    /// Rich messages already laid out, by message id.
    pub(crate) cache: BTreeMap<i64, Cached>,
    /// Clickable zones of the last `render`, in paint order.
    pub(crate) hits: Vec<Zone>,
    /// Visible lines of the input box: `(start, end)` in the text.
    pub(crate) input_lines: Vec<(usize, usize)>,
    /// Rectangle of the input box.
    pub(crate) input_box: Rect,
    /// When the field last had focus (caret blink).
    pub(crate) focus_t0: f64,
    /// `(copied text, until)`: the copy icon shows a check meanwhile.
    pub(crate) flash: Option<(String, f64)>,
    /// Pose text («Escuchando», «Pensando»…) written by the panel.
    pub status: String,
    /// Live orb of the header: `(cx, cy, diameter)` in logical pixels.
    pub orb: Option<(f64, f64, f64)>,
}

impl ChatView {
    /// View with the Python defaults: no read roots, no base folders and the
    /// «Ctrl+Alt+T» hotkey.
    pub fn new(
        model: Box<dyn ChatModel>,
        host: Box<dyn ViewHost>,
        input: Box<dyn TextField>,
    ) -> Self {
        Self {
            model,
            host,
            input,
            roots: None,
            base_dirs: Vec::new(),
            hotkey_label: "Ctrl+Alt+T".to_string(),
            scroll: Scroll::new(),
            mode: Mode::Chat,
            in_call: false,
            pending_delete: None,
            deleting: None,
            hover: None,
            g: Geometry::default(),
            cache: BTreeMap::new(),
            hits: Vec::new(),
            input_lines: Vec::new(),
            input_box: Rect::new(0.0, 0.0, 0.0, 0.0),
            focus_t0: monotonic(),
            flash: None,
            status: String::new(),
            orb: None,
        }
    }

    /// Sets the allowed roots and the folders relative links resolve against.
    pub fn with_paths(mut self, roots: Option<Vec<PathBuf>>, base_dirs: Vec<PathBuf>) -> Self {
        self.roots = roots;
        self.base_dirs = base_dirs;
        self
    }

    /// Sets the shortcut painted in the empty state.
    pub fn with_hotkey(mut self, label: impl Into<String>) -> Self {
        self.hotkey_label = label.into();
        self
    }

    /// Size, scale and colours; if they change the messages are laid out again.
    ///
    /// Messages depend on the width, not the height: stretching the window up
    /// or down does not rebuild them.
    pub fn configure(&mut self, width: f64, height: f64, scale: f64, theme: &str, pal: ColorMap) {
        let key = (round_digits(width, 1), round_digits(scale, 3), theme);
        let old = &self.g;
        if (
            round_digits(old.width, 1),
            round_digits(old.scale, 3),
            old.theme.as_str(),
        ) != key
            || old.pal != pal
        {
            self.cache.clear();
        }
        self.g = Geometry {
            width,
            height,
            scale,
            theme: theme.to_string(),
            pal,
        };
    }

    /// Width available to the messages.
    pub fn content_w(&self) -> f64 {
        self.g.width - 2.0 * PAD
    }

    /// Palette colour as an [`Rgba`] value.
    pub(crate) fn color(&self, name: &str) -> Rgba {
        let entry = self
            .g
            .pal
            .get(name)
            .unwrap_or_else(|| panic!("the palette has {name}"));
        canvas::rgba(*entry)
    }

    /// Text of the panel at the logical point `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text(
        &self,
        buf: &mut Layer,
        value: &str,
        style: FontStyle,
        color: Rgba,
        x: f64,
        y: f64,
        align: Align,
    ) {
        canvas::text(buf, value, style, color, x, y, self.g.scale, align);
    }

    /// Anti-aliased rounded fill.
    pub(crate) fn fill(&self, buf: &mut Layer, rect: Rect, radius: f64, color: Rgba, alpha: f64) {
        canvas::fill_round_rect(buf, rect, radius, color, self.g.scale, alpha);
    }

    /// Hairline ring of a rounded rectangle.
    pub(crate) fn stroke(&self, buf: &mut Layer, rect: Rect, radius: f64, color: Rgba, alpha: f64) {
        canvas::stroke_round_rect(buf, rect, radius, color, self.g.scale, alpha);
    }

    /// Anti-aliased filled circle.
    pub(crate) fn circle(
        &self,
        buf: &mut Layer,
        cx: f64,
        cy: f64,
        diameter: f64,
        color: Rgba,
        alpha: f64,
    ) {
        canvas::fill_circle(buf, cx, cy, diameter, color, self.g.scale, alpha);
    }

    /// Vector icon centred at `(cx, cy)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn icon(
        &self,
        buf: &mut Layer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Rgba,
        alpha: f64,
    ) {
        canvas::icon(buf, name, cx, cy, size, color, self.g.scale, alpha);
    }

    /// Markdown context of a column `width` logical pixels wide.
    pub(crate) fn ctx(&self, width: f64) -> crate::paint::chat::Ctx<'_> {
        crate::paint::chat::Ctx::new(self.g.scale, &self.g.pal, width, self.host.as_ref())
            .with_paths(self.roots.clone(), self.base_dirs.clone())
            .with_icons(&GFX_ICONS)
    }

    // ------------------------------------------------------------- drawing
    /// The whole panel without its background: header, body and footer.
    pub fn render(&mut self, now: f64) -> Layer {
        let mut buf = Layer::new(
            canvas::size_px(self.g.width * self.g.scale),
            canvas::size_px(self.g.height * self.g.scale),
        );
        self.hits.clear();
        let footer = self.footer_height();
        if self.mode == Mode::History {
            self.draw_history(&mut buf, HEADER_H, self.g.height - footer, now);
        } else {
            self.draw_messages(&mut buf, now, HEADER_H, self.g.height - footer);
        }
        self.draw_header(&mut buf);
        self.draw_footer(&mut buf, now, footer);
        buf
    }

    /// History row waiting for confirmation to be deleted.
    pub fn pending_delete(&self) -> Option<i64> {
        self.pending_delete
    }

    /// Whether the panel must keep repainting.
    pub fn animating(&mut self, now: f64) -> bool {
        let flashing = self.flash.as_ref().is_some_and(|(_, until)| now < *until);
        let mut fading = self.deleting.is_some();
        if let Some(deleting) = &self.deleting {
            if now - deleting.2 >= DELETE_SECS {
                self.deleting = None;
                fading = false;
            }
        }
        self.model.busy() || self.model.confirm().is_some() || flashing || fading
    }

    /// Shows for a moment that `text` was copied («COPIADO» where «COPIAR» was).
    pub fn flash_copied(&mut self, text: &str, now: Option<f64>) {
        let until = now.unwrap_or_else(monotonic) + 1.6;
        self.flash = Some((text.to_string(), until));
    }

    /// Fingerprint of the copy notice for the image cache.
    pub fn flash_token(&self) -> (String, bool) {
        match &self.flash {
            Some((text, until)) => (head_chars(text, 20).to_string(), monotonic() < *until),
            None => (String::new(), false),
        }
    }

    /// Whether it makes sense to start another conversation (the current one
    /// is not empty).
    pub fn can_start_new(&self) -> bool {
        self.model.viewing().is_some() || !self.model.snapshot().1.is_empty()
    }

    /// Registers a clickable zone of the panel.
    pub(crate) fn hit(&mut self, rect: Rect, kind: &'static str, data: Option<ZoneData>) {
        self.hits.push(Zone { kind, rect, data });
    }
}

// DECISION: Python rounds decimal digits (`round(x, n)`); scaling by 10^n and
// rounding the ties to even reproduces it for the window sizes the HUD uses.
fn round_digits(value: f64, digits: u32) -> f64 {
    let factor = 10f64.powi(digits as i32);
    (value * factor).round_ties_even() / factor
}

#[cfg(test)]
mod tests;
