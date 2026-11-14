//! Analytic rasterizer of `icons.py`: pixel centres, segment distance and the
//! one-pass coverage formula.

use ndarray::Array2;

use super::glyphs::Pen;
use super::ICON_GRID;

/// Pixel centres of a `pixels`² canvas, in grid units.
pub(crate) fn grid(pixels: u32) -> (Vec<f32>, Vec<f32>) {
    let k = (f64::from(pixels) / ICON_GRID) as f32;
    let len = (pixels * pixels) as usize;
    let mut px = Vec::with_capacity(len);
    let mut py = Vec::with_capacity(len);
    for y in 0..pixels {
        let gy = (y as f32 + 0.5) / k;
        for x in 0..pixels {
            px.push((x as f32 + 0.5) / k);
            py.push(gy);
        }
    }
    (px, py)
}

/// Distance (grid units) from each point to the nearest segment.
///
/// `px` and `py` are the flattened point grid, in the same order; `segs` holds
/// `(x0, y0, x1, y1)` rows like the `(N, 4)` float32 array of the Python
/// signature. The result has one distance per point.
pub fn segment_distance(px: &[f32], py: &[f32], segs: &[[f32; 4]]) -> Vec<f32> {
    assert_eq!(px.len(), py.len(), "px and py must have the same length");
    let mut best = vec![f32::INFINITY; px.len()];
    for segment in segs {
        let (ax, ay) = (segment[0], segment[1]);
        let (dx, dy) = (segment[2] - ax, segment[3] - ay);
        let length = (dx * dx + dy * dy).max(1e-9f32);
        for (index, (&x, &y)) in px.iter().zip(py).enumerate() {
            let (rx, ry) = (x - ax, y - ay);
            let t = ((rx * dx + ry * dy) / length).clamp(0.0, 1.0);
            let (qx, qy) = (rx - t * dx, ry - t * dy);
            best[index] = best[index].min((qx * qx + qy * qy).sqrt());
        }
    }
    best
}

/// `_coverage(distance_px, radius_px)`.
pub(crate) fn coverage(distance: f32, radius: f32) -> f32 {
    (radius - distance + 0.5).clamp(0.0, 1.0)
}

/// `_rrect_distance(px, py, box)` for one pixel.
fn rrect_distance(x: f32, y: f32, box_: [f64; 5]) -> f32 {
    let [x0, y0, x1, y1, r] = box_;
    let center_x = ((x0 + x1) / 2.0) as f32;
    let center_y = ((y0 + y1) / 2.0) as f32;
    let hx = ((x1 - x0) / 2.0 - r) as f32;
    let hy = ((y1 - y0) / 2.0 - r) as f32;
    let qx = (x - center_x).abs() - hx;
    let qy = (y - center_y).abs() - hy;
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    outside + qx.max(qy).min(0.0) - r as f32
}

/// Strokes and fills of one glyph to a `side`² coverage mask.
pub(crate) fn rasterize(pen: &Pen, side: u32, stroke: f64) -> Array2<f32> {
    let (px, py) = grid(side);
    let k = (f64::from(side) / ICON_GRID) as f32;
    let mut cover = vec![0.0f32; px.len()];
    if !pen.segs.is_empty() {
        let segs: Vec<[f32; 4]> = pen
            .segs
            .iter()
            .map(|s| [s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32])
            .collect();
        let distance = segment_distance(&px, &py, &segs);
        let radius = (stroke / 2.0) as f32;
        for (slot, d) in cover.iter_mut().zip(distance) {
            *slot = coverage(d * k, radius);
        }
    }
    for box_ in &pen.fills {
        for (slot, (&x, &y)) in cover.iter_mut().zip(px.iter().zip(&py)) {
            *slot = slot.max(coverage(rrect_distance(x, y, *box_) * k, 0.0));
        }
    }
    Array2::from_shape_vec((side as usize, side as usize), cover)
        .expect("the grid has exactly side² points")
}

/// Applies a coverage formula to flat distances and reshapes them.
pub(crate) fn coverage_mask(
    distance: &[f32],
    side: u32,
    formula: impl Fn(f32) -> f32,
) -> Array2<f32> {
    let values: Vec<f32> = distance.iter().map(|&d| formula(d)).collect();
    Array2::from_shape_vec((side as usize, side as usize), values)
        .expect("the grid has exactly side² points")
}

/// `_slash_points(progress)`: the diagonal from `(4, 4)` to `(4 + 16p, 4 + 16p)`.
pub(crate) fn slash_points(progress: f64) -> ((f64, f64), (f64, f64)) {
    let end = super::SLASH_FROM + super::SLASH_SPAN * progress.clamp(0.0, 1.0);
    ((super::SLASH_FROM, super::SLASH_FROM), (end, end))
}
