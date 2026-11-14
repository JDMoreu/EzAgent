//! Port of `src/ezagent/voice/chat/render.py` (F5-13).
//!
//! Draws a chat message (markdown) as an image: text, lists, code, tables,
//! images and documents. [`render_message`] is the entry point: it parses,
//! lays out and paints once; the window only crops and composes the result.
//! Clickable zones (copy code, open a document…) come out in
//! [`Rendered::hits`], in logical pixels relative to the message.
//!
//! DECISION: `ez-hud` does not depend on `ez-chat`, so [`render_message`]
//! receives the markdown parser through [`ChatHost`] and the blocks are the
//! local types declared in this module group.

use std::path::PathBuf;

use ez_gfx::draw::Draw;
use ez_gfx::{Image, Rgba};

use crate::tokens::ColorMap;

use super::{
    draw_icon_at, draw_text_at, hit_kind, line, paint_lines, round_rect, text_height, wrap_spans,
    Block, ChatHost, Code, Ctx, Heading, Hit, HitData, IconPainter, ImageBlock, ListItem, Painter,
    Paragraph, Quote, Table, PARA_GAP,
};

/// Height of one code row.
pub const CODE_LINE: f64 = 17.5;
/// Height of the code block header.
pub const CODE_HEAD: f64 = 24.0;
/// Inner padding of the code block.
pub const CODE_PAD: f64 = 12.0;
/// Step per list nesting level.
pub const LIST_STEP: f64 = 18.0;
/// Width reserved for the list marker.
pub const LIST_MARKER: f64 = 18.0;

/// A message already drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    /// RGBA (straight alpha) at the physical size.
    pub image: Image,
    /// Logical height.
    pub height: f64,
    /// Clickable zones.
    pub hits: Vec<Hit>,
    /// The plain text, for copying it.
    pub plain: String,
}

/// Everything a message needs beyond its text.
pub struct RenderOptions<'a> {
    /// DPI factor (`dpi / 96`).
    pub scale: f64,
    /// Allowed read folders (`None` = all).
    pub roots: Option<Vec<PathBuf>>,
    /// Folders relative links are resolved against.
    pub base_dirs: Vec<PathBuf>,
    /// Icon painter; `None` paints no icons.
    pub icons: Option<&'a dyn IconPainter>,
}

impl Default for RenderOptions<'_> {
    fn default() -> Self {
        Self {
            scale: 1.0,
            roots: None,
            base_dirs: Vec::new(),
            icons: None,
        }
    }
}

// ------------------------------------------------------------------- blocks
fn heading(block: &Heading, ctx: &Ctx<'_>) -> (f64, Painter) {
    let key = match block.level {
        1 => "h1",
        2 => "h2",
        _ => "h3",
    };
    let lines = wrap_spans(&block.spans, key, ctx.width, ctx.scale);
    let height = text_height(lines.len(), key);
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        paint_lines(img, ctx, &lines, key, x, y, ctx.color("TEXT", None));
    });
    (height, paint)
}

fn paragraph(block: &Paragraph, ctx: &Ctx<'_>) -> (f64, Painter) {
    let lines = wrap_spans(&block.spans, "body", ctx.width, ctx.scale);
    let height = text_height(lines.len(), "body");
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        paint_lines(img, ctx, &lines, "body", x, y, ctx.color("TEXT", None));
    });
    (height, paint)
}

fn list_item(block: &ListItem, ctx: &Ctx<'_>) -> (f64, Painter) {
    let indent = 4.0 + block.level as f64 * LIST_STEP;
    let lines = wrap_spans(
        &block.spans,
        "body",
        ctx.width - indent - LIST_MARKER,
        ctx.scale,
    );
    let height = text_height(lines.len(), "body");
    let step = ctx.size("body") * 1.5;
    let marker = block.marker.clone();
    let checked = block.checked;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        let cy = y + step / 2.0;
        let mx = x + indent;
        match checked {
            Some(state) => checkbox(img, ctx, mx, cy, state),
            None if marker == "•" => {
                let radius = 2.2;
                Draw::new(img)
                    .ellipse(
                        [
                            f64::from(ctx.px(mx + 4.0 - radius)),
                            f64::from(ctx.px(cy - radius)),
                            f64::from(ctx.px(mx + 4.0 + radius)),
                            f64::from(ctx.px(cy + radius)),
                        ],
                        Some(ctx.color("TEXT_3", None)),
                        None,
                        0,
                    )
                    .expect("the bullet box is ordered");
            }
            None => draw_text_at(
                img,
                ctx,
                mx,
                cy + 5.0,
                &marker,
                "body",
                ctx.color("TEXT_3", None),
                false,
            ),
        }
        paint_lines(
            img,
            ctx,
            &lines,
            "body",
            x + indent + LIST_MARKER,
            y,
            ctx.color("TEXT", None),
        );
    });
    (height, paint)
}

