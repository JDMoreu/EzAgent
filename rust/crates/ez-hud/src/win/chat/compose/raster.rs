//! Pixel arithmetic of the chat panel (`chat_paint.py`, F5-15).
//!
//! The Python composition works on a premultiplied `float32` RGBA canvas and
//! only converts to eight-bit BGRA at the very end. The helpers here keep that
//! order: composite, shade, blend and, at the end, quantize. Every pass walks
//! the canvas by contiguous rows (numpy did the same under `_composite`,
//! `_shade` and the `[..., None]` broadcasts) and allocates no temporaries.

use ndarray::{Array2, Array3, ArrayView3};

use crate::win::layered::RgbaFrame;

/// A finished panel frame: BGRA premultiplied, like the array `render` returned
/// from Python.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

impl Frame {
    /// Transparent frame of `width`×`height` pixels.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            bgra: vec![0; width.max(1) * height.max(1) * 4],
        }
    }

    /// One pixel as `[B, G, R, A]`.
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let index = (y * self.width + x) * 4;
        [
            self.bgra[index],
            self.bgra[index + 1],
            self.bgra[index + 2],
            self.bgra[index + 3],
        ]
    }

    /// Straight RGBA for the layered window (`RGBa` → `RGBA`).
    ///
    /// DECISION: rounds to the nearest straight value instead of Pillow's
    /// truncation. The window premultiplies again with `(c·a + 127) / 255`;
    /// rounding makes that integer step recover the exact BGRA bytes, while
    /// truncation shifted some colours down by one.
    pub fn to_rgba_frame(&self) -> RgbaFrame {
        let table = straight_table();
        let mut data = Vec::with_capacity(self.bgra.len());
        for pixel in self.bgra.as_chunks::<4>().0 {
            let row = usize::from(pixel[3]) * 256;
            data.extend_from_slice(&[
                table[row + usize::from(pixel[2])],
                table[row + usize::from(pixel[1])],
                table[row + usize::from(pixel[0])],
                pixel[3],
            ]);
        }
        RgbaFrame::new(self.width as u32, self.height as u32, data)
            .expect("a Frame always has its exact byte length")
    }
}

/// `round_ties_even(channel · 255 / alpha)`, capped at 255, indexed by
/// `alpha · 256 + channel`.
fn straight_table() -> &'static [u8] {
    static TABLE: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        let mut table = vec![0u8; 256 * 256];
        for alpha in 1..256usize {
            for channel in 0..256usize {
                let value = (channel as f64 * 255.0 / alpha as f64)
                    .round_ties_even()
                    .min(255.0) as u8;
                table[alpha * 256 + channel] = value;
            }
        }
        table
    });
    &TABLE
}

/// Quantizes the premultiplied float canvas to BGRA.
pub(super) fn frame_from_canvas(canvas: &Array3<f32>) -> Frame {
    let (height, width, _) = canvas.dim();
    let mut bgra = vec![0u8; width * height * 4];
    for (out, pixel) in bgra
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(points(canvas).as_chunks::<4>().0)
    {
        out[0] = quantize(pixel[2]);
        out[1] = quantize(pixel[1]);
        out[2] = quantize(pixel[0]);
        out[3] = quantize(pixel[3]);
    }
    Frame {
        width,
        height,
        bgra,
    }
}

fn quantize(value: f32) -> u8 {
    // `as u8` saturates, so the clamp of the Python is already applied.
    (value * 255.0 + 0.5) as u8
}

/// Contiguous pixels of a layer as one flat slice.
pub(super) fn points(layer: &Array3<f32>) -> &[f32] {
    layer.as_slice().expect("layers are contiguous")
}

/// Mutable pixels of the canvas as one flat slice.
pub(super) fn points_mut(canvas: &mut Array3<f32>) -> &mut [f32] {
    canvas.as_slice_mut().expect("the canvas is contiguous")
}

/// Canvas width in pixels.
pub(super) fn canvas_width(canvas: &Array3<f32>) -> usize {
    canvas.dim().1
}

/// Intersection of a source rectangle with the canvas, in both frames.
pub(super) struct Span {
    pub src_row: usize,
    pub src_column: usize,
    pub dst_row: usize,
    pub dst_column: usize,
    pub columns: usize,
    pub rows: usize,
}

