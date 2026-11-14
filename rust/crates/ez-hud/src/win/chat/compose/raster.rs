//! Pixel arithmetic of the chat panel (`chat_paint.py`, F5-15).
//!
//! The Python composition works on a premultiplied `float32` RGBA canvas and
//! only converts to eight-bit BGRA at the very end. The helpers here keep that
//! order: composite, shade, blend and, at the end, quantize.

use ndarray::{Array3, ArrayView2, ArrayView3};

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
        let mut data = Vec::with_capacity(self.bgra.len());
        for pixel in self.bgra.as_chunks::<4>().0 {
            let alpha = pixel[3];
            let straight = |channel: u8| -> u8 {
                if alpha == 0 {
                    0
                } else {
                    (f64::from(channel) * 255.0 / f64::from(alpha))
                        .round_ties_even()
                        .min(255.0) as u8
                }
            };
            data.extend_from_slice(&[
                straight(pixel[2]),
                straight(pixel[1]),
                straight(pixel[0]),
                alpha,
            ]);
        }
        RgbaFrame::new(self.width as u32, self.height as u32, data)
            .expect("a Frame always has its exact byte length")
    }
}

/// Quantizes the premultiplied float canvas to BGRA.
pub(super) fn frame_from_canvas(canvas: &Array3<f32>) -> Frame {
    let (height, width, _) = canvas.dim();
    let mut bgra = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for column in 0..width {
            let red = quantize(canvas[[row, column, 0]]);
            let green = quantize(canvas[[row, column, 1]]);
            let blue = quantize(canvas[[row, column, 2]]);
            let alpha = quantize(canvas[[row, column, 3]]);
            bgra.extend_from_slice(&[blue, green, red, alpha]);
        }
    }
    Frame {
        width,
        height,
        bgra,
    }
}