fn checkbox(img: &mut Image, ctx: &Ctx<'_>, x: f64, cy: f64, checked: bool) {
    let box_ = (x, cy - 6.5, x + 13.0, cy + 6.5);
    if checked {
        round_rect(img, ctx, box_, 4.0, Some(ctx.color("MINT", None)), None);
        let points = [
            (x + 3.2, cy + 0.2),
            (x + 5.6, cy + 2.8),
            (x + 10.0, cy - 2.6),
        ]
        .map(|(px, py)| (f64::from(ctx.px(px)), f64::from(ctx.px(py))));
        Draw::new(img).line(
            &points,
            Some(ctx.color("MINT_INK", None)),
            ctx.px(1.6).max(0) as u32,
        );
    } else {
        round_rect(
            img,
            ctx,
            box_,
            4.0,
            None,
            Some(ctx.color("TEXT_3", Some(0.9))),
        );
    }
}

fn quote(block: &Quote, ctx: &Ctx<'_>) -> (f64, Painter) {
    let lines = wrap_spans(&block.spans, "body", ctx.width - 16.0, ctx.scale);
    let height = text_height(lines.len(), "body");
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        round_rect(
            img,
            ctx,
            (x, y + 2.0, x + 3.0, y + height - 2.0),
            1.5,
            Some(ctx.color("MINT", Some(0.7))),
            None,
        );
        paint_lines(
            img,
            ctx,
            &lines,
            "body",
            x + 14.0,
            y,
            ctx.color("TEXT_2", None),
        );
    });
    (height, paint)
}

fn rule(ctx: &Ctx<'_>) -> (f64, Painter) {
    let width = ctx.width;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        line(
            img,
            ctx,
            x,
            y + 5.0,
            x + width,
            y + 6.0,
            ctx.color("LINE", None),
        );
    });
    (12.0, paint)
}

/// `str.expandtabs(4)`, counting code points.
fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0usize;
    for ch in text.chars() {
        match ch {
            '\t' => {
                let spaces = 4 - column % 4;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(ch);
                column = 0;
            }
            _ => {
                out.push(ch);
                column += 1;
            }
        }
    }
    out
}

fn code_lines(ctx: &Ctx<'_>, text: &str) -> Vec<String> {
    let step = (ctx.width - 2.0 * CODE_PAD) / ctx.length("M", "code").max(1.0);
    let chars = (step.trunc() as i64).max(8) as usize;
    let mut out = Vec::new();
    for raw in expand_tabs(text).split('\n') {
        let chars_of: Vec<char> = raw.chars().collect();
        if chars_of.is_empty() {
            out.push(String::new());
            continue;
        }
        for chunk in chars_of.chunks(chars) {
            out.push(chunk.iter().collect());
        }
    }
    out
}

fn code(block: &Code, ctx: &Ctx<'_>) -> (f64, Painter) {
    let rows = code_lines(ctx, &block.text);
    let height = CODE_HEAD + rows.len() as f64 * CODE_LINE + CODE_PAD;
    let lang = block.lang.to_uppercase();
    let text = block.text.clone();
    let width = ctx.width;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        round_rect(
            img,
            ctx,
            (x, y, x + width, y + height),
            9.0,
            Some(ctx.color("BODY", Some(0.55))),
            Some(ctx.color("LINE", None)),
        );
        if !lang.is_empty() {
            draw_text_at(
                img,
                ctx,
                x + CODE_PAD,
                y + 16.0,
                &lang,
                "micro",
                ctx.color("TEXT_3", None),
                false,
            );
        }
        // Copy: an icon (two sheets), not text.
        draw_icon_at(
            img,
            ctx,
            "copy",
            x + width - CODE_PAD - 7.0,
            y + CODE_HEAD / 2.0,
            15.0,
            ctx.color("MINT", None),
        );
        ctx.hits.push(Hit {
            kind: hit_kind::COPY_CODE,
            rect: crate::tokens::Rect::new(
                x + width - CODE_PAD - 22.0,
                y,
                x + width,
                y + CODE_HEAD,
            ),
            data: Some(HitData::Text(text.clone())),
        });
        for (index, row) in rows.iter().enumerate() {
            let baseline = y + CODE_HEAD + (index as f64 + 0.5) * CODE_LINE + 4.2;
            draw_text_at(
                img,
                ctx,
                x + CODE_PAD,
                baseline,
                row,
                "code",
                ctx.color("TEXT", None),
                false,
            );
        }
    });
    (height, paint)
}

