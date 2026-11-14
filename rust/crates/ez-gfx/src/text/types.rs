//! Public text types: alignment, colour, styles and errors.

use std::fmt;

/// Horizontal anchor of a text run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// `x` is the left edge of the ink.
    #[default]
    Left,
    /// `x` is the horizontal centre of the ink.
    Center,
    /// `x` is the right edge of the ink.
    Right,
}

impl Align {
    /// `_ALIGNS` lookup; `None` is the Python `align not in _ALIGNS` branch.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}
/// An RGB or RGBA colour with 0-255 channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Opaque colour.
    Rgb([u8; 3]),
    /// Colour with its own alpha.
    Rgba([u8; 4]),
}

impl Color {
    pub(crate) fn rgb(self) -> [u8; 3] {
        match self {
            Self::Rgb(rgb) => rgb,
            Self::Rgba([r, g, b, _]) => [r, g, b],
        }
    }

    pub(crate) fn alpha(self) -> u8 {
        match self {
            Self::Rgb(_) => 255,
            Self::Rgba([_, _, _, a]) => a,
        }
    }
}

impl From<[u8; 3]> for Color {
    fn from(rgb: [u8; 3]) -> Self {
        Self::Rgb(rgb)
    }
}

impl From<[u8; 4]> for Color {
    fn from(rgba: [u8; 4]) -> Self {
        Self::Rgba(rgba)
    }
}

/// Closed typographic style of the HUD (`FontStyle` in `tokens.py`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontStyle {
    /// Token name (`FS_TITLE`, ...).
    pub token: &'static str,
    /// Font family (`Geist`, `GeistMono`).
    pub family: &'static str,
    /// Logical size in pixels at 96 DPI.
    pub size: f64,
    /// Font weight.
    pub weight: i32,
    /// Tracking in `em` when no explicit value is given.
    pub tracking_em: f64,
    /// Whether text is upper-cased before rasterization.
    pub uppercase: bool,
}

impl FontStyle {
    /// Style without tracking or capitals, like the Python dataclass.
    pub const fn new(token: &'static str, family: &'static str, size: f64, weight: i32) -> Self {
        Self {
            token,
            family,
            size,
            weight,
            tracking_em: 0.0,
            uppercase: false,
        }
    }

    /// `FontStyle.mono`: whether the family is the monospaced one.
    pub fn mono(self) -> bool {
        self.family == "GeistMono"
    }
}

/// `FS_TITLE` de `tokens.py`.
pub const FS_TITLE: FontStyle = FontStyle::new("FS_TITLE", "Geist", 16.0, 600);
/// `FS_ROW` de `tokens.py`.
pub const FS_ROW: FontStyle = FontStyle::new("FS_ROW", "Geist", 14.0, 400);
/// `FS_BODY` de `tokens.py`.
pub const FS_BODY: FontStyle = FontStyle::new("FS_BODY", "Geist", 13.0, 400);
/// `FS_SECONDARY` de `tokens.py`.
pub const FS_SECONDARY: FontStyle = FontStyle::new("FS_SECONDARY", "Geist", 12.0, 600);
/// `FS_MICRO` de `tokens.py`.
pub const FS_MICRO: FontStyle = FontStyle {
    token: "FS_MICRO",
    family: "GeistMono",
    size: 11.0,
    weight: 500,
    tracking_em: 0.10,
    uppercase: true,
};

/// `FONT_STYLES`: the five HUD styles by token.
pub const FONT_STYLES: [(&str, FontStyle); 5] = [
    ("FS_TITLE", FS_TITLE),
    ("FS_ROW", FS_ROW),
    ("FS_BODY", FS_BODY),
    ("FS_SECONDARY", FS_SECONDARY),
    ("FS_MICRO", FS_MICRO),
];

/// `FONT_STYLES[key]`, with `KeyError` converted into `None`.
pub fn font_style(token: &str) -> Option<FontStyle> {
    FONT_STYLES
        .iter()
        .find(|(key, _)| *key == token)
        .map(|(_, style)| *style)
}

/// Either a `FontStyle` or a token resolved through `FONT_STYLES`.
#[derive(Clone, Copy, Debug)]
pub enum Style<'a> {
    /// Token such as `FS_BODY` or `body`; it is normalized first.
    Named(&'a str),
    /// Already resolved style.
    Resolved(FontStyle),
}

impl<'a> From<&'a str> for Style<'a> {
    fn from(name: &'a str) -> Self {
        Self::Named(name)
    }
}

impl<'a> From<&'a String> for Style<'a> {
    fn from(name: &'a String) -> Self {
        Self::Named(name)
    }
}

impl From<FontStyle> for Style<'_> {
    fn from(style: FontStyle) -> Self {
        Self::Resolved(style)
    }
}

/// Failure of a text helper; the text matches the Python exception.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// `ValueError("estilo de texto desconocido: ...")`.
    UnknownStyle(String),
    /// `ValueError("tracking debe ser un número finito")`.
    NonFiniteTracking,
    /// `ValueError("scale debe ser mayor que cero")`.
    InvalidScale,
    /// `ValueError("alineación desconocida: ...")`.
    UnknownAlign(String),
    /// `TypeError("buf debe ser float32 con forma (H, W, 4)")`.
    InvalidBuffer,
    /// `ValueError("alpha debe estar en [0, 1]")`.
    InvalidAlpha,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStyle(style) => {
                write!(f, "estilo de texto desconocido: {}", py_repr(style))
            }
            Self::NonFiniteTracking => f.write_str("tracking debe ser un número finito"),
            Self::InvalidScale => f.write_str("scale debe ser mayor que cero"),
            Self::UnknownAlign(align) => {
                write!(f, "alineación desconocida: {}", py_repr(align))
            }
            Self::InvalidBuffer => f.write_str("buf debe ser float32 con forma (H, W, 4)"),
            Self::InvalidAlpha => f.write_str("alpha debe estar en [0, 1]"),
        }
    }
}

impl std::error::Error for TextError {}

/// Options shared by measuring and drawing; the defaults are the Python ones.
#[derive(Clone, Copy, Debug)]
pub struct TextOptions {
    /// DPI factor (`dpi / 96`).
    pub scale: f64,
    /// Horizontal anchor.
    pub align: Align,
    /// Tracking in `em`; `None` uses the style's value.
    pub tracking: Option<f64>,
    /// Extra opacity in `[0, 1]`.
    pub alpha: f64,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            align: Align::Left,
            tracking: None,
            alpha: 1.0,
        }
    }
}

/// `repr(str)` for the error texts; the names involved are ASCII.
fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
