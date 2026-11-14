//! Port of `src/ezagent/voice/hud/icons.py` (F2-11).
//!
//! One glyph family drawn on a 24×24 grid as strokes (polylines, arcs,
//! outlines) plus a few fills. The rasterizer is analytic: each pixel gets the
//! coverage of its distance to the geometry (`r - d + 0.5`, clamped), so the
//! edge is anti-aliased once at the final size.
//!
//! Masks are `float32` arrays in `0..=1`; sprites are premultiplied RGBA in
//! `float32`, exactly like the NumPy arrays the Python module returns.

mod cache;
mod glyphs;
mod raster;

#[cfg(test)]
mod tests;

use std::cell::RefCell;

use ndarray::{Array2, Array3};

use cache::{Cache, GlyphKey, SlashKey, SlashPair, SpriteKey};
use glyphs::glyph;

pub use raster::segment_distance;

/// Side of the design grid, in logical units.
pub(crate) const ICON_GRID: f64 = 24.0;
/// Stroke width at ordinary sizes.
pub(crate) const ICON_STROKE: f64 = 1.8;
/// Stroke width below [`ICON_SMALL_THRESHOLD`].
pub(crate) const ICON_STROKE_SMALL: f64 = 1.6;
/// Logical size under which the thinner stroke is used.
pub(crate) const ICON_SMALL_THRESHOLD: f64 = 18.0;
/// First and last unit of the diagonal that crosses out a glyph.
const SLASH_FROM: f64 = 4.0;
/// Length of that diagonal, in grid units.
const SLASH_SPAN: f64 = 16.0;
/// Erase radius around the slash, in stroke widths.
const SLASH_GAP: f64 = 1.2;
/// FIFO cap of the mask and sprite caches (`_CACHE_LIMIT`).
const CACHE_LIMIT: usize = 512;

/// Every glyph name, in the order of the Python tuple.
pub const ICON_NAMES: [&str; 28] = [
    "microphone",
    "microphone-slash",
    "phone",
    "hangup",
    "chevron-down",
    "chevron-up",
    "minus",
    "check",
    "speaker",
    "speaker-slash",
    "close",
    "sparkle",
    "mic-line",
    "voice-line",
    "chip",
    "sliders",
    "copy",
    "plus",
    "clock",
    "send",
    "stop",
    "chevron-left",
    "trash",
    "menu",
    "chat",
    "waves",
    "alert",
    "compose",
];

/// RGB or RGBA color in `0..=255`, like `icons.Color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    /// Opaque `(r, g, b)`.
    Rgb(u8, u8, u8),
    /// `(r, g, b, a)`.
    Rgba(u8, u8, u8, u8),
}

impl Color {
    /// `_rgba(color)`: the four channels divided by 255.
    fn normalized(self) -> (f64, f64, f64, f64) {
        match self {
            Self::Rgb(r, g, b) => (
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
                1.0,
            ),
            Self::Rgba(r, g, b, a) => (
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
                f64::from(a) / 255.0,
            ),
        }
    }
}

/// Failure of an icon call; the messages are the Python `ValueError`/`TypeError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconError {
    /// `icono desconocido: {name!r}`.
    Unknown(String),
    /// `size y scale deben ser mayores que cero`.
    NonPositive,
    /// `buf debe ser float32 con forma (H, W, 4)`.
    BufferShape,
    /// `alpha debe estar en [0, 1]`.
    AlphaRange,
}

impl std::fmt::Display for IconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "icono desconocido: {}", python_repr(name)),
            Self::NonPositive => f.write_str("size y scale deben ser mayores que cero"),
            Self::BufferShape => f.write_str("buf debe ser float32 con forma (H, W, 4)"),
            Self::AlphaRange => f.write_str("alpha debe estar en [0, 1]"),
        }
    }
}

impl std::error::Error for IconError {}

thread_local! {
    // DECISION: the Python module-level caches are thread-local here; the HUD
    // draws from its UI thread and `Array2` values are cheap to clone.
    static MASKS: RefCell<Cache<GlyphKey, Array2<f32>>> = RefCell::new(Cache::new(CACHE_LIMIT));
    static SLASHES: RefCell<Cache<SlashKey, SlashPair>> = RefCell::new(Cache::new(CACHE_LIMIT));
    static SPRITES: RefCell<Cache<SpriteKey, Array3<f32>>> = RefCell::new(Cache::new(CACHE_LIMIT));
}

