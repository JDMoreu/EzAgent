//! Text rendered with FreeType, a port of
//! `src/ezagent/voice/hud/text.py` (F2-10).
//!
//! Five closed styles (`FS_*`), Geist fonts, FreeType's native anti-aliasing
//! and composition onto a float32 premultiplied RGBA buffer. Text is
//! rasterized directly at the physical size `logical size * scale`, so it
//! stays sharp at 100, 150 and 200 % DPI.
//!
//! Coordinates:
//! - `x` and `y` are logical and `y` is the top edge of the ink box.
//! - `align` moves the horizontal anchor: `left` uses `x` as the left edge,
//!   `center` as the centre and `right` as the right edge.
//! - `tracking` is expressed in `em` (0.1 = 0.1 * font size).
//!
//! The bundled `freetype-sys` 0.23 compiles FreeType 2.13.2; Pillow 13 uses
//! 2.14.3. Advances, boxes, metrics and line fitting match exactly; a few
//! glyph masks differ by at most 16/255 of coverage (see the unit tests).
//!
//! DECISION: `FontStyle` and the Geist font table live here instead of
//! importing `ez-hud::tokens`; `ez-hud` depends on this crate, so the
//! dependency cannot point the other way.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use crate::Mask;

pub mod chat;
mod font;
mod render;
mod types;

pub use types::{
    font_style, Align, Color, FontStyle, Style, TextError, TextOptions, FONT_STYLES, FS_BODY,
    FS_MICRO, FS_ROW, FS_SECONDARY, FS_TITLE,
};

/// Mutable float32 RGBA image with premultiplied channels in `0..=1`.
pub struct FloatSurface<'a> {
    /// Row-major pixels, four `f32` per pixel.
    pub data: &'a mut [f32],
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

impl<'a> FloatSurface<'a> {
    /// Wraps a caller-owned buffer.
    pub fn new(data: &'a mut [f32], width: usize, height: usize) -> Self {
        Self {
            data,
            width,
            height,
        }
    }
}

struct TextBitmap {
    alpha: Mask,
    width_px: u32,
    line_h_px: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StyleKey {
    token: &'static str,
    family: &'static str,
    size: u64,
    weight: i32,
    tracking_em: u64,
    uppercase: bool,
}

impl StyleKey {
    fn new(style: FontStyle) -> Self {
        Self {
            token: style.token,
            family: style.family,
            size: float_key(style.size),
            weight: style.weight,
            tracking_em: float_key(style.tracking_em),
            uppercase: style.uppercase,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BitmapKey {
    text: String,
    style: StyleKey,
    scale: u64,
    tracking: u64,
}

const CACHE_LIMIT: usize = 512;

struct BitmapCache {
    entries: HashMap<BitmapKey, Rc<TextBitmap>>,
    order: VecDeque<BitmapKey>,
}

impl BitmapCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

// DECISION: Python's module-level caches become thread-local because FreeType
// faces are `Rc`/`!Send`; the HUD measures and draws from its UI thread, and
// `clear_cache` empties the calling thread's caches.
thread_local! {
    static BITMAP_CACHE: RefCell<BitmapCache> = RefCell::new(BitmapCache::new());
}

fn float_key(value: f64) -> u64 {
    if value == 0.0 {
        0.0f64.to_bits()
    } else {
        value.to_bits()
    }
}

fn resolve_style(style: Style<'_>) -> Result<FontStyle, TextError> {
    match style {
        Style::Resolved(style) => Ok(style),
        Style::Named(name) => {
            let trimmed = py_strip(name);
            let mut key = trimmed.to_uppercase();
            if !key.starts_with("FS_") {
                key.insert_str(0, "FS_");
            }
            font_style(&key).ok_or_else(|| TextError::UnknownStyle(name.to_string()))
        }
    }
}

fn resolve_tracking(style: FontStyle, tracking: Option<f64>) -> Result<f64, TextError> {
    let value = tracking.unwrap_or(style.tracking_em);
    if !value.is_finite() {
        return Err(TextError::NonFiniteTracking);
    }
    Ok(value)
}

fn display_text(text: &str, style: FontStyle) -> String {
    if style.uppercase {
        text.to_uppercase()
    } else {
        text.to_string()
    }
}

fn bitmap_for(text: &str, style: FontStyle, scale: f64, tracking: f64) -> Rc<TextBitmap> {
    let key = BitmapKey {
        text: text.to_string(),
        style: StyleKey::new(style),
        scale: float_key(scale),
        tracking: float_key(tracking),
    };
    if let Some(bitmap) = BITMAP_CACHE.with(|cache| cache.borrow().entries.get(&key).cloned()) {
        return bitmap;
    }
    let bitmap = Rc::new(render::render_bitmap(text, style, scale, tracking));
    BITMAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.entries.len() >= CACHE_LIMIT {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
        cache.entries.insert(key.clone(), bitmap.clone());
        cache.order.push_back(key);
    });
    bitmap
}

/// Logical ink size `(width, height)`.
///
/// `scale` is the DPI factor (`dpi / 96`). Text without ink (only spaces)
/// returns the advance and the typographic line height.
pub fn text_size<'a>(
    text: &str,
    style: impl Into<Style<'a>>,
    scale: f64,
    tracking: Option<f64>,
    align: &str,
) -> Result<(f64, f64), TextError> {
    if scale <= 0.0 {
        return Err(TextError::InvalidScale);
    }
    if Align::parse(align).is_none() {
        return Err(TextError::UnknownAlign(align.to_string()));
    }
    let style = resolve_style(style.into())?;
    let track = resolve_tracking(style, tracking)?;
    let display = display_text(text, style);
    let bitmap = bitmap_for(&display, style, scale, track);
    if bitmap.alpha.width() > 0 {
        Ok((
            f64::from(bitmap.alpha.width()) / scale,
            f64::from(bitmap.alpha.height()) / scale,
        ))
    } else {
        Ok((
            f64::from(bitmap.width_px) / scale,
            f64::from(bitmap.line_h_px) / scale,
        ))
    }
}

/// Height of the capitals in logical pixels (the band the eye centres on).
pub fn cap_height<'a>(style: impl Into<Style<'a>>, scale: f64) -> Result<f64, TextError> {
    let style = resolve_style(style.into())?;
    if let Some(span) = font::ink_span(style, scale, "H") {
        return Ok(f64::from(span.1 - span.0) / scale);
    }
    Ok(style.size * 0.7)
}

/// `y` for [`draw_text`] that leaves `text` on `baseline` (logical pixels).
///
/// `draw_text` puts the top edge of the ink at `y`, which depends on the
/// letters: «ace» ended up higher than «Abc». This anchors everything to the
/// baseline.
pub fn baseline_top<'a>(
    text: &str,
    style: impl Into<Style<'a>>,
    baseline: f64,
    scale: f64,
) -> Result<f64, TextError> {
    let style = resolve_style(style.into())?;
    let display = display_text(text, style);
    let measure = if display.is_empty() { "H" } else { &display };
    let font = font::load_font(style, scale);
    if let Some(ink_top) = font.ink_top(measure) {
        return Ok(baseline + (f64::from(ink_top) - f64::from(font.metrics().0)) / scale);
    }
    Ok(baseline - style.size * 0.7)
}

