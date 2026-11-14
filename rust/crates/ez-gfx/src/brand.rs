//! Port of `src/ezagent/voice/brand.py` (F2-11).
//!
//! The EzAgent mark: an opening quotation mark made of two drops of «Marea»
//! over a rounded square of bluish graphite. The geometry of the drops is
//! shared by the raster (`mark_image`, `drops_coverage`) and the SVG.

mod fluid;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::f64::consts::FRAC_PI_2;
use std::sync::LazyLock;

use crate::raster::{Image, Mask, Rgba};
use crate::resample::Resampling;
use crate::{draw::Draw, icons::py_round};
use ndarray::Array2;

/// Graphite at the bottom of the square.
pub const INK: [u8; 3] = [0x07, 0x16, 0x1B];
/// The same graphite a little lighter at the top.
pub const INK_TOP: [u8; 3] = [0x0D, 0x2A, 0x31];

/// Tile corner radius, as a fraction of the side (`_TILE_RADIUS`).
const TILE_RADIUS: f64 = 0.22;
/// Head radius of a drop, in unit coordinates (`_DROP_R`).
const DROP_R: f64 = 0.125;
/// Distance between the two heads (`_DROP_SPREAD`).
const DROP_SPREAD: f64 = 0.28;
/// Height of the heads (`_DROP_Y`).
const DROP_Y: f64 = 0.60;
/// Head to tail tip (`_DROP_TAIL`).
const DROP_TAIL: (f64, f64) = (0.075, -0.30);
/// Control point of the tail curve (`_DROP_BEND`).
const DROP_BEND: (f64, f64) = (-0.115, -0.20);
/// How much the tail tapers (`_DROP_TAPER`).
const DROP_TAPER: f64 = 0.80;
/// Samples of the tail curve (`_STEPS`).
const STEPS: usize = 220;
/// Side of the working canvas in `mark_image`.
const BIG: u32 = 1024;

/// Failure of a brand raster; the text is Pillow's `ValueError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrandError {
    /// `height and width must be > 0`.
    EmptySize,
}

impl std::fmt::Display for BrandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("height and width must be > 0")
    }
}

impl std::error::Error for BrandError {}

/// Point `i` of the tail (quadratic Bézier) and its fraction `t`.
fn curve(i: usize, head: (f64, f64)) -> (f64, f64, f64) {
    let t = i as f64 / STEPS as f64;
    let (hx, hy) = head;
    let (tx, ty) = (hx + DROP_TAIL.0, hy + DROP_TAIL.1);
    let (bx, by) = (hx + DROP_BEND.0, hy + DROP_BEND.1);
    let one = 1.0 - t;
    (
        one * one * hx + 2.0 * one * t * bx + t * t * tx,
        one * one * hy + 2.0 * one * t * by + t * t * ty,
        t,
    )
}

/// Radius of the tail at `t` (`_radius`).
fn radius(t: f64) -> f64 {
    DROP_R * (1.0 - DROP_TAPER * t.powf(0.85))
}

/// The two head centres (`_heads`).
fn heads() -> [(f64, f64); 2] {
    let half = DROP_SPREAD / 2.0 + 0.005;
    [(0.5 - half, DROP_Y), (0.5 + half, DROP_Y)]
}

/// Mask of the two drops: circles that taper along the curve.
fn drops_mask(side: u32) -> Mask {
    let mut mask = Mask::new(side, side, 0);
    {
        let mut draw = Draw::new(&mut mask);
        for head in heads() {
            for i in 0..=STEPS {
                let (x, y, t) = curve(i, head);
                let rr = radius(t) * f64::from(side);
                let (cx, cy) = (x * f64::from(side), y * f64::from(side));
                draw.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], Some(255u8), None, 0)
                    .expect("the drop ellipses are ordered");
            }
        }
    }
    mask
}

/// `_PIL_bbox`: `Image.getbbox()` for mode `L` (non-zero pixels).
fn bbox(mask: &Mask) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..mask.height() {
        let row = mask.row(y);
        let Some(first) = row.iter().position(|&value| value != 0) else {
            continue;
        };
        let last = row.iter().rposition(|&value| value != 0).unwrap_or(first) as u32;
        bounds = Some(match bounds {
            None => (first as u32, y, last + 1, y + 1),
            Some((left, top, right, bottom)) => (
                left.min(first as u32),
                top,
                right.max(last + 1),
                bottom.max(y + 1),
            ),
        });
    }
    bounds
}