/// Alias table of names the HUD already uses.
fn alias(name: &str) -> &str {
    match name {
        "mic" => "microphone",
        "mic-off" | "mic_off" | "microphone-off" => "microphone-slash",
        "call" => "phone",
        "chevron" | "chevron_down" => "chevron-down",
        "chevron_up" => "chevron-up",
        "dash" => "minus",
        "volume" => "speaker",
        "volume-off" | "speaker-off" | "mute" => "speaker-slash",
        "gear" => "menu",
        other => other,
    }
}

/// `_stroke_px(size, scale)`.
pub(crate) fn stroke_px(size: f64, scale: f64) -> f64 {
    let stroke = if size < ICON_SMALL_THRESHOLD {
        ICON_STROKE_SMALL
    } else {
        ICON_STROKE
    };
    (stroke * scale).max(1.0)
}

/// `int(round(value))`: Python's ties-to-even rounding, truncated to an integer.
pub(crate) fn py_round(value: f64) -> i32 {
    value.round_ties_even() as i32
}

/// `round(value, 3)` as the cache key Python builds.
fn round3(value: f64) -> i64 {
    (value * 1000.0).round_ties_even() as i64
}

/// Coverage (`0..=1`, `float32`) of `name` at `size` logical px.
///
/// `pixels` forces the physical side (callers use it to keep parity with a
/// sprite). The `*-slash` names include their slash and the gap around it.
pub fn icon_mask(
    name: &str,
    size: f64,
    scale: f64,
    pixels: Option<u32>,
) -> Result<Array2<f32>, IconError> {
    let name = alias(name);
    if !ICON_NAMES.contains(&name) {
        return Err(IconError::Unknown(name.to_string()));
    }
    if size <= 0.0 || scale <= 0.0 {
        return Err(IconError::NonPositive);
    }
    let side = match pixels {
        Some(pixels) if pixels != 0 => pixels,
        _ => (py_round(size * scale).max(1)) as u32,
    };
    let stroke = stroke_px(size, scale);
    let key = GlyphKey {
        name: name.to_string(),
        side,
        stroke: round3(stroke),
    };
    if let Some(mask) = MASKS.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(mask);
    }
    let mut mask = raster::rasterize(&glyph(name), side, stroke);
    if name.ends_with("-slash") {
        let (gap, line) = slash_masks(size, scale, 1.0, Some(side))?;
        mask = max_masked(&mask, &gap, &line);
    }
    MASKS.with(|cache| cache.borrow_mut().insert(key, mask.clone()));
    Ok(mask)
}

/// `(gap, slash)` masks of the diagonal that crosses out a glyph, drawn to
/// `progress` (`0..=1`).
pub fn slash_masks(
    size: f64,
    scale: f64,
    progress: f64,
    pixels: Option<u32>,
) -> Result<(Array2<f32>, Array2<f32>), IconError> {
    let side = match pixels {
        Some(pixels) if pixels != 0 => pixels,
        _ => (py_round(size * scale).max(1)) as u32,
    };
    let stroke = stroke_px(size, scale);
    let key = SlashKey {
        side,
        stroke: round3(stroke),
        progress: round3(progress),
    };
    if let Some((gap, line)) = SLASHES.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok((gap, line));
    }
    let (a, b) = raster::slash_points(progress);
    let (px, py) = raster::grid(side);
    let segment = [[a.0 as f32, a.1 as f32, b.0 as f32, b.1 as f32]];
    let distance = raster::segment_distance(&px, &py, &segment);
    let k = (f64::from(side) / ICON_GRID) as f32;
    let gap = raster::coverage_mask(&distance, side, |d| {
        raster::coverage(d * k, (stroke * SLASH_GAP) as f32)
    });
    let line = raster::coverage_mask(&distance, side, |d| {
        raster::coverage(d * k, (stroke / 2.0) as f32)
    });
    SLASHES.with(|cache| {
        cache.borrow_mut().insert(key, (gap.clone(), line.clone()));
    });
    Ok((gap, line))
}