/// Draws text on `surface` and returns the drawn logical size.
///
/// The buffer must be float32 premultiplied RGBA with channels `0..=1`.
/// `color` is RGB or RGBA 0-255. The extra `alpha` allows fades without
/// rasterizing again.
pub fn draw_text<'a>(
    surface: &mut FloatSurface<'_>,
    text: &str,
    style: impl Into<Style<'a>>,
    color: Color,
    position: (f64, f64),
    options: TextOptions,
) -> Result<(f64, f64), TextError> {
    if options.scale <= 0.0 {
        return Err(TextError::InvalidScale);
    }
    validate_buffer(surface)?;
    if !(0.0..=1.0).contains(&options.alpha) {
        return Err(TextError::InvalidAlpha);
    }

    let style = resolve_style(style.into())?;
    let track = resolve_tracking(style, options.tracking)?;
    let display = display_text(text, style);
    let rgb = color.rgb();
    let draw_alpha = options.alpha * f64::from(color.alpha()) / 255.0;
    if draw_alpha <= 0.0 || display.is_empty() {
        return Ok((0.0, 0.0));
    }

    let bitmap = bitmap_for(&display, style, options.scale, track);
    let drawn = (
        f64::from(bitmap.alpha.width()) / options.scale,
        f64::from(bitmap.alpha.height()) / options.scale,
    );
    if bitmap.alpha.width() == 0 {
        return Ok((
            f64::from(bitmap.width_px) / options.scale,
            f64::from(bitmap.line_h_px) / options.scale,
        ));
    }
    let (x0, y0) = anchor(bitmap.alpha.width(), position, options);
    composite(surface, &bitmap, x0, y0, draw_alpha as f32, rgb);
    Ok(drawn)
}

fn validate_buffer(surface: &FloatSurface<'_>) -> Result<(), TextError> {
    let expected = surface
        .width
        .checked_mul(surface.height)
        .and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(surface.data.len()) {
        return Err(TextError::InvalidBuffer);
    }
    Ok(())
}

fn anchor(mask_width: u32, position: (f64, f64), options: TextOptions) -> (i64, i64) {
    let x = position.0 * options.scale;
    let x0 = match options.align {
        Align::Left => py_round(x),
        Align::Center => py_round(x - f64::from(mask_width) / 2.0),
        Align::Right => py_round(x - f64::from(mask_width)),
    };
    (x0, py_round(position.1 * options.scale))
}

fn composite(
    surface: &mut FloatSurface<'_>,
    bitmap: &TextBitmap,
    x0: i64,
    y0: i64,
    draw_alpha: f32,
    rgb: [u8; 3],
) {
    let mask_w = i64::from(bitmap.alpha.width());
    let mask_h = i64::from(bitmap.alpha.height());
    let dst_w = surface.width as i64;
    let dst_h = surface.height as i64;
    let sx0 = (-x0).max(0);
    let sy0 = (-y0).max(0);
    let dx0 = x0.max(0);
    let dy0 = y0.max(0);
    let copy_w = (mask_w - sx0).min(dst_w - dx0);
    let copy_h = (mask_h - sy0).min(dst_h - dy0);
    if copy_w <= 0 || copy_h <= 0 {
        return;
    }

    let rgb = [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
    ];
    for row in 0..copy_h {
        for col in 0..copy_w {
            let coverage =
                f32::from(bitmap.alpha.get((sx0 + col) as u32, (sy0 + row) as u32)) / 255.0;
            let a = coverage * draw_alpha;
            let one_minus = 1.0 - a;
            let index = (((dy0 + row) as usize * surface.width) + (dx0 + col) as usize) * 4;
            for (channel, value) in rgb.iter().enumerate() {
                surface.data[index + channel] =
                    value * a + surface.data[index + channel] * one_minus;
            }
            surface.data[index + 3] = a + surface.data[index + 3] * one_minus;
        }
    }
}

/// Empties the caches. Call it when the DPI or the theme changes.
pub fn clear_cache() {
    BITMAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.entries.clear();
        cache.order.clear();
    });
    font::clear_font_cache();
}

/// `round()`: ties to even, like Python.
fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `str.strip()`: Unicode whitespace plus the separators Python includes.
fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod vectors;
