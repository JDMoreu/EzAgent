//! Port of `src/ezagent/voice/hud/chat_ui.py` and `chat_paint.py` (F5-15).
//!
//! The chat panel: the bar shape (sphere or pill) morphs into a rounded panel,
//! its content appears row by row, and the header orb and status line follow
//! the live bar pose. Windows, lifecycle and input routing live in
//! [`window`]; the pixel composition lives in [`compose`].
//!
//! This module group is inside `ez-hud::win`, so the small amount of Win32
//! that the chat needs is allowed here (AGENTS.rust.md §5).

pub mod compose;
pub mod window;

pub use crate::geometry::{
    ease, lerp, lerp_rect, panel_limits, panel_rect, panel_size, rounded_coverage, smoothstep,
    FADE_SECS, MARGIN, MIN_H, MIN_W, MORPH_SECS, PANEL_H, PANEL_RADIUS, PANEL_W, SCREEN_PAD,
};
pub use compose::{fluid_array, row_reveal, shape_progress, Frame};
pub use window::{
    status_text, ChatEvent, ChatEvents, ChatPanel, ChatUi, ChatWindow, ACTIVE_POSES, CLOSE_SECS,
    STATUS_TEXT,
};

#[cfg(test)]
pub(crate) mod test_support;