/// `np.maximum(mask * (1 - gap), line)`.
fn max_masked(mask: &Array2<f32>, gap: &Array2<f32>, line: &Array2<f32>) -> Array2<f32> {
    Array2::from_shape_fn(mask.dim(), |(y, x)| {
        (mask[[y, x]] * (1.0 - gap[[y, x]])).max(line[[y, x]])
    })
}

/// Coverage mask to premultiplied `float32` RGBA sprite.
pub fn colorize(mask: &Array2<f32>, color: Color) -> Array3<f32> {
    let (r, g, b, ca) = color.normalized();
    let (r, g, b, ca) = ((r * ca) as f32, (g * ca) as f32, (b * ca) as f32, ca as f32);
    Array3::from_shape_fn((mask.nrows(), mask.ncols(), 4), |(y, x, channel)| {
        let alpha = mask[[y, x]];
        match channel {
            0 => r * alpha,
            1 => g * alpha,
            2 => b * alpha,
            _ => ca * alpha,
        }
    })
}

fn sprite(name: &str, color: Color, size: f64, scale: f64) -> Result<Array3<f32>, IconError> {
    let key = SpriteKey {
        name: alias(name).to_string(),
        color,
        size: size.to_bits(),
        scale: scale.to_bits(),
    };
    if let Some(sprite) = SPRITES.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(sprite);
    }
    let mask = icon_mask(name, size, scale, None)?;
    let sprite = colorize(&mask, color);
    SPRITES.with(|cache| cache.borrow_mut().insert(key, sprite.clone()));
    Ok(sprite)
}

/// Draws `name` centered at `(cx, cy)` over a `float32` RGBA buffer.
///
/// `size` is the logical side of the 24×24 box; `cx`/`cy` are logical too.
/// `color` accepts RGB or RGBA in `0..=255` and `alpha` allows fades. The
/// buffer is row-major with four channels, shape `(height, width, 4)`.
#[allow(clippy::too_many_arguments)]
pub fn draw_icon(
    buf: &mut [f32],
    width: usize,
    height: usize,
    name: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: Color,
    scale: f64,
    alpha: f64,
) -> Result<(), IconError> {
    if width == 0 || height == 0 || buf.len() != width * height * 4 {
        return Err(IconError::BufferShape);
    }
    if !(0.0..=1.0).contains(&alpha) {
        return Err(IconError::AlphaRange);
    }
    if alpha <= 0.0 {
        return Ok(());
    }

    let sprite = sprite(name, color, size, scale)?;
    let (sh, sw) = (sprite.shape()[0], sprite.shape()[1]);
    let x0 = py_round(cx * scale - sw as f64 / 2.0);
    let y0 = py_round(cy * scale - sh as f64 / 2.0);

    let sx0 = (-x0).max(0);
    let sy0 = (-y0).max(0);
    let dx0 = x0.max(0);
    let dy0 = y0.max(0);
    let copy_w = (sw as i32 - sx0).min(width as i32 - dx0);
    let copy_h = (sh as i32 - sy0).min(height as i32 - dy0);
    if copy_w <= 0 || copy_h <= 0 {
        return Ok(());
    }

    for row in 0..copy_h as usize {
        let dy = dy0 as usize + row;
        let sy = sy0 as usize + row;
        for column in 0..copy_w as usize {
            let dx = dx0 as usize + column;
            let sx = sx0 as usize + column;
            let src_a = sprite[[sy, sx, 3]] * alpha as f32;
            let one = 1.0 - src_a;
            let index = (dy * width + dx) * 4;
            for channel in 0..3 {
                buf[index + channel] =
                    sprite[[sy, sx, channel]] * alpha as f32 + buf[index + channel] * one;
            }
            buf[index + 3] = src_a + buf[index + 3] * one;
        }
    }
    Ok(())
}

/// Clears the mask and sprite caches of the calling thread.
pub fn clear_cache() {
    MASKS.with(|cache| cache.borrow_mut().clear());
    SLASHES.with(|cache| cache.borrow_mut().clear());
    SPRITES.with(|cache| cache.borrow_mut().clear());
}

/// Python `repr(str)`, needed byte for byte by the unknown-icon message.
fn python_repr(text: &str) -> String {
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