/// Clips `[x_from, x_to)`×`[y_from, y_to)` placed at `(x0, y0)` to the canvas.
pub(super) fn span(
    canvas: &Array3<f32>,
    x0: i32,
    y0: i32,
    x_from: usize,
    x_to: usize,
    y_from: usize,
    y_to: usize,
) -> Option<Span> {
    let (canvas_h, canvas_w, _) = canvas.dim();
    let left = x0 + x_from as i32;
    let top = y0 + y_from as i32;
    let right = x0 + x_to as i32;
    let bottom = y0 + y_to as i32;
    let dx0 = left.max(0);
    let dy0 = top.max(0);
    let dx1 = right.min(canvas_w as i32);
    let dy1 = bottom.min(canvas_h as i32);
    if dx0 >= dx1 || dy0 >= dy1 {
        return None;
    }
    Some(Span {
        src_row: (y_from as i32 + (dy0 - top)) as usize,
        src_column: (x_from as i32 + (dx0 - left)) as usize,
        dst_row: dy0 as usize,
        dst_column: dx0 as usize,
        columns: (dx1 - dx0) as usize,
        rows: (dy1 - dy0) as usize,
    })
}

/// `_composite`: premultiplied `src` over the canvas.
pub(super) fn composite(canvas: &mut Array3<f32>, src: &Array3<f32>, x0: i32, y0: i32) {
    let (height, width, _) = src.dim();
    let Some(clip) = span(canvas, x0, y0, 0, width, 0, height) else {
        return;
    };
    let canvas_w = canvas_width(canvas);
    let source = points(src);
    let target = points_mut(canvas);
    for row in 0..clip.rows {
        let source_row = clip.src_row + row;
        let source_column = clip.src_column;
        let pixels = &source[(source_row * width + source_column) * 4..][..clip.columns * 4];
        let start = ((clip.dst_row + row) * canvas_w + clip.dst_column) * 4;
        let under = &mut target[start..start + clip.columns * 4];
        for (bottom, top) in under
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels.as_chunks::<4>().0)
        {
            let alpha = top[3];
            for channel in 0..4 {
                bottom[channel] = bottom[channel] * (1.0 - alpha) + top[channel];
            }
        }
    }
}

/// `_shade`: blackens the canvas under `alpha` (times `gain`) and adds its
/// coverage to alpha.
#[allow(clippy::too_many_arguments)]
pub(super) fn shade_rows(
    canvas: &mut Array3<f32>,
    alpha: &Array2<f32>,
    x_from: usize,
    x_to: usize,
    y_from: usize,
    y_to: usize,
    x0: i32,
    y0: i32,
    gain: f32,
) {
    let (_, width) = alpha.dim();
    let Some(clip) = span(canvas, x0, y0, x_from, x_to, y_from, y_to) else {
        return;
    };
    let canvas_w = canvas_width(canvas);
    let source = alpha.as_slice().expect("coverage is contiguous");
    let target = points_mut(canvas);
    for row in 0..clip.rows {
        let source_row = clip.src_row + row;
        let source_column = clip.src_column;
        let values = &source[source_row * width + source_column..][..clip.columns];
        let start = ((clip.dst_row + row) * canvas_w + clip.dst_column) * 4;
        let under = &mut target[start..start + clip.columns * 4];
        for (bottom, &value) in under.as_chunks_mut::<4>().0.iter_mut().zip(values) {
            let cover = value * gain;
            let keep = 1.0 - cover;
            bottom[0] *= keep;
            bottom[1] *= keep;
            bottom[2] *= keep;
            bottom[3] = cover + bottom[3] * keep;
        }
    }
}

/// `_shade_frame`: only the `band`-pixel border of `alpha` darkens the canvas.
pub(super) fn shade_frame(
    canvas: &mut Array3<f32>,
    alpha: &Array2<f32>,
    x0: i32,
    y0: i32,
    band: i32,
    gain: f32,
) {
    let (height, width) = alpha.dim();
    let band = band.max(1).min(height as i32).min(width as i32) as usize;
    if band * 2 >= height || band * 2 >= width {
        shade_rows(canvas, alpha, 0, width, 0, height, x0, y0, gain);
        return;
    }
    for rows in [0..band, height - band..height] {
        shade_rows(canvas, alpha, 0, width, rows.start, rows.end, x0, y0, gain);
    }
    let middle = band..height - band;
    for columns in [0..band, width - band..width] {
        shade_rows(
            canvas,
            alpha,
            columns.start,
            columns.end,
            middle.start,
            middle.end,
            x0,
            y0,
            gain,
        );
    }
}