/// `Image.crop(box)` for mode `L`: pixels outside the source are zero.
fn crop(mask: &Mask, left: i32, top: i32, right: i32, bottom: i32) -> Mask {
    let width = (right - left).max(0) as u32;
    let height = (bottom - top).max(0) as u32;
    let mut out = Mask::new(width, height, 0);
    for y in 0..height {
        let source_y = top + y as i32;
        if source_y < 0 || source_y >= mask.height() as i32 {
            continue;
        }
        let source = mask.row(source_y as u32);
        let target = out.row_mut(y);
        for x in 0..width {
            let source_x = left + x as i32;
            if source_x >= 0 && source_x < mask.width() as i32 {
                target[x as usize] = source[source_x as usize];
            }
        }
    }
    out
}

/// LRU cap of `drops_coverage` (`lru_cache(maxsize=8)`).
const DROPS_CACHE_LIMIT: usize = 8;

thread_local! {
    // DECISION: `lru_cache` is thread-local, like the icon caches.
    static DROPS: RefCell<Vec<(u32, Array2<f32>)>> = const { RefCell::new(Vec::new()) };
}

/// Coverage (`0..=1`, `float32`) of the drops cropped to their box, `size`².
///
/// It is the silhouette alone, without the square, centred on the visual
/// centre of the drops.
pub fn drops_coverage(size: u32) -> Result<Array2<f32>, BrandError> {
    if size == 0 {
        return Err(BrandError::EmptySize);
    }
    if let Some(cached) = DROPS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let position = cache.iter().position(|(key, _)| *key == size)?;
        let entry = cache.remove(position);
        let coverage = entry.1.clone();
        cache.push(entry);
        Some(coverage)
    }) {
        return Ok(cached);
    }
    let side = (8.0 * f64::from(size) / 0.56).round_ties_even() as u32;
    let mask = drops_mask(side);
    let (left, top, right, bottom) = bbox(&mask).unwrap_or((0, 0, mask.width(), mask.height()));
    let half = f64::from((right - left).max(bottom - top)) / 2.0 * 1.04;
    let (cx, cy) = (f64::from(left + right) / 2.0, f64::from(top + bottom) / 2.0);
    let box_ = (
        py_round(cx - half),
        py_round(cy - half),
        py_round(cx + half),
        py_round(cy + half),
    );
    let cropped = crop(&mask, box_.0, box_.1, box_.2, box_.3);
    let resized = cropped.resize(size, size, Resampling::Lanczos);
    let values = resized
        .as_raw()
        .iter()
        .map(|&value| f32::from(value) / 255.0)
        .collect();
    let coverage = Array2::from_shape_vec((size as usize, size as usize), values)
        .expect("the mask has exactly size² pixels");
    DROPS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= DROPS_CACHE_LIMIT {
            cache.remove(0);
        }
        cache.push((size, coverage.clone()));
    });
    Ok(coverage)
}

/// Rounded square of graphite with a very soft vertical gradient.
fn tile(side: u32) -> Image {
    let mut data = vec![0u8; side as usize * side as usize * 3];
    for y in 0..side {
        let k = if side > 1 {
            (f64::from(y) / f64::from(side - 1)) as f32
        } else {
            0.0
        };
        let mut color = [0u8; 3];
        for (channel, value) in color.iter_mut().enumerate() {
            *value = (f32::from(INK_TOP[channel]) * (1.0 - k) + f32::from(INK[channel]) * k) as u8;
        }
        let row = y as usize * side as usize * 3;
        for x in 0..side as usize {
            let index = row + x * 3;
            data[index..index + 3].copy_from_slice(&color);
        }
    }
    let base = Image::from_rgb(side, side, data).expect("the tile has side² RGB pixels");
    let mut mask = Mask::new(side, side, 0);
    Draw::new(&mut mask)
        .rounded_rectangle(
            [0.0, 0.0, f64::from(side - 1), f64::from(side - 1)],
            (TILE_RADIUS * f64::from(side)).trunc(),
            Some(255u8),
            None,
            0,
        )
        .expect("the tile rectangle is ordered");
    let mut out = Image::new(side, side, Rgba::TRANSPARENT);
    out.paste_masked(&base, (0, 0), &mask)
        .expect("the tile mask matches its source");
    out
}

