//! Port of `src/ezagent/voice/chat/render_base.py` (F5-13).
//!
//! Base of the chat drawing: context, colours, soft shapes and styled text.
//! Everything is laid out in logical pixels and painted at the physical size
//! (`× scale`), like the rest of the HUD, so text stays sharp at any DPI.
//!
//! Measurement and line fitting live in [`ez_gfx::text::chat`] (F2-10) and are
//! reused here; this module ports the drawing half.
//!
//! DECISION: `ez-hud` does not depend on `ez-chat`, so the parsed markdown
//! types (`Span`, `Block`, …) and the file lookups (`ChatHost`) are declared
//! here. The wiring translates the `ez-chat` types in F5-14/F6.

use std::path::{Path, PathBuf};

use ez_gfx::text::FontStyle;
use ez_gfx::{Image, Rgba};

use crate::tokens::{ColorMap, Rect};

mod base;
pub mod labels;
pub mod render;
pub mod render_rich;

pub(crate) use base::{crop, multiply_mask, multiply_masks, put_alpha};
pub use base::{draw_icon_at, draw_text_at, line, paint_lines, round_rect, wrap_spans};
pub use ez_gfx::text::chat::{pieces, text_height, wrap, Piece, Run, SpanInput, LINE_K, PARA_GAP};

/// Kinds of clickable zone a message produces.
pub mod hit_kind {
    /// Copy the code block to the clipboard.
    pub const COPY_CODE: &str = "copy_code";
    /// Open the document with its default application.
    pub const OPEN: &str = "open";
    /// Reveal the document in the file explorer.
    pub const REVEAL: &str = "reveal";
    /// Interaction with a rendered image.
    pub const IMAGE: &str = "image";
}

/// A laid-out block: its logical height and the closure that paints it.
///
/// DECISION: the Python painter captures `ctx`; here it receives it as an
/// argument so the context can record hits while painting.
pub type Painter = Box<dyn Fn(&mut Image, &mut Ctx<'_>, f64, f64)>;

/// A clickable zone of a message, in logical pixels relative to its corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// One of [`hit_kind`].
    pub kind: &'static str,
    /// Zone rectangle.
    pub rect: Rect,
    /// Payload: the code text or the document/image file.
    pub data: Option<HitData>,
}

/// Payload carried by a [`Hit`] (`object` in Python).
#[derive(Clone, Debug, PartialEq)]
pub enum HitData {
    /// The source text of a code block.
    Text(String),
    /// The file behind a document or image card.
    File(FileInfo),
}

/// `media.Kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    /// A supported image.
    Image,
    /// A readable text file.
    Text,
    /// A supported office document.
    Office,
    /// Anything else.
    Other,
}

/// Data of a local file for painting its card (`media.FileInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo {
    /// Absolute path.
    pub path: PathBuf,
    /// File name with extension.
    pub name: String,
    /// Lowercase extension, dot included.
    pub ext: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix seconds).
    pub mtime: f64,
    /// File family.
    pub kind: MediaKind,
    /// Inside the allowed read folders.
    pub allowed: bool,
    /// Safe type to open with one click.
    pub openable: bool,
}

/// A piece of text with its style (`markdown.Span`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Span text.
    pub text: String,
    /// Bold run.
    pub bold: bool,
    /// Italic run.
    pub italic: bool,
    /// Inline code.
    pub code: bool,
    /// Link target, if any.
    pub link: Option<String>,
}

impl Span {
    /// A plain span with only its text.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            code: false,
            link: None,
        }
    }
}

/// Column alignment of a table (`markdown.Table.aligns`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableAlign {
    /// Left edge.
    #[default]
    Left,
    /// Centered.
    Center,
    /// Right edge.
    Right,
}

/// `markdown.Heading`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// Heading level, 1 to 6.
    pub level: i32,
    /// Inline text.
    pub spans: Vec<Span>,
}

/// `markdown.Paragraph`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    /// Inline text.
    pub spans: Vec<Span>,
}

/// `markdown.ListItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// Nesting level, 0-based.
    pub level: usize,
    /// Bullet («•») or number («1.»).
    pub marker: String,
    /// Inline text.
    pub spans: Vec<Span>,
    /// Task state; `None` when it is not a task.
    pub checked: Option<bool>,
}

/// `markdown.Quote`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    /// Inline text.
    pub spans: Vec<Span>,
}

/// `markdown.Code`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    /// Fence language, possibly empty.
    pub lang: String,
    /// Code body.
    pub text: String,
}

/// `markdown.Table`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Header cells.
    pub header: Vec<Vec<Span>>,
    /// Body rows.
    pub rows: Vec<Vec<Vec<Span>>>,
    /// One alignment per column.
    pub aligns: Vec<TableAlign>,
}

/// `markdown.Image` (the name `Image` belongs to the raster type here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageBlock {
    /// Alternative text, used as caption.
    pub alt: String,
    /// Local path or web address.
    pub src: String,
}

/// `markdown.Document`: a line that is only a link to a local file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Visible label.
    pub label: String,
    /// Local target.
    pub target: String,
}

/// A parsed markdown block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A heading.
    Heading(Heading),
    /// A paragraph.
    Paragraph(Paragraph),
    /// A list item.
    ListItem(ListItem),
    /// A quote.
    Quote(Quote),
    /// A fenced code block.
    Code(Code),
    /// A table.
    Table(Table),
    /// A horizontal rule.
    Rule,
    /// An image.
    Image(ImageBlock),
    /// A document card.
    Document(Document),
}

