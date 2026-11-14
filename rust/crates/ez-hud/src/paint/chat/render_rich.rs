//! Port of `src/ezagent/voice/chat/render_rich.py` (F5-13).
//!
//! Rich chat blocks: tables, images and document cards (with a preview).
//! Nothing opens on its own: a card offers «Abrir» and «Mostrar» and the user
//! decides.
//!
//! DECISION: file lookups go through [`ChatHost`] because `ez-hud` does not
//! depend on `ez-chat::media`; the wiring lands in F5-14/F6.

mod common;
mod documents;
mod images;
mod tables;

#[cfg(test)]
mod tests;

pub use common::{human_size, is_local_target, url_netloc};
pub use documents::layout_document;
pub use images::layout_image;
pub use tables::layout_table;

/// Horizontal padding of a table cell.
pub const CELL_PAD_X: f64 = 10.0;
/// Vertical padding of a table cell.
pub const CELL_PAD_Y: f64 = 7.0;
/// Minimum width of a table column.
pub const CELL_MIN: f64 = 48.0;
/// Height of a document card without a preview.
pub const CARD_H: f64 = 64.0;
/// Maximum height of the preview of a text document.
pub const PREVIEW_MAX: f64 = 190.0;
/// Maximum height of a rendered image.
pub const IMAGE_MAX_H: f64 = 340.0;
