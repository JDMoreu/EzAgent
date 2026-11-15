//! Drawing primitives and styled text of the chat, from
//! `src/ezagent/voice/chat/render_base.py` (F5-13).

use ez_gfx::draw::Draw;
use ez_gfx::resample::Resampling;
use ez_gfx::text::chat::{pieces, wrap, SpanInput, LINE_K};
use ez_gfx::text::{self, Align, TextOptions};
use ez_gfx::{Image, Mask, Rgba};

use super::{style, Ctx, Run, Span};

/// Finishes the spans of one block into laid-out lines.
pub fn wrap_spans(spans: &[Span], base: &str, width: f64, scale: f64) -> Vec<Vec<Run>> {
    let inputs: Vec<SpanInput<'_>> = spans
        .iter()
        .map(|span| SpanInput {
            text: &span.text,
            bold: span.bold,
            italic: span.italic,
            code: span.code,
            link: span.link.is_some(),
        })
        .collect();
    wrap(&pieces(&inputs, base), width, scale)
}

// ---------------------------------------------------------------- shapes
/// Rounded rectangle with soft edges (mask supersampled ×4).
pub fn round_rect(
    img: &mut Image,
    ctx: &Ctx<'_>,
    box_: (f64, f64, f64, f64),
    radius: f64,
    fill: Option<Rgba>,
    outline: Option<Rgba>,
) {
    let (x0, y0, x1, y1) = (
        ctx.px(box_.0),
        ctx.px(box_.1),
        ctx.px(box_.2),
        ctx.px(box_.3),
    );
    let width = (x1 - x0).max(1) as u32;
    let height = (y1 - y0).max(1) as u32;
    let supersample = 4u32;
    let corner = ctx.px(radius).max(0) as u32 * supersample;
    let ring = ctx.px(1.0).max(1) as u32 * supersample;
    let shape = |inset: u32| -> Mask {
        let mut mask = Mask::new(width * supersample, height * supersample, 0);
        Draw::new(&mut mask)
            .rounded_rectangle(
                [
                    f64::from(inset),
                    f64::from(inset),
                    f64::from(width * supersample - 1 - inset),
                    f64::from(height * supersample - 1 - inset),
                ],
                f64::from(corner.saturating_sub(inset)),
                Some(255u8),
                None,
                0,
            )
            .expect("the rounded box is ordered");
        mask.resize(width, height, Resampling::Lanczos)
    };
    let outer = shape(0);
    let inner = if outline.is_some() {
        shape(ring)
    } else {
        outer.clone()
    };
    if let Some(color) = fill {
        composite_mask(img, &multiply_mask(&inner, color.a), color, (x0, y0));
    }
    if let Some(color) = outline {
        let ring_mask = subtract_masks(&outer, &inner);
        composite_mask(img, &multiply_mask(&ring_mask, color.a), color, (x0, y0));
    }
}

/// Axis-aligned 1 px line (`ImageDraw.rectangle`).
pub fn line(img: &mut Image, ctx: &Ctx<'_>, x0: f64, y0: f64, x1: f64, y1: f64, color: Rgba) {
    let left = ctx.px(x0);
    let top = ctx.px(y0);
    let right = ctx.px(x0).max(ctx.px(x1) - 1);
    let bottom = ctx.px(y0).max(ctx.px(y1) - 1);
    Draw::new(img)
        .rectangle(
            [
                f64::from(left),
                f64::from(top),
                f64::from(right),
                f64::from(bottom),
            ],
            Some(color),
            None,
            0,
        )
        .expect("the line box is ordered");
}

