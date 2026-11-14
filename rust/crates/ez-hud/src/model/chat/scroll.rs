//! Message scroll of the chat, from `chat_view.py` (F5-14).
//!
//! The Python view keeps `scroll` and `_max_scroll` as fields and walks the
//! wheel in `wheel`. They live here as one value so the view and its footer and
//! header can share it without repeating the clamps.

use crate::tokens::Rect;

/// Logical pixels per notch of the wheel (`SCROLL_STEP`).
pub const SCROLL_STEP: f64 = 56.0;
/// Smallest scrollbar thumb, so it stays visible on very long conversations.
const THUMB_MIN: f64 = 28.0;
/// Free space above the thumb's travel (`top + 4`).
const TRACK_INSET: f64 = 4.0;
/// Free space above and below the thumb inside the viewport.
const TRACK_GAP: f64 = 8.0;
/// Width of the thin bar and its distance to the window edge.
const BAR_W: f64 = 3.0;
const BAR_X: f64 = 6.0;

/// Scroll state: distance from the last message (0 = glued to the bottom) and
/// the maximum the layout allows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    offset: f64,
    max: f64,
}

impl Scroll {
    /// New scroll glued to the last message.
    pub fn new() -> Self {
        Self::default()
    }

    /// Logical pixels scrolled up from the end.
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    /// Greatest offset the last layout allowed.
    pub const fn max(&self) -> f64 {
        self.max
    }

    /// Sets the offset, clamped to the current range.
    pub fn set_offset(&mut self, offset: f64) {
        self.offset = offset.clamp(0.0, self.max.max(0.0));
    }

    /// Records a new maximum and clamps the offset, as the view does after
    /// laying the messages out.
    pub fn set_max(&mut self, max: f64) {
        self.max = max.max(0.0);
        self.offset = self.offset.clamp(0.0, self.max);
    }

    /// Back to the last message.
    pub fn reset(&mut self) {
        self.offset = 0.0;
    }

    /// Mouse wheel. `delta > 0` goes up, towards older messages.
    pub fn wheel(&mut self, delta: i32) {
        let steps = f64::from(delta) / 120.0;
        self.offset = (self.offset + steps * SCROLL_STEP).clamp(0.0, self.max);
    }

    /// Scrollbar thumb in window coordinates, or `None` when everything fits.
    pub fn thumb(&self, top: f64, viewport: f64, total: f64, width: f64) -> Option<Rect> {
        if self.max <= 0.0 || total <= 0.0 {
            return None;
        }
        let thumb = THUMB_MIN.max(viewport * viewport / total);
        let travel = viewport - thumb - TRACK_GAP;
        let pos = top + TRACK_INSET + travel * (1.0 - self.offset / self.max.max(1.0));
        let x = width - BAR_X;
        Some(Rect::new(x, pos, x + BAR_W, pos + thumb))
    }
}

#[cfg(test)]
mod tests;