fn quantize(value: f32) -> u8 {
    (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// `_composite_over`: premultiplied `rgb` plus its `alpha` over the canvas.
pub(super) fn composite_over(
    canvas: &mut Array3<f32>,
    rgb: &ArrayView3<f32>,
    alpha: &ArrayView2<f32>,
    x0: i32,
    y0: i32,
) {
    let (height, width) = alpha.dim();
    let (sx0, sy0, dx0, dy0, copy_w, copy_h) = clip(canvas, width, height, x0, y0);
    if copy_w == 0 || copy_h == 0 {
        return;
    }
    for sy in sy0..sy0 + copy_h {
        for sx in sx0..sx0 + copy_w {
            let a = alpha[[sy, sx]];
            let dy = dy0 + (sy - sy0);
            let dx = dx0 + (sx - sx0);
            for channel in 0..3 {
                canvas[[dy, dx, channel]] =
                    canvas[[dy, dx, channel]] * (1.0 - a) + rgb[[sy, sx, channel]];
            }
            canvas[[dy, dx, 3]] = canvas[[dy, dx, 3]] * (1.0 - a) + a;
        }
    }
}

/// `_shade`: blackens the canvas under `alpha` and adds its coverage to alpha.
pub(super) fn shade(canvas: &mut Array3<f32>, alpha: &ArrayView2<f32>, x0: i32, y0: i32) {
    let (height, width) = alpha.dim();
    let (sx0, sy0, dx0, dy0, copy_w, copy_h) = clip(canvas, width, height, x0, y0);
    if copy_w == 0 || copy_h == 0 {
        return;
    }
    for sy in sy0..sy0 + copy_h {
        for sx in sx0..sx0 + copy_w {
            let a = alpha[[sy, sx]];
            let dy = dy0 + (sy - sy0);
            let dx = dx0 + (sx - sx0);
            for channel in 0..3 {
                canvas[[dy, dx, channel]] *= 1.0 - a;
            }
            canvas[[dy, dx, 3]] = a + canvas[[dy, dx, 3]] * (1.0 - a);
        }
    }
}

/// `_shade_frame`: only the `band`-pixel border of `alpha` darkens the canvas.
pub(super) fn shade_frame(
    canvas: &mut Array3<f32>,
    alpha: &ArrayView2<f32>,
    x0: i32,
    y0: i32,
    band: i32,
) {
    let (height, width) = alpha.dim();
    let band = band.max(1).min(height as i32).min(width as i32) as usize;
    if band * 2 >= height || band * 2 >= width {
        shade(canvas, alpha, x0, y0);
        return;
    }
    for rows in [0..band, height - band..height] {
        shade(
            canvas,
            &alpha.slice(ndarray::s![rows.clone(), ..]),
            x0,
            y0 + rows.start as i32,
        );
    }
    let middle = band..height - band;
    for columns in [0..band, width - band..width] {
        shade(
            canvas,
            &alpha.slice(ndarray::s![middle.clone(), columns.clone()]),
            x0 + columns.start as i32,
            y0 + band as i32,
        );
    }
}

/// `_composite_frame`: only the `band`-pixel border of `src` is composited.
pub(super) fn composite_frame(
    canvas: &mut Array3<f32>,
    src: &ArrayView3<f32>,
    x0: i32,
    y0: i32,
    band: i32,
) {
    let (height, width, _) = src.dim();
    let band = band.max(1).min(height as i32).min(width as i32) as usize;
    if band * 2 >= height || band * 2 >= width {
        composite(canvas, src, x0, y0);
        return;
    }
    for rows in [0..band, height - band..height] {
        composite(
            canvas,
            &src.slice(ndarray::s![rows.clone(), .., ..]),
            x0,
            y0 + rows.start as i32,
        );
    }
    let middle = band..height - band;
    for columns in [0..band, width - band..width] {
        composite(
            canvas,
            &src.slice(ndarray::s![middle.clone(), columns.clone(), ..]),
            x0 + columns.start as i32,
            y0 + band as i32,
        );
    }
}

/// `_composite`: premultiplied `src` over the canvas.
pub(super) fn composite(canvas: &mut Array3<f32>, src: &ArrayView3<f32>, x0: i32, y0: i32) {
    let (height, width, _) = src.dim();
    let (sx0, sy0, dx0, dy0, copy_w, copy_h) = clip(canvas, width, height, x0, y0);
    if copy_w == 0 || copy_h == 0 {
        return;
    }
    for sy in sy0..sy0 + copy_h {
        for sx in sx0..sx0 + copy_w {
            let top_alpha = src[[sy, sx, 3]];
            let dy = dy0 + (sy - sy0);
            let dx = dx0 + (sx - sx0);
            for channel in 0..4 {
                canvas[[dy, dx, channel]] =
                    canvas[[dy, dx, channel]] * (1.0 - top_alpha) + src[[sy, sx, channel]];
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
    for sy in sy0..sy0 + copy_h {
        for sx in sx0..sx0 + copy_w {
            let top = [
                src[[sy, sx, 0]],
                src[[sy, sx, 1]],
                src[[sy, sx, 2]],
                src[[sy, sx, 3]],
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

/// Source and destination offsets of a clipped blit.
#[allow(clippy::too_many_arguments)]
fn clip(
    canvas: &Array3<f32>,
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
) -> (usize, usize, usize, usize, usize, usize) {
    let (canvas_h, canvas_w, _) = canvas.dim();
    clip_bounds(canvas_w, canvas_h, width, height, x0, y0)
}

fn clip_frame(
    frame: &Frame,
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
) -> (usize, usize, usize, usize, usize, usize) {
    clip_bounds(frame.width, frame.height, width, height, x0, y0)
}

fn clip_bounds(
    canvas_w: usize,
    canvas_h: usize,
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
) -> (usize, usize, usize, usize, usize, usize) {
    let sx0 = (-x0).max(0) as usize;
    let sy0 = (-y0).max(0) as usize;
    let dx0 = x0.max(0) as usize;
    let dy0 = y0.max(0) as usize;
    let copy_w = width.saturating_sub(sx0).min(canvas_w.saturating_sub(dx0));
    let copy_h = height.saturating_sub(sy0).min(canvas_h.saturating_sub(dy0));
    (sx0, sy0, dx0, dy0, copy_w, copy_h)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let layer = Array3::from_shape_vec((1, 1, 4), vec![0.5, 0.0, 0.0, 0.5]).expect("one pixel");
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
        composite(&mut canvas, &src.view(), -1, -1);
        assert!(canvas[[0, 0, 3]] == 0.0);
        composite(&mut canvas, &src.view(), 1, 1);
        assert_eq!(canvas[[1, 1, 3]], 1.0);
        assert_eq!(canvas[[1, 1, 0]], 0.25);
    }
}
