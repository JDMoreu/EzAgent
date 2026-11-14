//! Drawing helpers of the chat view: text, icons, soft shapes and blits.
//!
//! DECISION: Python draws the panel on a `float32` premultiplied numpy buffer;
//! the Rust port uses [`ez_gfx::sdf::Layer`] (the same buffer) so the message
//! tiles built by `paint::chat` (byte images) only need one conversion.

use ez_gfx::sdf::{self, Layer};
use ez_gfx::text::{self, Align, FloatSurface, FontStyle, TextOptions};
use ez_gfx::Rgba;

use crate::tokens::Color;

/// `round()` of Python: ties to even, then to `i64`.
pub fn px(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `round()` of Python clamped to an image dimension (`u32`).
pub fn size_px(value: f64) -> u32 {
    value.round_ties_even().max(0.0) as u32
}

/// The palette [`Color`] as an [`Rgba`] value, alpha included.
pub fn rgba(color: Color) -> Rgba {
    match color {
        Color::Rgb([r, g, b]) => Rgba::opaque(r, g, b),
        Color::Rgba([r, g, b, a]) => Rgba::new(r, g, b, a),
    }
}

/// `text.py:draw_text` on the panel buffer.
#[allow(clippy::too_many_arguments)]
pub fn text(
    layer: &mut Layer,
    value: &str,
    style: FontStyle,
    color: Rgba,
    x: f64,
    y: f64,
    scale: f64,
    align: Align,
) {
    if value.is_empty() {
        return;
    }
    let (width, height) = (layer.width() as usize, layer.height() as usize);
    let mut surface = FloatSurface::new(layer.as_raw_mut(), width, height);
    let options = TextOptions {
        scale,
        align,
        tracking: None,
        alpha: 1.0,
    };
    let _ = text::draw_text(
        &mut surface,
        value,
        style,
        text::Color::Rgba(color.to_array()),
        (x, y),
        options,
    );
}

/// `icons.draw_icon` on the panel buffer.
#[allow(clippy::too_many_arguments)]
pub fn icon(
    layer: &mut Layer,
    name: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    let (width, height) = (layer.width() as usize, layer.height() as usize);
    let _ = ez_gfx::icons::draw_icon(
        layer.as_raw_mut(),
        width,
        height,
        name,
        cx,
        cy,
        size,
        ez_gfx::icons::Color::Rgba(color.r, color.g, color.b, color.a),
        scale,
        alpha,
    );
}

/// `menu._fill_round_rect`.
pub fn fill_round_rect(
    layer: &mut Layer,
    rect: crate::tokens::Rect,
    radius: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    sdf::fill_round_rect(
        layer,
        [rect.x0, rect.y0, rect.x1, rect.y1],
        radius,
        color,
        scale,
        alpha,
    );
}

/// `menu._stroke_round_rect`.
pub fn stroke_round_rect(
    layer: &mut Layer,
    rect: crate::tokens::Rect,
    radius: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    sdf::stroke_round_rect(
        layer,
        [rect.x0, rect.y0, rect.x1, rect.y1],
        radius,
        color,
        scale,
        alpha,
    );
}

/// `menu._fill_circle`.
pub fn fill_circle(
    layer: &mut Layer,
    cx: f64,
    cy: f64,
    diameter: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    sdf::fill_circle(layer, cx, cy, diameter, color, scale, alpha);
}

/// `menu._fit_text`: truncates with `…` using the ink width.
pub fn fit_text(value: &str, style: FontStyle, max_width: f64, scale: f64) -> String {
    sdf::fit_text(value, max_width, |candidate| {
        text::text_size(candidate, style, scale, None, "left")
            .map_or(f64::INFINITY, |(width, _)| width)
    })
}

/// Logical advance of `value` with the `body` chat style.
pub fn body_length(value: &str, scale: f64) -> f64 {
    ez_gfx::text::chat::length(value, "body", scale)
}

/// `value[a:b]` counting code points, like Python's slicing.
pub fn slice(value: &str, start: usize, end: usize) -> &str {
    let from = value
        .char_indices()
        .nth(start)
        .map_or(value.len(), |(index, _)| index);
    let to = value
        .char_indices()
        .nth(end)
        .map_or(value.len(), |(index, _)| index);
    &value[from..to.max(from)]
}

/// `view._blit`: copies the visible rows of `src` over `dst`.
pub fn blit(dst: &mut Layer, src: &Layer, x: f64, y: f64, top: f64, bottom: f64, scale: f64) {
    let y0 = px(y * scale);
    let y1 = y0 + i64::from(src.height());
    let lo = px(top * scale);
    let hi = px(bottom * scale);
    if y1 <= lo || y0 >= hi {
        return;
    }
    let cut0 = 0.max(lo - y0);
    let cut1 = i64::from(src.height()) - 0.max(y1 - hi);
    composite_rows(dst, src, px(x * scale), y0.max(lo), cut0, cut1, 1.0);
}

/// `settings_window.composite_over` with a fade factor.
pub fn composite_over_alpha(dst: &mut Layer, src: &Layer, x: i64, y: i64, alpha: f32) {
    composite_rows(dst, src, x, y, 0, i64::from(src.height()), alpha);
}

fn composite_rows(dst: &mut Layer, src: &Layer, x: i64, y: i64, sy0: i64, sy1: i64, alpha: f32) {
    let dst_w = i64::from(dst.width());
    let dst_h = i64::from(dst.height());
    let src_w = i64::from(src.width());
    let src_h = i64::from(src.height());
    let sy0 = sy0.clamp(0, src_h);
    let sy1 = sy1.clamp(sy0, src_h);
    let dx0 = x.max(0);
    let dy0 = y.max(0);
    if dx0 >= dst_w || dy0 >= dst_h || sy0 >= sy1 {
        return;
    }
    let sx0 = dx0 - x;
    let width = (src_w - sx0).min(dst_w - dx0);
    let height = (sy1 - sy0).min(dst_h - dy0);
    if width <= 0 || height <= 0 {
        return;
    }
    let dst_w = dst_w as usize;
    let src_w = src_w as usize;
    for row in 0..height as usize {
        let src_row = (sy0 as usize + row) * src_w;
        let dst_row = (dy0 as usize + row) * dst_w;
        for column in 0..width as usize {
            let source = (src_row + sx0 as usize + column) * 4;
            let target = (dst_row + dx0 as usize + column) * 4;
            blend_pixel(dst.as_raw_mut(), src.as_raw(), source, target, alpha);
        }
    }
}

/// Composites one premultiplied pixel with a fade factor.
fn blend_pixel(dst: &mut [f32], src: &[f32], source: usize, target: usize, alpha: f32) {
    let top = &src[source..source + 4];
    let a = top[3] * alpha;
    let one = 1.0 - a;
    let pixel = &mut dst[target..target + 4];
    for channel in 0..3 {
        pixel[channel] = top[channel] * alpha + pixel[channel] * one;
    }
    pixel[3] = a + pixel[3] * one;
}

/// Fills a horizontal span of rows with a premultiplied colour (`buf[rows, cols] = …`).
pub fn fill_span(
    layer: &mut Layer,
    rows: (f64, f64),
    columns: (f64, f64),
    color: Rgba,
    scale: f64,
) {
    let y0 = px(rows.0 * scale).clamp(0, i64::from(layer.height()));
    let y1 = px(rows.1 * scale).clamp(y0, i64::from(layer.height()));
    let x0 = px(columns.0 * scale).clamp(0, i64::from(layer.width()));
    let x1 = px(columns.1 * scale).clamp(x0, i64::from(layer.width()));
    if y0 >= y1 || x0 >= x1 {
        return;
    }
    let alpha = f32::from(color.a) / 255.0;
    let premultiplied = [
        f32::from(color.r) / 255.0 * alpha,
        f32::from(color.g) / 255.0 * alpha,
        f32::from(color.b) / 255.0 * alpha,
        alpha,
    ];
    let width = layer.width() as usize;
    for row in y0 as usize..y1 as usize {
        let start = (row * width + x0 as usize) * 4;
        let end = (row * width + x1 as usize) * 4;
        for pixel in layer.as_raw_mut()[start..end].as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&premultiplied);
        }
    }
}