// ------------------------------------------------------------------ layout
/// Logical height of a block and the function that paints it at `(x, y)`.
pub fn layout_block(block: &Block, ctx: &Ctx<'_>) -> (f64, Painter) {
    match block {
        Block::Heading(block) => heading(block, ctx),
        Block::Paragraph(block) => paragraph(block, ctx),
        Block::ListItem(block) => list_item(block, ctx),
        Block::Quote(block) => quote(block, ctx),
        Block::Code(block) => code(block, ctx),
        Block::Table(block) => super::render_rich::layout_table(block, ctx),
        Block::Image(block) => super::render_rich::layout_image(block, ctx),
        Block::Document(block) => super::render_rich::layout_document(block, ctx),
        Block::Rule => rule(ctx),
    }
}

fn gap(prev: Option<&Block>, block: &Block) -> f64 {
    let Some(prev) = prev else {
        return 0.0;
    };
    if matches!(prev, Block::ListItem(_)) && matches!(block, Block::ListItem(_)) {
        return 3.0;
    }
    if matches!(block, Block::Heading(_)) {
        return PARA_GAP + 6.0;
    }
    if matches!(prev, Block::Heading(_)) {
        return 4.0;
    }
    PARA_GAP
}

/// RGBA image with every block stacked and its logical height.
pub fn render_blocks(blocks: &[Block], ctx: &mut Ctx<'_>) -> (Image, f64) {
    let mut placed: Vec<(f64, Painter)> = Vec::new();
    let mut y = 0.0;
    let mut prev: Option<&Block> = None;
    for block in blocks {
        y += gap(prev, block);
        let (height, painter) = layout_block(block, ctx);
        placed.push((y, painter));
        y += height;
        prev = Some(block);
    }
    let total = y.max(1.0);
    let width = ctx.px(ctx.width).max(1) as u32;
    let height = ctx.px(total).max(1) as u32;
    let mut image = Image::new(width, height, Rgba::TRANSPARENT);
    for (top, painter) in placed {
        painter(&mut image, ctx, 0.0, top);
    }
    (image, total)
}

/// Parses and draws `text` in a column of `width` logical pixels.
pub fn render_message(
    host: &dyn ChatHost,
    text: &str,
    pal: &ColorMap,
    width: f64,
    options: &RenderOptions<'_>,
) -> Rendered {
    let blocks = host.parse(text);
    let mut ctx = Ctx::new(options.scale, pal, width, host)
        .with_paths(options.roots.clone(), options.base_dirs.clone());
    if let Some(icons) = options.icons {
        ctx = ctx.with_icons(icons);
    }
    let (image, height) = render_blocks(&blocks, &mut ctx);
    Rendered {
        image,
        height,
        hits: ctx.hits,
        plain: plain_text(&blocks),
    }
}

/// The unformatted text (to copy a message to the clipboard).
// DECISION: `markdown.plain_text` lives in `ez-chat`; `ez-hud` cannot depend on
// it, so the 20-line formatter is repeated here over the local block types.
pub fn plain_text(blocks: &[Block]) -> String {
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            Block::Heading(Heading { spans, .. })
            | Block::Paragraph(Paragraph { spans })
            | Block::Quote(Quote { spans }) => {
                out.push(spans.iter().map(|span| span.text.as_str()).collect());
            }
            Block::ListItem(item) => {
                let mark = match item.checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                };
                let body: String = item.spans.iter().map(|span| span.text.as_str()).collect();
                out.push(format!(
                    "{}{} {}{}",
                    "  ".repeat(item.level),
                    item.marker,
                    mark,
                    body
                ));
            }
            Block::Code(Code { text, .. }) => out.push(text.clone()),
            Block::Table(Table { header, rows, .. }) => {
                for row in std::iter::once(header).chain(rows) {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|cell| cell.iter().map(|span| span.text.as_str()).collect())
                        .collect();
                    out.push(cells.join(" | "));
                }
            }
            Block::Image(ImageBlock { alt, src }) => {
                let label = if alt.is_empty() { src } else { alt };
                out.push(format!("[imagen: {label}]"));
            }
            Block::Document(super::Document { label, target }) => {
                out.push(format!("{label} ({target})"));
            }
            Block::Rule => {}
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests;