/// External services the renderer needs but does not own.
///
/// DECISION: markdown parsing and file access live in `ez-chat`; `ez-hud`
/// takes them through this seam so the wiring (F5-14/F6) can bridge the two
/// crates without a dependency.
pub trait ChatHost {
    /// Parses markdown into blocks (`markdown.parse`).
    fn parse(&self, text: &str) -> Vec<Block>;
    /// Resolves a local link to an absolute path (`media.resolve_target`).
    fn resolve_target(&self, target: &str, base_dirs: &[PathBuf]) -> Option<PathBuf>;
    /// Reads the file card, or `None` when it is missing (`media.file_info`).
    fn file_info(&self, path: Option<&Path>, roots: Option<&[PathBuf]>) -> Option<FileInfo>;
    /// Loads the image as RGBA, or `None` when it must not be shown
    /// (`media.load_image`).
    fn load_image(&self, info: &FileInfo) -> Option<Image>;
    /// Beginning of a text file with a «there is more» flag
    /// (`media.text_preview`).
    fn text_preview(&self, info: &FileInfo) -> Option<(String, bool)>;
}

/// Vector icon painter (`hud.icons.draw_icon`).
///
/// DECISION: `ez-gfx::icons` is still being written (F2-11); the renderer
/// receives a small trait instead and the default paints nothing.
pub trait IconPainter {
    /// Rasterizes `name` into `side × side` premultiplied float32 RGBA, with
    /// its centre at the logical point `(cx, cy)`.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        tile: &mut [f32],
        side: u32,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Rgba,
        scale: f64,
    );
}

struct NoIcons;

impl IconPainter for NoIcons {
    fn draw(&self, _: &mut [f32], _: u32, _: &str, _: f64, _: f64, _: f64, _: Rgba, _: f64) {}
}

/// Everything the drawing needs: size, colours and which files may be shown.
pub struct Ctx<'a> {
    /// DPI factor (`dpi / 96`).
    pub scale: f64,
    /// Semantic palette.
    pub pal: &'a ColorMap,
    /// Usable logical width.
    pub width: f64,
    /// Allowed read folders (`None` = all).
    pub roots: Option<Vec<PathBuf>>,
    /// Folders relative links are resolved against.
    pub base_dirs: Vec<PathBuf>,
    /// Clickable zones added while painting.
    pub hits: Vec<Hit>,
    /// Markdown parser and file access.
    pub host: &'a dyn ChatHost,
    /// Vector icon painter.
    pub icons: &'a dyn IconPainter,
}

impl<'a> Ctx<'a> {
    /// Context with the Python defaults: no read roots, no base folders and no
    /// icons.
    pub fn new(scale: f64, pal: &'a ColorMap, width: f64, host: &'a dyn ChatHost) -> Self {
        Self {
            scale,
            pal,
            width,
            roots: None,
            base_dirs: Vec::new(),
            hits: Vec::new(),
            host,
            icons: &NoIcons,
        }
    }

    /// Uses `icons` for the copy-button icon.
    pub fn with_icons(mut self, icons: &'a dyn IconPainter) -> Self {
        self.icons = icons;
        self
    }

    /// Sets the allowed roots and the relative-link folders.
    pub fn with_paths(mut self, roots: Option<Vec<PathBuf>>, base_dirs: Vec<PathBuf>) -> Self {
        self.roots = roots;
        self.base_dirs = base_dirs;
        self
    }

    /// Base size of a chat style.
    pub fn size(&self, key: &str) -> f64 {
        style(key).size
    }

    /// `int(round(value * scale))`.
    pub fn px(&self, value: f64) -> i32 {
        round_ties_even(value * self.scale)
    }

    /// Logical advance of `text` with the font for `key`.
    pub fn length(&self, text: &str, key: &str) -> f64 {
        ez_gfx::text::chat::length(text, key, self.scale)
    }

    /// Palette colour; without `alpha` it keeps its own transparency (LINE).
    pub fn color(&self, name: &str, alpha: Option<f64>) -> Rgba {
        let entry = *self
            .pal
            .get(name)
            .unwrap_or_else(|| panic!("the palette has {name}"));
        let [r, g, b] = entry.channels();
        match (alpha, entry.alpha()) {
            (None, Some(a)) => Rgba::new(r, g, b, a),
            (None, None) => Rgba::new(r, g, b, 255),
            (Some(alpha), _) => {
                let value = (255.0 * alpha).clamp(0.0, 255.0).round_ties_even() as u8;
                Rgba::new(r, g, b, value)
            }
        }
    }

    /// Context for drawing inside a narrower box (preview, cells).
    ///
    /// Clickable zones of a nested box are not the message's: it gets its own.
    pub fn child(&self, width: f64) -> Ctx<'a> {
        Ctx {
            scale: self.scale,
            pal: self.pal,
            width,
            roots: self.roots.clone(),
            base_dirs: self.base_dirs.clone(),
            hits: Vec::new(),
            host: self.host,
            icons: self.icons,
        }
    }
}

pub(crate) fn style(key: &str) -> FontStyle {
    ez_gfx::text::chat::style(key).unwrap_or_else(|| panic!("the chat styles have {key}"))
}

fn round_ties_even(value: f64) -> i32 {
    value.round_ties_even() as i32
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    use ez_gfx::Image;

    use super::{Block, ChatHost, FileInfo};

    /// Host that parses nothing and finds no files.
    pub struct EmptyHost;

    impl ChatHost for EmptyHost {
        fn parse(&self, _: &str) -> Vec<Block> {
            Vec::new()
        }

        fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
            None
        }

        fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
            None
        }

        fn load_image(&self, _: &FileInfo) -> Option<Image> {
            None
        }

        fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
            None
        }
    }

    /// Factory palette of the dark theme.
    pub fn palette() -> crate::tokens::ColorMap {
        crate::tokens::palette("dark").expect("the dark palette exists")
    }
}

#[cfg(test)]
mod tests;