/// `_paint_content` without the temporary: content × coverage × row mask.
///
/// `skip` is the rectangle of the source that is painted apart (the caret);
/// those pixels are composited by [`composite_caret`].
#[allow(clippy::too_many_arguments)]
pub(super) fn composite_masked(
    canvas: &mut Array3<f32>,
    src: &[f32],
    src_width: usize,
    coverage: &Array2<f32>,
    mask: &[f32],
    columns: usize,
    rows: usize,
    x0: i32,
    y0: i32,
    skip: Option<Skip>,
) {
    let (coverage_height, coverage_width) = coverage.dim();
    let Some(clip) = span(canvas, x0, y0, 0, columns, 0, rows) else {
        return;
    };
    let canvas_w = canvas_width(canvas);
    let source = src;
    let cover = coverage.as_slice().expect("coverage is contiguous");
    let target = points_mut(canvas);
    for row in 0..clip.rows {
        let source_row = clip.src_row + row;
        let source_column = clip.src_column;
        debug_assert!(source_row < mask.len() && source_row < coverage_height);
        let opacity = mask[source_row];
        let last = source_column + clip.columns;
        let (lo, hi) = match skip {
            Some(skip) if skip.contains_row(source_row) => {
                (skip.x0.max(source_column), skip.x1.min(last))
            }
            _ => (last, last),
        };
        if source_column < lo {
            composite_range(
                target,
                source,
                cover,
                canvas_w,
                clip.dst_row + row,
                clip.dst_column,
                source_column,
                source_row,
                src_width,
                coverage_width,
                opacity,
                source_column,
                lo,
            );
        }
        if hi < last {
            composite_range(
                target,
                source,
                cover,
                canvas_w,
                clip.dst_row + row,
                clip.dst_column,
                source_column,
                source_row,
                src_width,
                coverage_width,
                opacity,
                hi,
                last,
            );
        }
    }
}

/// A rectangle of the source that is not composited with the rest.
#[derive(Clone, Copy, Debug)]
pub(super) struct Skip {
    /// Left edge, in source pixels.
    pub x0: usize,
    /// Top edge, in source pixels.
    pub y0: usize,
    /// Right edge, in source pixels.
    pub x1: usize,
    /// Bottom edge, in source pixels.
    pub y1: usize,
}

impl Skip {
    /// Whether the row is inside the rectangle.
    fn contains_row(&self, row: usize) -> bool {
        row >= self.y0 && row < self.y1
    }
}

/// One range of columns of a row, with the arithmetic of the content.
#[allow(clippy::too_many_arguments)]
fn composite_range(
    target: &mut [f32],
    source: &[f32],
    cover: &[f32],
    canvas_w: usize,
    dst_row: usize,
    dst_column: usize,
    src_column: usize,
    src_row: usize,
    src_width: usize,
    coverage_width: usize,
    opacity: f32,
    from: usize,
    to: usize,
) {
    if from >= to {
        return;
    }
    // The source column `from` lands on the canvas column `dst_column` plus
    // the columns it moved since the first one of the clip.
    let start = (dst_row * canvas_w + dst_column + (from - src_column)) * 4;
    let under = &mut target[start..start + (to - from) * 4];
    let pixels = &source[(src_row * src_width + from) * 4..][..(to - from) * 4];
    let mattes = &cover[src_row * coverage_width + from..][..to - from];
    for ((bottom, top), &value) in under
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixels.as_chunks::<4>().0)
        .zip(mattes)
    {
        // `src * matte` is the premultiplied layer: its own alpha is the
        // blend factor, as `_composite` reads `src[..., 3]`.
        let matte = value * opacity;
        let alpha = top[3] * matte;
        for channel in 0..4 {
            bottom[channel] = bottom[channel] * (1.0 - alpha) + top[channel] * matte;
        }
    }
}

