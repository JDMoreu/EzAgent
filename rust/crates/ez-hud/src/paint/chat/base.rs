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
    let top = text::baseline_top(body, style, baseline, ctx.scale)
        .expect("the chat style is known and the font is bundled");
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
            text::Color::Rgba(color.to_array()),
            position,
            options,
        )
        .expect("the text tile is valid");
    }
    let mut dx = ctx.px(x + shift) - pad as i32;
    let dy = ctx.px(top) - pad as i32;
    let mut output_w = width;
    if italic {
        let baseline_row = f64::from(pad) + f64::from(ctx.px(baseline) - ctx.px(top));
        let extra = (0.2 * f64::from(height)).ceil() as u32 + 1;
        values = shear_rows(&values, width, height, baseline_row, extra);
        output_w = width + 2 * extra;
        dx -= extra as i32;
    }
    let tile = Image::from_f32_premultiplied_rgba(output_w, height, &values)
        .expect("the text tile has its buffer");
    img.alpha_composite(&tile, (dx, dy));
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
// DECISION: horizontal resampling is bilinear; Pillow uses BICUBIC, a
// sub-pixel difference on italics until parity tests land.
fn shear_rows(values: &[f32], width: u32, height: u32, baseline_row: f64, extra: u32) -> Vec<f32> {
    let out_w = width + 2 * extra;
    let mut out = vec![0f32; out_w as usize * height as usize * 4];
    for y in 0..height {
        let shift = 0.2 * (baseline_row - f64::from(y));
        let out_row = y as usize * out_w as usize * 4;
        for x in 0..out_w {
            let source_x = f64::from(x) - f64::from(extra) - shift;
            let index = (y * width) as usize;
            let alpha = sample_row(values, index, width, source_x, 3);
            let out_index = out_row + x as usize * 4;
            for channel in 0..3 {
                out[out_index + channel] = sample_row(values, index, width, source_x, channel);
            }
            out[out_index + 3] = alpha;
        }
    }
    out
}

fn is_py_space(ch: char) -> bool {
    ch.is_whitespace() || (''..='').contains(&ch)
}

fn sample_row(values: &[f32], row: usize, width: u32, x: f64, channel: usize) -> f32 {
    if x < 0.0 || x >= f64::from(width) {
        return 0.0;
    }
    let left = x.floor() as u32;
    let fraction = (x - f64::from(left)) as f32;
    let right = (left + 1).min(width - 1);
    let sample = |index: u32| values[(row + index as usize) * 4 + channel];
    sample(left) * (1.0 - fraction) + sample(right) * fraction
}