/// The mark on a square RGBA canvas; `tile` false leaves it transparent.
///
/// Python defaults are `size = 256` and `tile = True`; Rust callers pass both.
pub fn mark_image(size: u32, tile: bool) -> Result<Image, BrandError> {
    if size == 0 {
        return Err(BrandError::EmptySize);
    }
    let mask = drops_mask(BIG * 2).resize(BIG, BIG, Resampling::Lanczos);
    let fluid = Image::from_rgb(BIG, BIG, fluid::fluid_frame(BIG))
        .expect("the fluid frame has 1024² RGB pixels");
    let mut img = if tile {
        self::tile(BIG)
    } else {
        Image::new(BIG, BIG, Rgba::TRANSPARENT)
    };
    img.paste_masked(&fluid, (0, 0), &mask)
        .expect("the drops mask matches the fluid");

    // Slight shine at the top, clipped to the drops: the glass light of the bar.
    let mut shine = Mask::new(BIG, BIG, 0);
    Draw::new(&mut shine)
        .ellipse(
            [
                0.1 * f64::from(BIG),
                -0.15 * f64::from(BIG),
                0.9 * f64::from(BIG),
                0.38 * f64::from(BIG),
            ],
            Some(38u8),
            None,
            0,
        )
        .expect("the shine ellipse is ordered");
    let shine = shine.gaussian_blur(0.04 * BIG as f32);
    let mut combined = Vec::with_capacity(BIG as usize * BIG as usize);
    for (&light, &cover) in shine.as_raw().iter().zip(mask.as_raw()) {
        combined.push((f32::from(light) * f32::from(cover) / 255.0) as u8);
    }
    let combined = Mask::from_raw(BIG, BIG, combined).expect("the shine mask has 1024² pixels");
    img.paste_masked(&Image::new(BIG, BIG, Rgba::WHITE), (0, 0), &combined)
        .expect("the shine mask matches the white layer");
    Ok(img.resize(size, size, Resampling::Lanczos))
}

/// SVG outline of one drop: tail sides, tip and head as circles.
fn drop_path(head: (f64, f64), units: f64) -> String {
    let mut left = Vec::with_capacity(49);
    let mut right = Vec::with_capacity(49);
    for j in 0..=48 {
        let i = py_round(j as f64 * STEPS as f64 / 48.0).max(0) as usize;
        let (x, y, t) = curve(i, head);
        let (x2, y2, _) = curve((i + 1).min(STEPS), head);
        let (x1, y1, _) = curve(i.saturating_sub(1), head);
        let angle = (y2 - y1).atan2(x2 - x1) + FRAC_PI_2;
        let r = radius(t);
        left.push(((x + r * angle.cos()) * units, (y + r * angle.sin()) * units));
        right.push(((x - r * angle.cos()) * units, (y - r * angle.sin()) * units));
    }
    let mut points = left;
    points.extend(right.into_iter().rev());
    let mut path = String::from("M");
    for (index, (px, py)) in points.iter().enumerate() {
        if index > 0 {
            path.push_str(" L");
        }
        path.push_str(&format!("{px:.2} {py:.2}"));
    }
    path.push_str(" Z");
    let (tip_x, tip_y, tip_t) = curve(STEPS, head);
    let tip = format!(
        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\"/>",
        tip_x * units,
        tip_y * units,
        radius(tip_t) * units
    );
    let cap = format!(
        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\"/>",
        head.0 * units,
        head.1 * units,
        DROP_R * units
    );
    format!("<path d=\"{path}\"/>{cap}{tip}")
}

fn svg_text() -> String {
    let units = 48.0;
    let drops: String = heads().iter().map(|&head| drop_path(head, units)).collect();
    let r = TILE_RADIUS * units;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\" width=\"256\" height=\"256\">\n\
         \x20 <defs>\n\
         \x20   <linearGradient id=\"tile\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\n\
         \x20     <stop offset=\"0\" stop-color=\"#{:02X}{:02X}{:02X}\"/>\n\
         \x20     <stop offset=\"1\" stop-color=\"#{:02X}{:02X}{:02X}\"/>\n\
         \x20   </linearGradient>\n\
         \x20   <linearGradient id=\"marea\" gradientUnits=\"userSpaceOnUse\" x1=\"10\" y1=\"12\" x2=\"40\" y2=\"36\">\n\
         \x20     <stop offset=\"0\" stop-color=\"#3E9FB0\"/>\n\
         \x20     <stop offset=\"0.45\" stop-color=\"#1FAE9F\"/>\n\
         \x20     <stop offset=\"1\" stop-color=\"#A8E6C8\"/>\n\
         \x20   </linearGradient>\n\
         \x20 </defs>\n\
         \x20 <rect width=\"48\" height=\"48\" rx=\"{r:.2}\" fill=\"url(#tile)\"/>\n\
         \x20 <g fill=\"url(#marea)\">{drops}</g>\n\
         </svg>\n",
        INK_TOP[0],
        INK_TOP[1],
        INK_TOP[2],
        INK[0],
        INK[1],
        INK[2],
    )
}

/// The brand SVG, computed once at first use (`brand.SVG`).
pub static SVG: LazyLock<String> = LazyLock::new(svg_text);