/// The caret over the content, with the arithmetic of the rest of the layer.
///
/// DECISION: the caret is painted apart from the content layer (DX-1, paso 4)
/// because it is the only thing that changes twice a second; but the pixels
/// have to be the same ones: the content under the caret is blended with the
/// colour of the caret and the result is composited with the same coverage
/// and row mask as the rest of the layer.
#[allow(clippy::too_many_arguments)]
pub(super) fn composite_caret(
    canvas: &mut Array3<f32>,
    content: &[f32],
    content_width: usize,
    patch: &[f32],
    patch_width: usize,
    patch_height: usize,
    patch_x: usize,
    patch_y: usize,
    coverage: &Array2<f32>,
    mask: &[f32],
    x0: i32,
    y0: i32,
) {
    let (coverage_height, coverage_width) = coverage.dim();
    let canvas_w = canvas_width(canvas);
    let cover = coverage.as_slice().expect("coverage is contiguous");
    // The clips are computed before the mutable borrow of the canvas.
    let clips: Vec<(usize, Span)> = (0..patch_height)
        .filter_map(|row| {
            let source_row = patch_y + row;
            if source_row >= coverage_height || source_row >= mask.len() {
                return None;
            }
            span(
                canvas,
                x0,
                y0,
                patch_x,
                patch_x + patch_width,
                source_row,
                source_row + 1,
            )
            .map(|clip| (row, clip))
        })
        .collect();
    let target = points_mut(canvas);
    for (row, clip) in clips {
        let source_row = patch_y + row;
        let opacity = mask[source_row];
        for index in 0..clip.columns {
            let source_column = clip.src_column + index;
            let dst_column = clip.dst_column + index;
            let patch_index = ((source_row - patch_y) * patch_width + (source_column - patch_x)) * 4;
            let content_index = (source_row * content_width + source_column) * 4;
            let cover = cover[source_row * coverage_width + source_column];
            let matte = cover * opacity;
            // The caret over the content, as if it had been painted inside.
            let k = patch[patch_index + 3];
            let mut combined = [0.0f32; 4];
            for channel in 0..4 {
                combined[channel] =
                    content[content_index + channel] * (1.0 - k) + patch[patch_index + channel];
            }
            let alpha = combined[3] * matte;
            let start = (clip.dst_row * canvas_w + dst_column) * 4;
            for channel in 0..4 {
                target[start + channel] =
                    target[start + channel] * (1.0 - alpha) + combined[channel] * matte;
            }
        }
    }
}

/// `_blend_u8`: a premultiplied float layer over a finished BGRA frame.
pub(super) fn blend(frame: &mut Frame, src: &ArrayView3<f32>, x0: i32, y0: i32) {
    let (height, width, _) = src.dim();
    let (sx0, sy0, dx0, dy0, copy_w, copy_h) = clip_frame(frame, width, height, x0, y0);
    if copy_w == 0 || copy_h == 0 {
        return;
    }
    let source = src.as_slice().expect("layers are contiguous");
    for sy in sy0..sy0 + copy_h {
        for sx in sx0..sx0 + copy_w {
            let index = (sy * width + sx) * 4;
            let top = [
                source[index],
                source[index + 1],
                source[index + 2],
                source[index + 3],
            ];
            let dy = dy0 + (sy - sy0);
            let dx = dx0 + (sx - sx0);
            let index = (dy * frame.width + dx) * 4;
            let under = [
                f32::from(frame.bgra[index + 2]) / 255.0,
                f32::from(frame.bgra[index + 1]) / 255.0,
                f32::from(frame.bgra[index]) / 255.0,
                f32::from(frame.bgra[index + 3]) / 255.0,
            ];
            let mut mixed = [0.0f32; 4];
            for channel in 0..4 {
                mixed[channel] = (top[channel] + under[channel] * (1.0 - top[3])).clamp(0.0, 1.0);
            }
            frame.bgra[index] = quantize_from_unit(mixed[2]);
            frame.bgra[index + 1] = quantize_from_unit(mixed[1]);
            frame.bgra[index + 2] = quantize_from_unit(mixed[0]);
            frame.bgra[index + 3] = quantize_from_unit(mixed[3]);
        }
    }
}

fn quantize_from_unit(value: f32) -> u8 {
    (value * 255.0 + 0.5) as u8
}

