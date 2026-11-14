//! Text measurement and line fitting from
//! `src/ezagent/voice/chat/render_base.py` (F2-10).
//!
//! Drawing (`draw_text_at`, `paint_lines`) belongs to the chat view; this
//! module ports the parts that need Pillow's `getlength`: the chat styles,
//! `Ctx.length`, `_pieces`, `wrap`, `_split_word` and `text_height`.
//!
//! DECISION: the span type lives here as [`SpanInput`]; `ez-chat::markdown`
//! cannot be a dependency of `ez-gfx` because the view depends on this crate.

use super::{font, FontStyle};

/// `LINE_K`: interlineado.
pub const LINE_K: f64 = 1.5;
/// `PARA_GAP`: espacio entre bloques.
pub const PARA_GAP: f64 = 8.0;
/// `_EPS`: tolerancia de redondeo al medir texto (px lógicos).
const EPS: f64 = 0.05;

/// `STYLES`: the chat styles declared in `render_base.py`.
pub const STYLES: [(&str, FontStyle); 11] = [
    ("body", FontStyle::new("CHAT_BODY", "Geist", 14.0, 400)),
    ("bold", FontStyle::new("CHAT_BOLD", "Geist", 14.0, 600)),
    ("code", FontStyle::new("CHAT_CODE", "GeistMono", 12.5, 400)),
    ("h1", FontStyle::new("CHAT_H1", "Geist", 19.0, 600)),
    ("h2", FontStyle::new("CHAT_H2", "Geist", 16.5, 600)),
    ("h3", FontStyle::new("CHAT_H3", "Geist", 14.5, 600)),
    ("cell", FontStyle::new("CHAT_CELL", "Geist", 13.0, 400)),
    (
        "cellbold",
        FontStyle::new("CHAT_CELLBOLD", "Geist", 13.0, 600),
    ),
    ("small", FontStyle::new("CHAT_SMALL", "Geist", 12.0, 400)),
    (
        "smallbold",
        FontStyle::new("CHAT_SMALLBOLD", "Geist", 12.0, 600),
    ),
    (
        "micro",
        FontStyle::new("CHAT_MICRO", "GeistMono", 10.5, 500),
    ),
];

/// `STYLES[key]`, with `KeyError` converted into `None`.
pub fn style(key: &str) -> Option<FontStyle> {
    STYLES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, style)| *style)
}

/// `Ctx.length`: logical advance of `text` with the font for `key`.
pub fn length(text: &str, key: &str, scale: f64) -> f64 {
    let style = style(key).expect("the chat styles are closed");
    font::load_font(style, scale).length(text) / scale
}

/// `text_height`: `nlines * size * LINE_K`.
pub fn text_height(nlines: usize, base: &str) -> f64 {
    let style = style(base).expect("the chat styles are closed");
    nlines as f64 * style.size * LINE_K
}

/// One `Span` of the parsed markdown; the chat view builds these.
#[derive(Clone, Copy, Debug)]
pub struct SpanInput<'a> {
    /// Span text.
    pub text: &'a str,
    /// Bold run.
    pub bold: bool,
    /// Italic run.
    pub italic: bool,
    /// Inline code.
    pub code: bool,
    /// Span belongs to a link.
    pub link: bool,
}

/// `_Piece`: a word or whitespace run with its style key.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// Piece text.
    pub text: String,
    /// Chat style key.
    pub key: &'static str,
    /// Italic run.
    pub italic: bool,
    /// Inline code.
    pub code: bool,
    /// Link run.
    pub link: bool,
    /// Whitespace run (never painted).
    pub space: bool,
}

/// `_Run`: a piece placed at `x` on its line.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// Logical x inside the line.
    pub x: f64,
    /// Piece text.
    pub text: String,
    /// Piece that produced the run.
    pub piece: Piece,
    /// Measured width including the code padding.
    pub width: f64,
}