/// Vector icon centred at the logical point `(cx, cy)`.
pub fn draw_icon_at(
    img: &mut Image,
    ctx: &Ctx<'_>,
    name: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: Rgba,
) {
    let side = (ctx.px(size) + 4).max(4) as u32;
    let mut tile = vec![0f32; side as usize * side as usize * 4];
    ctx.icons.draw(
        &mut tile,
        side,
        name,
        f64::from(side) / 2.0 / ctx.scale,
        f64::from(side) / 2.0 / ctx.scale,
        size,
        color,
        ctx.scale,
    );
    let mut pixels = Vec::with_capacity(tile.len());
    for pixel in tile.as_chunks::<4>().0 {
        let alpha = pixel[3];
        for channel in &pixel[..3] {
            let value = if alpha > 1e-6 {
                (channel / alpha).clamp(0.0, 1.0)
            } else {
                0.0
            };
            pixels.push((value * 255.0 + 0.5) as u8);
        }
        pixels.push((alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }
    let tile = Image::from_rgba(side, side, pixels).expect("the icon tile has its buffer");
    img.alpha_composite(
        &tile,
        (ctx.px(cx) - side as i32 / 2, ctx.px(cy) - side as i32 / 2),
    );
}

/// Text with its baseline at `baseline`; italics are slanted.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_at(
    img: &mut Image,
    ctx: &Ctx<'_>,
    x: f64,
    baseline: f64,
    text: &str,
    key: &str,
    color: Rgba,
    italic: bool,
) {
    if text.is_empty() {
        return;
    }
    let leading = text.len() - text.trim_start_matches(is_py_space).len();
    let (prefix, body) = text.split_at(leading);
    if body.is_empty() {
        return;
    }
    let shift = if prefix.is_empty() {
        0.0
    } else {
        ez_gfx::text::chat::length(prefix, key, ctx.scale)
    };
    let style = style(key);
    let (logical_w, logical_h) =
        text::text_size(body, style, ctx.scale, None, "left").expect("the chat style is known");
    let pad = ((2.0 * ctx.scale).round_ties_even() as i64).max(1) as u32;
    let width = ((logical_w * ctx.scale).ceil().max(0.0) as u32).max(1) + 2 * pad;
    let height = ((logical_h * ctx.scale).ceil().max(0.0) as u32).max(1) + 2 * pad;
    let mut values = vec![0f32; width as usize * height as usize * 4];
    {
        let mut surface = text::FloatSurface::new(&mut values, width as usize, height as usize);
        let options = TextOptions {
            scale: ctx.scale,
            align: Align::Left,
            tracking: None,
            alpha: 1.0,
        };
        let position = (f64::from(pad) / ctx.scale, f64::from(pad) / ctx.scale);
        text::draw_text(
            &mut surface,
            body,
            style,
            // DECISION: el blanco solo obtiene la cobertura; el color lo pone
            // el mezclado de abajo, como el `draw_bitmap` de Pillow.
            text::Color::Rgb([255, 255, 255]),
            position,
            options,
        )
        .expect("the text tile is valid");
    }
    let coverage = values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| (pixel[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
        .collect();
    let mask = Mask::from_raw(width, height, coverage).expect("the coverage tile has its buffer");
    let dx = ctx.px(x + shift) - pad as i32 + ink_bearing(body, style, key, ctx.scale);
    // Python rounds the baseline and then offsets the mask by a whole number of
    // physical pixels; rounding the sum instead moves the text a pixel.
    let ink_offset = ink_top(body, style, ctx.scale);
    let dy = ctx.px(baseline) + ink_offset - pad as i32;
    if !italic {
        blend_mask(img, &mask, (dx, dy), color);
        return;
    }
    let baseline_row = f64::from(pad) - f64::from(ink_offset);
    let extra = (0.2 * (f64::from(height) + baseline_row.max(0.0))).ceil() as u32 + 1;
    let sheared = shear_rows(&mask, baseline_row, extra);
    let tinted = multiply_mask(&sheared, color.a);
    composite_mask(img, &tinted, color, (dx - extra as i32, dy));
}

/// Distance from the baseline to the first row of ink, in physical pixels.
///
/// `baseline_top` returns the outline box, which can sit one row above the
/// raster (`*`, accents); the mask Pillow pastes carries that empty row, so the
/// crop has to move down to compensate. `Á` is taller than any chat glyph and
/// has no descender, so comparing heights against `Á` + text gives the ink's
/// top row outright.
pub(super) fn ink_top(text: &str, style: text::FontStyle, scale: f64) -> i32 {
    let outline = text::baseline_top(text, style, 0.0, scale)
        .map(|top| (top * scale).round_ties_even() as i32);
    let probe = format!("Á{text}");
    let with = text::text_size(&probe, style, scale, None, "left").map(|(_, height)| height);
    let alone = text::text_size(text, style, scale, None, "left").map(|(_, height)| height);
    let probe_top = text::baseline_top("Á", style, 0.0, scale)
        .map(|top| (top * scale).round_ties_even() as i32)
        .unwrap_or(0);
    let (Ok(outline), Ok(with), Ok(alone)) = (outline, with, alone) else {
        return 0;
    };
    let measured = probe_top + ((with - alone) * scale).round_ties_even() as i32;
    // The probe only dominates the top row when the run does not reach above it
    // or stop above the baseline; anything further off falls back to the box.
    if outline <= measured && measured <= outline + 1 {
        measured
    } else {
        outline
    }
}

/// Horizontal offset between the pen and the left edge of the ink.
///
/// Pillow pastes the text mask at the pen, so the ink lands at `pen + bearing`;
/// `ez_gfx::text::draw_text` anchors the cropped ink instead and drops it.
/// `ink("T" + text) - ink(text) - advance("T")` recovers the bearing: `T` is
/// the leftmost ink and its own bearing is zero in every style the chat uses
/// (checked against Pillow for the eleven styles at 100, 125, 150 and 200 %).
pub(super) fn ink_bearing(text: &str, style: text::FontStyle, key: &str, scale: f64) -> i32 {
    let probe = format!("T{text}");
    let with = text::text_size(&probe, style, scale, None, "left").map(|(width, _)| width * scale);
    let alone = text::text_size(text, style, scale, None, "left").map(|(width, _)| width * scale);
    let (Ok(with), Ok(alone)) = (with, alone) else {
        return 0;
    };
    let advance = ez_gfx::text::chat::length("T", key, scale) * scale;
    let bearing = (with - alone - advance).round_ties_even() as i32;
    // The formula needs the run's ink to reach past the probe's; a run whose
    // ink is narrower than the gap falls back to no offset.
    if (-3..=3).contains(&bearing) {
        bearing
    } else {
        0
    }
}

/// `ImageDraw.text` on RGBA: Pillow's `draw_bitmap` blend (`Draw.c`).
/// With a transparent destination it copies the ink colour and only scales the
/// coverage; over anything else it blends every channel, alpha included, in
/// premultiplied arithmetic.
pub(super) fn blend_mask(img: &mut Image, mask: &Mask, dest: (i32, i32), color: Rgba) {
    let (width, height) = (img.width() as i32, img.height() as i32);
    let x0 = dest.0.max(0);
    let y0 = dest.1.max(0);
    let x1 = (dest.0 + mask.width() as i32).min(width);
    let y1 = (dest.1 + mask.height() as i32).min(height);
    let ink = color.to_array();
    for y in y0..y1 {
        for x in x0..x1 {
            let coverage = u32::from(mask.get((x - dest.0) as u32, (y - dest.1) as u32));
            if coverage == 0 {
                continue;
            }
            let target = img.pixel(x as u32, y as u32);
            let mut out = target.to_array();
            if target.a == 0 {
                out = [
                    ink[0],
                    ink[1],
                    ink[2],
                    blend_channel(coverage, u32::from(ink[3]), 0),
                ];
            } else {
                for (channel, value) in out.iter_mut().enumerate() {
                    *value = blend_channel(coverage, u32::from(ink[channel]), u32::from(*value));
                }
            }
            img.set_pixel(x as u32, y as u32, Rgba::from_array(out));
        }
    }
}

/// `(src * mask + dst * (255 - mask) + 127) / 255`, Pillow's text blend.
fn blend_channel(mask: u32, source: u32, target: u32) -> u8 {
    ((source * mask + target * (255 - mask) + 127) / 255) as u8
}

/// `_Piece` code padding and line drawing live in `ez-gfx::text::chat`; this
/// paints the wrapped lines from `(x0, y0)` (top-left corner).
pub fn paint_lines(
    img: &mut Image,
    ctx: &Ctx<'_>,
    lines: &[Vec<Run>],
    base: &str,
    x0: f64,
    y0: f64,
    color: Rgba,
) {
    let step = ctx.size(base) * LINE_K;
    for (index, runs) in lines.iter().enumerate() {
        let baseline = y0 + index as f64 * step + step * 0.5 + ctx.size(base) * 0.34;
        for run in runs {
            if run.piece.space {
                continue;
            }
            let mut tone = color;
            let mut x = x0 + run.x;
            if run.piece.code {
                let size = ctx.size("code");
                round_rect(
                    img,
                    ctx,
                    (
                        x,
                        baseline - size * 1.02,
                        x + run.width,
                        baseline + size * 0.34,
                    ),
                    4.0,
                    Some(ctx.color("TRACK", Some(0.55))),
                    None,
                );
                x += 3.0;
                tone = ctx.color("TEXT", None);
            } else if run.piece.link {
                tone = ctx.color("MINT", None);
            }
            draw_text_at(
                img,
                ctx,
                x,
                baseline,
                &run.text,
                run.piece.key,
                tone,
                run.piece.italic,
            );
            if run.piece.link {
                line(
                    img,
                    ctx,
                    x,
                    baseline + 2.0,
                    x + run.width,
                    baseline + 3.0,
                    tone,
                );
            }
        }
    }
}

fn composite_mask(img: &mut Image, mask: &Mask, color: Rgba, dest: (i32, i32)) {
    let mut layer = Image::new(
        mask.width(),
        mask.height(),
        Rgba::opaque(color.r, color.g, color.b),
    );
    put_alpha(&mut layer, mask);
    img.alpha_composite(&layer, dest);
}

pub(crate) fn put_alpha(img: &mut Image, mask: &Mask) {
    for (pixel, &alpha) in img
        .as_raw_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.as_raw())
    {
        pixel[3] = alpha;
    }
}

/// `ImageChops.multiply` of two masks.
pub(crate) fn multiply_masks(left: &Mask, right: &Mask) -> Mask {
    let data = left
        .as_raw()
        .iter()
        .zip(right.as_raw())
        .map(|(&a, &b)| (u16::from(a) * u16::from(b) / 255) as u8)
        .collect();
    Mask::from_raw(left.width(), left.height(), data).expect("the masks share dimensions")
}

/// `ImageChops.multiply(mask, constant)`: integer `(a * b) / 255`.
pub(crate) fn multiply_mask(mask: &Mask, alpha: u8) -> Mask {
    let mut out = mask.clone();
    for value in out.as_raw_mut() {
        *value = (u16::from(*value) * u16::from(alpha) / 255) as u8;
    }
    out
}

/// `ImageChops.subtract` for masks.
pub(crate) fn subtract_masks(left: &Mask, right: &Mask) -> Mask {
    let data = left
        .as_raw()
        .iter()
        .zip(right.as_raw())
        .map(|(&a, &b)| a.saturating_sub(b))
        .collect();
    Mask::from_raw(left.width(), left.height(), data).expect("the masks share dimensions")
}

/// Copies a region, clipped to the image (Pillow pads with transparent; the
/// call sites always stay inside).
pub(crate) fn crop(img: &Image, x0: u32, y0: u32, x1: u32, y1: u32) -> Image {
    let x1 = x1.min(img.width());
    let y1 = y1.min(img.height());
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    if width == 0 || height == 0 {
        return Image::new(1, 1, Rgba::TRANSPARENT);
    }
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    for y in y0..y1 {
        let start = (y * img.width() + x0) as usize * 4;
        data.extend_from_slice(&img.as_raw()[start..start + width as usize * 4]);
    }
    Image::from_rgba(width, height, data).expect("the crop has its buffer")
}

/// Applies Pillow's affine shear `(1, 0.2, -0.2 * ascent, 0, 1, 0)`, row by
/// row around the baseline.
///
/// Pillow samples the source at the destination pixel centre (`y + 0.5`) and
/// convolves with the cubic kernel below, keeping the taps that fall inside the
/// row and truncating the accumulated coverage.
pub(super) fn shear_rows(mask: &Mask, baseline_row: f64, extra: u32) -> Mask {
    let width = mask.width();
    let height = mask.height();
    let out_w = width + 2 * extra;
    let mut out = vec![0u8; out_w as usize * height as usize];
    for y in 0..height {
        let shift = 0.2 * (f64::from(y) + 0.5 - baseline_row);
        let out_row = y as usize * out_w as usize;
        let row = mask.row(y);
        for x in 0..out_w {
            let source_x = f64::from(x) - f64::from(extra) + shift;
            out[out_row + x as usize] = sample_row(row, source_x);
        }
    }
    Mask::from_raw(out_w, height, out).expect("the sheared tile has its buffer")
}

fn is_py_space(ch: char) -> bool {
    ch.is_whitespace() || (''..='').contains(&ch)
}

/// Pillow's cubic kernel for affine transforms (`a = -1`): it is not
/// renormalised, so a half-pixel sample keeps `1.25` of the 255.
pub(super) fn cubic_kernel(t: f64) -> f64 {
    let t = t.abs();
    if t < 1.0 {
        t * t * t - 2.0 * t * t + 1.0
    } else if t < 2.0 {
        -t * t * t + 5.0 * t * t - 8.0 * t + 4.0
    } else {
        0.0
    }
}

/// One row of Pillow's affine resample: taps within two pixels, truncated.
fn sample_row(row: &[u8], x: f64) -> u8 {
    let first = x.floor() as i64 - 1;
    let mut total = 0.0;
    for index in first..first + 4 {
        let weight = cubic_kernel(x - index as f64);
        if weight != 0.0 && (0..row.len() as i64).contains(&index) {
            total += f64::from(row[index as usize]) * weight;
        }
    }
    total.clamp(0.0, 255.0) as u8
}