fn clip_frame(
    frame: &Frame,
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
) -> (usize, usize, usize, usize, usize, usize) {
    let sx0 = (-x0).max(0) as usize;
    let sy0 = (-y0).max(0) as usize;
    let dx0 = x0.max(0) as usize;
    let dy0 = y0.max(0) as usize;
    let copy_w = width
        .saturating_sub(sx0)
        .min(frame.width.saturating_sub(dx0));
    let copy_h = height
        .saturating_sub(sy0)
        .min(frame.height.saturating_sub(dy0));
    (sx0, sy0, dx0, dy0, copy_w, copy_h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array2;

    fn layer(data: Vec<f32>) -> Array3<f32> {
        let pixels = data.len() / 4;
        Array3::from_shape_vec((1, pixels, 4), data).expect("one row")
    }

    #[test]
    fn frame_conversion_unpremultiplies_and_swaps_channels() {
        let frame = Frame {
            width: 1,
            height: 1,
            bgra: vec![50, 40, 20, 128],
        };
        let rgba = frame.to_rgba_frame();
        // Nearest straight values: 20/128→40, 40/128→80, 50/128→100.
        assert_eq!(rgba.data, vec![40, 80, 100, 128]);
        assert_eq!(rgba.to_bgra_premultiplied(), vec![50, 40, 20, 128]);
    }

    #[test]
    fn straight_round_trip_recovers_premultiplied_pixels() {
        for alpha in 1u8..=255 {
            for value in 0..=alpha {
                let frame = Frame {
                    width: 1,
                    height: 1,
                    bgra: vec![value, 0, 0, alpha],
                };
                assert_eq!(
                    frame.to_rgba_frame().to_bgra_premultiplied()[0],
                    value,
                    "alpha {alpha}, premultiplied {value}"
                );
            }
        }
    }

    #[test]
    fn transparent_pixels_stay_transparent() {
        let frame = Frame {
            width: 1,
            height: 1,
            bgra: vec![20, 30, 40, 0],
        };
        assert_eq!(frame.to_rgba_frame().data, vec![0, 0, 0, 0]);
    }

    #[test]
    fn blending_a_layer_uses_premultiplied_math() {
        let mut frame = Frame {
            width: 1,
            height: 1,
            bgra: vec![0, 0, 0, 0],
        };
        let layer = layer(vec![0.5, 0.0, 0.0, 0.5]);
        blend(&mut frame, &layer.view(), 0, 0);
        assert_eq!(frame.pixel(0, 0), [0, 0, 128, 128]);
    }

    #[test]
    fn clipping_moves_the_source_and_destination_together() {
        let mut canvas = Array3::<f32>::zeros((2, 2, 4));
        let src = Array3::from_shape_fn(
            (1, 1, 4),
            |(_, _, channel)| {
                if channel == 3 {
                    1.0
                } else {
                    0.25
                }
            },
        );
        composite(&mut canvas, &src, -1, -1);
        assert!(canvas[[0, 0, 3]] == 0.0);
        composite(&mut canvas, &src, 1, 1);
        assert_eq!(canvas[[1, 1, 3]], 1.0);
        assert_eq!(canvas[[1, 1, 0]], 0.25);
    }

    #[test]
    fn a_band_leaves_the_centre_of_the_canvas_alone() {
        let mut canvas = Array3::<f32>::zeros((6, 6, 4));
        let alpha = Array2::<f32>::ones((6, 6));
        shade_frame(&mut canvas, &alpha, 0, 0, 1, 1.0);
        assert_eq!(canvas[[3, 3, 3]], 0.0);
        assert_eq!(canvas[[0, 3, 3]], 1.0);
        assert_eq!(canvas[[5, 0, 3]], 1.0);
        assert_eq!(canvas[[3, 5, 3]], 1.0);
        assert_eq!(canvas[[3, 0, 3]], 1.0);
    }

    #[test]
    fn the_row_mask_scales_the_matted_content() {
        let mut canvas = Array3::<f32>::zeros((2, 1, 4));
        let src = Array3::from_shape_fn((2, 1, 4), |(row, _, channel)| {
            if channel == 3 {
                0.5 + row as f32 * 0.25
            } else {
                0.25
            }
        });
        let coverage = Array2::<f32>::ones((2, 1));
        composite_masked(
            &mut canvas,
            src.as_slice().expect("contiguous"),
            1,
            &coverage,
            &[1.0, 0.5],
            1,
            2,
            0,
            0,
            None,
        );
        assert!((canvas[[0, 0, 3]] - 0.5).abs() < 1e-6);
        assert!((canvas[[1, 0, 3]] - 0.375).abs() < 1e-6);
    }
}