/// `_pieces`: splits spans into words and whitespace runs.
pub fn pieces(spans: &[SpanInput<'_>], base: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    for span in spans {
        if span.code {
            out.push(Piece {
                text: span.text.to_string(),
                key: "code",
                italic: false,
                code: true,
                link: span.link,
                space: false,
            });
            continue;
        }
        let key = if span.bold || base == "bold" {
            "bold"
        } else {
            chat_key(base)
        };
        split_whitespace(span.text, key, span.italic, span.link, &mut out);
    }
    out
}

/// `wrap`: splits pieces into lines of at most `width` logical pixels.
pub fn wrap(pieces: &[Piece], width: f64, scale: f64) -> Vec<Vec<Run>> {
    let mut lines: Vec<Vec<Run>> = vec![Vec::new()];
    let mut x = 0.0;
    for piece in pieces {
        let measured = piece_width(piece, scale);
        if piece.space {
            if let Some(line) = lines.last_mut() {
                if !line.is_empty() {
                    line.push(Run {
                        x,
                        text: piece.text.clone(),
                        piece: piece.clone(),
                        width: measured,
                    });
                    x += measured;
                }
            }
        } else if measured > width + EPS && !piece.code && piece.text.chars().count() > 1 {
            for chunk in split_word(piece, width, scale) {
                let chunk_width = piece_width(&chunk, scale);
                place(&mut lines, &mut x, chunk, chunk_width, width);
            }
        } else {
            place(&mut lines, &mut x, piece.clone(), measured, width);
        }
    }
    while lines.len() > 1 && lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

fn place(lines: &mut Vec<Vec<Run>>, x: &mut f64, piece: Piece, width_piece: f64, width: f64) {
    if *x + width_piece > width + EPS && !lines.last().is_some_and(Vec::is_empty) {
        while lines
            .last()
            .is_some_and(|line| line.last().is_some_and(|run| run.piece.space))
        {
            lines.last_mut().expect("non-empty").pop();
        }
        lines.push(Vec::new());
        *x = 0.0;
    }
    let line = lines.last_mut().expect("a line exists");
    line.push(Run {
        x: *x,
        text: piece.text.clone(),
        piece,
        width: width_piece,
    });
    *x += width_piece;
}

fn piece_width(piece: &Piece, scale: f64) -> f64 {
    let pad = if piece.code { 6.0 } else { 0.0 };
    length(&piece.text, piece.key, scale) + pad
}

/// `_split_word`: cuts a word wider than the box into chunks that fit.
fn split_word(piece: &Piece, width: f64, scale: f64) -> Vec<Piece> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in piece.text.chars() {
        if !current.is_empty() {
            let mut candidate = current.clone();
            candidate.push(ch);
            if length(&candidate, piece.key, scale) > width {
                chunks.push(chunk(&current, piece));
                current.clear();
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(chunk(&current, piece));
    }
    chunks
}

fn chunk(text: &str, piece: &Piece) -> Piece {
    Piece {
        text: text.to_string(),
        key: piece.key,
        italic: piece.italic,
        code: piece.code,
        link: piece.link,
        space: false,
    }
}

/// `_SPLIT = re.compile(r"(\s+)")`, preserving the separators.
fn split_whitespace(text: &str, key: &'static str, italic: bool, link: bool, out: &mut Vec<Piece>) {
    let mut start = 0;
    let mut space: Option<bool> = None;
    for (index, ch) in text.char_indices() {
        let is_space = is_py_space(ch);
        if let Some(previous) = space {
            if previous != is_space {
                out.push(Piece {
                    text: text[start..index].to_string(),
                    key,
                    italic,
                    code: false,
                    link,
                    space: previous,
                });
                start = index;
            }
        }
        space = Some(is_space);
    }
    if let Some(previous) = space {
        out.push(Piece {
            text: text[start..].to_string(),
            key,
            italic,
            code: false,
            link,
            space: previous,
        });
    }
}

fn is_py_space(ch: char) -> bool {
    ch.is_whitespace() || ('\x1c'..='\x1f').contains(&ch)
}

fn chat_key(base: &str) -> &'static str {
    STYLES
        .iter()
        .find(|(name, _)| *name == base)
        .map(|(name, _)| *name)
        .expect("the chat base is a known style")
}

#[cfg(test)]
mod tests;
