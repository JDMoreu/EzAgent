//! Bitmap rasterization and the ink-crop cache shape of `text.py`.

use crate::Mask;

use super::font::{self, pixel, Font, RasterGlyph};
use super::{FontStyle, TextBitmap};

/// `_render_bitmap`: draws into an `L` canvas and crops to the ink box.
pub(super) fn render_bitmap(text: &str, style: FontStyle, scale: f64, tracking: f64) -> TextBitmap {
    let font = font::load_font(style, scale);
    let tracking_px = tracking * style.size * scale;
    let (ascent, descent) = font.metrics();
    let line_h = ((ascent + descent) as f64).round_ties_even() as i32;
    let line_h = line_h.max(1);
    let pad = ((2.0 * scale).round_ties_even() as i32).max(2);

    if text.is_empty() {
        return TextBitmap {
            alpha: Mask::new(0, 0, 0),
            width_px: 0,
            line_h_px: line_h as u32,
        };
    }

    let (total, canvas) = if tracking_px.abs() > 1e-6 {
        render_tracked(&font, text, tracking_px, line_h, pad)
    } else {
        render_run(&font, text, line_h, pad)
    };
    let Some((min_x, min_y, max_x, max_y)) = ink_bounds(&canvas) else {
        return TextBitmap {
            alpha: Mask::new(0, 0, 0),
            width_px: round_advance(total, 0),
            line_h_px: line_h as u32,
        };
    };

    let canvas_w = canvas.width();
    let raw = canvas.as_raw();
    let crop_w = max_x - min_x + 1;
    let crop_h = max_y - min_y + 1;
    let mut pixels = Vec::with_capacity((crop_w * crop_h) as usize);
    for y in min_y..=max_y {
        let start = (y * canvas_w + min_x) as usize;
        pixels.extend_from_slice(&raw[start..start + crop_w as usize]);
    }
    let alpha = Mask::from_raw(crop_w, crop_h, pixels).expect("the crop matches its bounds");
    TextBitmap {
        alpha,
        width_px: round_advance(total, 1),
        line_h_px: line_h as u32,
    }
}

fn ink_bounds(canvas: &Mask) -> Option<(u32, u32, u32, u32)> {
    let width = canvas.width();
    let raw = canvas.as_raw();
    let mut min_x = width;
    let mut min_y = canvas.height();
    let mut max_x = 0;
    let mut max_y = 0;
    let mut any = false;
    for y in 0..canvas.height() {
        let row = &raw[(y * width) as usize..((y + 1) * width) as usize];
        for (x, &value) in row.iter().enumerate() {
            if value != 0 {
                any = true;
                min_x = min_x.min(x as u32);
                max_x = max_x.max(x as u32);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

/// Pillow's tracking branch: one `draw.text` per character at a float pen.
fn render_tracked(font: &Font, text: &str, tracking_px: f64, line_h: i32, pad: i32) -> (f64, Mask) {
    let chars: Vec<char> = text.chars().collect();
    let advances: Vec<f64> = chars
        .iter()
        .map(|ch| font.length(&ch.to_string()))
        .collect();
    let total = advances.iter().sum::<f64>() + tracking_px * (chars.len() as f64 - 1.0);
    let canvas_w = (total.ceil() as i64 + 2 * i64::from(pad)).max(1) as u32;
    let canvas_h = (line_h + 2 * pad) as u32;
    let mut canvas = Mask::new(canvas_w, canvas_h, 0);
    let baseline = pad + font.metrics().0;
    let mut pen = f64::from(pad);
    for (ch, advance) in chars.iter().zip(advances) {
        draw_string(&mut canvas, font, &ch.to_string(), pen, baseline);
        pen += advance + tracking_px;
    }
    (total, canvas)
}

/// Pillow's normal branch: the whole string laid out with kerning.
fn render_run(font: &Font, text: &str, line_h: i32, pad: i32) -> (f64, Mask) {
    let glyphs = font.layout(text);
    let total = glyphs.iter().map(|glyph| glyph.advance).sum::<i64>() as f64 / 64.0;
    let canvas_w = (total.ceil() as i64 + 2 * i64::from(pad)).max(1) as u32;
    let canvas_h = (line_h + 2 * pad) as u32;
    let mut canvas = Mask::new(canvas_w, canvas_h, 0);
    let baseline = pad + font.metrics().0;
    let mut pen = 0i64;
    for glyph in glyphs {
        let x = i64::from(pad) + pixel(pen);
        draw_glyph(&mut canvas, font, glyph.index, x, baseline);
        pen += glyph.advance;
    }
    (total, canvas)
}

fn draw_string(canvas: &mut Mask, font: &Font, text: &str, pen: f64, baseline: i32) {
    let whole = pen.trunc() as i64;
    let fraction = pen - pen.trunc();
    let mut position = (whole << 6) + (fraction * 64.0).round() as i64;
    for glyph in font.layout(text) {
        draw_glyph(canvas, font, glyph.index, pixel(position), baseline);
        position += glyph.advance;
    }
}

fn draw_glyph(canvas: &mut Mask, font: &Font, index: u32, x: i64, baseline: i32) {
    let Some(glyph) = font.raster(index) else {
        return;
    };
    let left = x + i64::from(glyph.left);
    let top = i64::from(baseline) - i64::from(glyph.top);
    blit(canvas, &glyph, left, top);
}

/// Pillow's `L` blend: `src + MULDIV255(dst, 255 - src)` when dst is inked.
fn blit(canvas: &mut Mask, glyph: &RasterGlyph, left: i64, top: i64) {
    let width = i64::from(canvas.width());
    let height = i64::from(canvas.height());
    let x0 = left.max(0);
    let y0 = top.max(0);
    let x1 = (left + i64::from(glyph.width)).min(width);
    let y1 = (top + i64::from(glyph.height)).min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for y in y0..y1 {
        let source_y = (y - top) as usize;
        for x in x0..x1 {
            let source_x = (x - left) as usize;
            let source = glyph.pixels[source_y * glyph.width as usize + source_x];
            if source == 0 {
                continue;
            }
            let target = canvas.get(x as u32, y as u32);
            let value = if target > 0 {
                clip8(u32::from(source) + muldiv255(u32::from(target), 255 - u32::from(source)))
            } else {
                source
            };
            canvas.set(x as u32, y as u32, value);
        }
    }
}

fn muldiv255(left: u32, right: u32) -> u32 {
    let tmp = left * right + 128;
    (tmp + (tmp >> 8)) >> 8
}

fn clip8(value: u32) -> u8 {
    value.min(255) as u8
}

/// `max(minimum, int(round(total)))`; ties round to even like Python.
fn round_advance(total: f64, minimum: u32) -> u32 {
    (total.round_ties_even() as i64).max(i64::from(minimum)) as u32
}
