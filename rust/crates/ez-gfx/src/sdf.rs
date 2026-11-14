//! Port of `src/ezagent/voice/hud/menu.py 382-699` (F2-08).
//!
//! The menu paints into a straight-alpha `float32` RGBA layer with signed
//! distance fields and a one-logical-pixel anti-alias ramp. [`Layer`] is that
//! buffer; the `fill_*` helpers are the Python module helpers of the same
//! names, keeping numpy's `float32`/`float64` split and the mask cache.

use std::sync::Arc;

use crate::raster::Rgba;
use crate::GfxError;

mod cache;

#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;

/// `tokens.AA_RAMP`: width of the coverage ramp in logical pixels.
const AA_RAMP: f64 = 1.0;
/// `tokens.W`: width the Python `_composite_band` uses when `x1` is omitted.
const W: f64 = 320.0;
/// Premultiplied straight-alpha `float32` RGBA pixels, row-major.
///
/// This is the `(H, W, 4)` buffer the Python menu keeps for its layers and the
/// window buffer it composites onto.
///
/// // DECISION: the layer lives in `sdf` because F2-07 only models byte images;
/// // the menu port reuses this type for every float32 surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

impl Layer {
    /// Creates a transparent black `width`×`height` layer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; pixel_len(width, height)],
        }
    }

    /// Wraps an existing straight-alpha premultiplied buffer.
    pub fn from_raw(width: u32, height: u32, data: Vec<f32>) -> Result<Self, GfxError> {
        let expected = pixel_len(width, height);
        if data.len() != expected {
            return Err(GfxError::BufferLength {
                expected,
                got: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Raw row-major RGBA `f32` values.
    pub fn as_raw(&self) -> &[f32] {
        &self.data
    }

    /// Mutable raw row-major RGBA `f32` values.
    pub fn as_raw_mut(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// Writes `value` into every channel (`layer.fill(value)` in numpy).
    pub fn fill(&mut self, value: f32) {
        self.data.fill(value);
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    fn pixel_mut(&mut self, x: u32, y: u32) -> &mut [f32] {
        let index = self.index(x, y);
        &mut self.data[index..index + 4]
    }
}

fn pixel_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

/// `_color_rgb`: the RGB channels as `float32` in `0..=1`.
fn color_rgb(color: Rgba) -> [f32; 3] {
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
    ]
}

/// `_color_alpha`: the alpha channel as a `float64` in `0..=1`.
fn color_alpha(color: Rgba) -> f64 {
    f64::from(color.a) / 255.0
}

/// Coverage and pixel bounds returned by `_round_mask`.
#[derive(Clone, Debug)]
struct MaskData {
    py0: u32,
    py1: u32,
    px0: u32,
    px1: u32,
    coverage: Vec<f64>,
}

/// `_round_mask`: SDF coverage of a rounded rectangle (optionally a stroke).
fn round_mask(
    width: u32,
    height: u32,
    bounds: [f64; 4],
    radius_top: f64,
    radius_bottom: f64,
    scale: f64,
    stroke: bool,
) -> Option<Arc<MaskData>> {
    let [x0, y0, x1, y1] = bounds;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let key = cache::MaskKey {
        height,
        width,
        x0: cache::round_key(x0, 3),
        y0: cache::round_key(y0, 3),
        x1: cache::round_key(x1, 3),
        y1: cache::round_key(y1, 3),
        radius_top: cache::round_key(radius_top, 3),
        radius_bottom: cache::round_key(radius_bottom, 3),
        scale: cache::round_key(scale, 4),
        stroke,
    };
    if let Some(cached) = cache::get(&key) {
        return Some(cached);
    }
    let mask_box = pixel_bounds(width, height, bounds, scale)?;
    let hw = (x1 - x0) / 2.0;
    let hh = (y1 - y0) / 2.0;
    let rr_top = radius_top.min(hw).min(hh).max(0.0);
    let rr_bottom = radius_bottom.min(hw).min(hh).max(0.0);
    let coverage = sdf_coverage(mask_box, bounds, rr_top, rr_bottom, scale, stroke);
    let mask = Arc::new(MaskData {
        py0: mask_box.py0,
        py1: mask_box.py1,
        px0: mask_box.px0,
        px1: mask_box.px1,
        coverage,
    });
    cache::put(key, Arc::clone(&mask));
    Some(mask)
}

/// Pixel rectangle the SDF is evaluated on, padded by 1.5 logical pixels.
#[derive(Clone, Copy, Debug)]
struct MaskBox {
    px0: u32,
    px1: u32,
    py0: u32,
    py1: u32,
}

fn pixel_bounds(width: u32, height: u32, bounds: [f64; 4], scale: f64) -> Option<MaskBox> {
    let [x0, y0, x1, y1] = bounds;
    let pad = 1.5;
    let mask_box = MaskBox {
        px0: clamp_floor((x0 - pad) * scale, width),
        px1: clamp_ceil((x1 + pad) * scale, width),
        py0: clamp_floor((y0 - pad) * scale, height),
        py1: clamp_ceil((y1 + pad) * scale, height),
    };
    if mask_box.px1 <= mask_box.px0 || mask_box.py1 <= mask_box.py0 {
        None
    } else {
        Some(mask_box)
    }
}

fn sdf_coverage(
    mask_box: MaskBox,
    bounds: [f64; 4],
    rr_top: f64,
    rr_bottom: f64,
    scale: f64,
    stroke: bool,
) -> Vec<f64> {
    let [x0, y0, x1, y1] = bounds;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let mut coverage = Vec::with_capacity(
        (mask_box.px1 - mask_box.px0) as usize * (mask_box.py1 - mask_box.py0) as usize,
    );
    for y in mask_box.py0..mask_box.py1 {
        let yy = (y as f32 + 0.5) / scale as f32;
        let radius = if yy < cy as f32 { rr_top } else { rr_bottom };
        let qy = f64::from((yy - cy as f32).abs()) - (hh - radius);
        for x in mask_box.px0..mask_box.px1 {
            let xx = (x as f32 + 0.5) / scale as f32;
            let qx = f64::from((xx - cx as f32).abs()) - (hw - radius);
            let distance =
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let cover = if stroke {
                0.5 - (distance.abs() - 0.5) / AA_RAMP
            } else {
                0.5 - distance / AA_RAMP
            };
            coverage.push(cover.clamp(0.0, 1.0));
        }
    }
    coverage
}

fn clamp_floor(value: f64, limit: u32) -> u32 {
    value.floor().clamp(0.0, f64::from(limit)) as u32
}

fn clamp_ceil(value: f64, limit: u32) -> u32 {
    value.ceil().clamp(0.0, f64::from(limit)) as u32
}

/// `_paint_mask`: composites one coverage mask onto `layer`.
fn paint_mask(layer: &mut Layer, mask: &MaskData, color: Rgba, alpha: f64, replace: bool) {
    if mask.coverage.is_empty() || alpha <= 0.0 {
        return;
    }
    let factor = alpha * color_alpha(color);
    let rgb = color_rgb(color);
    let columns = (mask.px1 - mask.px0) as usize;
    for (row, y) in (mask.py0..mask.py1).enumerate() {
        for (column, x) in (mask.px0..mask.px1).enumerate() {
            let coverage = mask.coverage[row * columns + column];
            let a = if factor >= 1.0 {
                coverage
            } else {
                coverage * factor
            };
            let pixel = layer.pixel_mut(x, y);
            if replace {
                for channel in 0..3 {
                    pixel[channel] = (f64::from(rgb[channel]) * a) as f32;
                }
                pixel[3] = a as f32;
            } else {
                let one = 1.0 - a;
                for channel in 0..3 {
                    let mixed = (f64::from(pixel[channel]) * one) as f32;
                    pixel[channel] = (f64::from(mixed) + f64::from(rgb[channel]) * a) as f32;
                }
                let mixed = (f64::from(pixel[3]) * one) as f32;
                pixel[3] = (f64::from(mixed) + a) as f32;
            }
        }
    }
}

/// Optional arguments of [`fill_round_rect_opts`].
#[derive(Clone, Copy, Debug, Default)]
pub struct FillOptions {
    /// Overrides the top corner radius.
    pub radius_top: Option<f64>,
    /// Overrides the bottom corner radius.
    pub radius_bottom: Option<f64>,
    /// Writes instead of blending, for a region that is still empty.
    pub replace: bool,
}

/// `_fill_round_rect`: anti-aliased fill of a rounded rectangle.
pub fn fill_round_rect(
    layer: &mut Layer,
    bounds: [f64; 4],
    radius: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    fill_round_rect_opts(
        layer,
        bounds,
        radius,
        color,
        scale,
        alpha,
        FillOptions::default(),
    );
}

/// `_fill_round_rect` with the keyword-only arguments of the Python helper.
pub fn fill_round_rect_opts(
    layer: &mut Layer,
    bounds: [f64; 4],
    radius: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
    options: FillOptions,
) {
    let top = options.radius_top.unwrap_or(radius);
    let bottom = options.radius_bottom.unwrap_or(radius);
    let Some(mask) = round_mask(layer.width, layer.height, bounds, top, bottom, scale, false)
    else {
        return;
    };
    paint_mask(layer, &mask, color, alpha, options.replace);
}

/// `_fill_round_gradient`: rounded rectangle filled with horizontal `stops`
/// (RGB, `0..=255`).
pub fn fill_round_gradient(
    layer: &mut Layer,
    bounds: [f64; 4],
    radius: f64,
    stops: &[[f32; 3]],
    scale: f64,
    alpha: f64,
) {
    if stops.is_empty() {
        // DECISION: numpy raises IndexError on an empty stop list; a no-op is safer.
        return;
    }
    let single = stops.len() == 1;
    let [x0, _, x1, _] = bounds;
    let Some(mask) = round_mask(
        layer.width,
        layer.height,
        bounds,
        radius,
        radius,
        scale,
        false,
    ) else {
        return;
    };
    let last = stops.len().saturating_sub(2);
    let segments = stops.len() - 1;
    let denominator = (x1 - x0).max(1e-6) as f32;
    let columns = (mask.px1 - mask.px0) as usize;
    for (row, y) in (mask.py0..mask.py1).enumerate() {
        for (column, x) in (mask.px0..mask.px1).enumerate() {
            let xs = (x as f32 + 0.5) / scale as f32;
            let t = ((xs - x0 as f32) / denominator).clamp(0.0, 1.0) * segments as f32;
            let index = if single {
                0
            } else {
                (t.floor() as i32).min(last as i32) as usize
            };
            let f = if single {
                0.0
            } else {
                f64::from(t) - index as f64
            };
            let a = mask.coverage[row * columns + column] * alpha;
            let mut rgb = [0.0f64; 3];
            for (channel, value) in rgb.iter_mut().enumerate() {
                let low = f64::from(stops[index][channel]);
                let high = if single {
                    low
                } else {
                    f64::from(stops[index + 1][channel])
                };
                *value = (low * (1.0 - f) + high * f) / 255.0;
            }
            let one = 1.0 - a;
            let pixel = layer.pixel_mut(x, y);
            for (channel, value) in rgb.iter().enumerate() {
                pixel[channel] = (value * a + f64::from(pixel[channel]) * one) as f32;
            }
            pixel[3] = (a + f64::from(pixel[3]) * one) as f32;
        }
    }
}

/// `_stroke_round_rect`: one-logical-pixel hairline ring on the border.
pub fn stroke_round_rect(
    layer: &mut Layer,
    bounds: [f64; 4],
    radius: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    let Some(mask) = round_mask(
        layer.width,
        layer.height,
        bounds,
        radius,
        radius,
        scale,
        true,
    ) else {
        return;
    };
    paint_mask(layer, &mask, color, alpha, false);
}

/// `_fill_circle`: anti-aliased filled circle.
pub fn fill_circle(
    layer: &mut Layer,
    cx: f64,
    cy: f64,
    diameter: f64,
    color: Rgba,
    scale: f64,
    alpha: f64,
) {
    let radius = (diameter / 2.0).max(0.0);
    fill_round_rect(
        layer,
        [cx - radius, cy - radius, cx + radius, cy + radius],
        radius,
        color,
        scale,
        alpha,
    );
}

/// `_fill_vertical_gradient`: opaque-to-transparent vertical gradient used by
/// the scroll edges.
pub fn fill_vertical_gradient(
    layer: &mut Layer,
    box_: [f64; 4],
    color: Rgba,
    scale: f64,
    alpha_start: f64,
    alpha_end: f64,
) {
    let [x0, x1, y0, y1] = box_;
    if y1 <= y0 || x1 <= x0 {
        return;
    }
    let px0 = clamp_floor(x0 * scale, layer.width);
    let px1 = clamp_ceil(x1 * scale, layer.width);
    let py0 = clamp_floor(y0 * scale, layer.height);
    let py1 = clamp_ceil(y1 * scale, layer.height);
    if px1 <= px0 || py1 <= py0 {
        return;
    }
    let rows = py1 - py0;
    let rgb = color_rgb(color);
    let delta = (alpha_end - alpha_start) as f32;
    for (row, y) in (py0..py1).enumerate() {
        // numpy's linspace(0, 1, rows) is correctly rounded in f64, then f32.
        let k = (if rows > 1 {
            row as f64 / f64::from(rows - 1)
        } else {
            0.0
        }) as f32;
        let a = alpha_start as f32 + delta * k;
        let one = 1.0 - a;
        for x in px0..px1 {
            let pixel = layer.pixel_mut(x, y);
            for (channel, value) in rgb.iter().enumerate() {
                pixel[channel] = value * a + pixel[channel] * one;
            }
            pixel[3] = a + pixel[3] * one;
        }
    }
}

/// `_composite_band`: composites a horizontal band of a premultiplied `layer`
/// onto `dst`.
///
/// // DECISION: the Python default `x1=None` uses `tokens.W` (320); the value is
/// // repeated here because `ez-gfx` cannot depend on `ez-hud`. Pass `x1` explicitly.
pub fn composite_band(
    dst: &mut Layer,
    layer: &Layer,
    y0: f64,
    y1: f64,
    scale: f64,
    x0: f64,
    x1: Option<f64>,
) -> Result<(), GfxError> {
    if y1 <= y0 {
        return Ok(());
    }
    let p0 = clamp_round(y0 * scale, dst.height);
    let p1 = clamp_round(y1 * scale, dst.height);
    if p1 <= p0 {
        return Ok(());
    }
    let right = x1.unwrap_or(W);
    let q0 = clamp_floor(x0 * scale, dst.width);
    let q1 = clamp_ceil(right * scale, dst.width);
    if q1 <= q0 {
        return Ok(());
    }
    if p1 > layer.height || q1 > layer.width {
        return Err(GfxError::SizeMismatch);
    }
    for y in p0..p1 {
        for x in q0..q1 {
            let source = layer_index(layer, x, y);
            let destination = layer_index(dst, x, y);
            let source_alpha = layer.data[source + 3];
            let one = 1.0 - source_alpha;
            for channel in 0..3 {
                let mixed = dst.data[destination + channel] * one;
                dst.data[destination + channel] = mixed + layer.data[source + channel];
            }
            let mixed = dst.data[destination + 3] * one;
            dst.data[destination + 3] = mixed + source_alpha;
        }
    }
    Ok(())
}

fn layer_index(layer: &Layer, x: u32, y: u32) -> usize {
    (y as usize * layer.width as usize + x as usize) * 4
}

fn clamp_round(value: f64, limit: u32) -> u32 {
    value.round_ties_even().clamp(0.0, f64::from(limit)) as u32
}

/// `_fit_text`: truncates with `…` using the measured ink width of `text`.
///
/// The Python helper uses `text_size(text, style, scale)[0]`; the caller
/// supplies that width through `measure`.
///
/// // DECISION: text metrics belong to F2-10, so the width arrives as a closure;
/// // the Python `lru_cache` stays with the caller as well.
pub fn fit_text(text: &str, max_width: f64, measure: impl Fn(&str) -> f64) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    if measure(text) <= max_width {
        return text.to_string();
    }
    const ELLIPSIS: &str = "…";
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0usize, chars.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let prefix: String = chars[..middle].iter().collect();
        let candidate = format!("{}…", py_rstrip(&prefix));
        if measure(&candidate) <= max_width {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if low == 0 {
        return ELLIPSIS.to_string();
    }
    let prefix: String = chars[..low].iter().collect();
    format!("{}…", py_rstrip(&prefix))
}

/// `str.rstrip()`: `char::is_whitespace` plus the `\x1c`-`\x1f` separators.
fn py_rstrip(text: &str) -> &str {
    text.trim_end_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

#[cfg(test)]
fn cached_masks() -> usize {
    cache::len()
}
