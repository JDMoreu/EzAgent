//! Shape, body, ring and shadow of the chat panel (`chat_paint.py`).
//!
//! Every pass premultiplies and composites in one walk of contiguous rows, so
//! no full-size temporary is allocated per frame (numpy broadcast the same
//! multiplications over the whole array).

use ndarray::{Array2, Array3};

use crate::geometry::{
    lerp, lerp_rect, rounded_coverage, rounded_shadow, smoothstep, Rect, MARGIN, PANEL_RADIUS,
};
use crate::theme::card_palette;

use super::raster::{canvas_width, points, points_mut, shade_rows, span};
use super::round_i32;
use super::{row_reveal, shape_progress, Scene};

/// Shape rectangle, radius and opacities for the current morph progress.
#[allow(clippy::too_many_arguments)]
pub(super) fn shape_geometry(
    scene: &Scene<'_>,
    p: f64,
    opening: bool,
    panel: Rect,
    scale: f64,
    reduced: bool,
    fly: bool,
    form: Option<Rect>,
) -> (Rect, f64, f64, f64, f64) {
    if fly {
        let form = form.expect("fly has a form");
        let rect = lerp_rect(form, panel, shape_progress(p, opening));
        let form_radius = if scene.ui.layout() == "circle" {
            (form.2 - form.0).min(form.3 - form.1) / 2.0
        } else {
            (form.3 - form.1) / 2.0
        };
        (
            rect,
            lerp(form_radius, PANEL_RADIUS * scale, p.min(1.0)),
            smoothstep((p - 0.4) / 0.55),
            1.0,
            smoothstep((p - 0.6) / 0.4),
        )
    } else {
        let grow = if reduced { 1.0 } else { 0.95 + 0.05 * p };
        let cx = (panel.0 + panel.2) / 2.0;
        let cy = (panel.1 + panel.3) / 2.0;
        let half_w = (panel.2 - panel.0) / 2.0 * grow;
        let half_h = (panel.3 - panel.1) / 2.0 * grow;
        (
            (cx - half_w, cy - half_h, cx + half_w, cy + half_h),
            PANEL_RADIUS * scale,
            1.0,
            smoothstep(p),
            p,
        )
    }
}

/// `row_reveal` on opening, a flat fade otherwise.
pub(super) fn content_mask(
    rows: usize,
    p: f64,
    opening: bool,
    fly: bool,
    reduced: bool,
    alpha: f64,
) -> Vec<f32> {
    if fly {
        row_reveal(rows, p, opening)
    } else {
        let value = if reduced {
            alpha as f32
        } else {
            smoothstep((p - 0.3) / 0.7) as f32
        };
        vec![value; rows]
    }
}

/// Vertical offset while the content rises on opening.
pub(super) fn content_lift(mask: &[f32], fly: bool, opening: bool, scale: f64) -> i32 {
    if fly && opening {
        let mean = mask.iter().sum::<f32>() / mask.len() as f32;
        round_i32((1.0 - f64::from(mean)) * 10.0 * scale)
    } else {
        0
    }
}

/// `_paint_shape`: body (or fluid) plus shadow and ring.
pub(super) fn paint_shape(
    canvas: &mut Array3<f32>,
    scene: &Scene<'_>,
    rect: Rect,
    radius: f64,
    solid: f64,
    alpha_k: f64,
    shadow_k: f64,
) {
    let x0 = round_i32(rect.0);
    let y0 = round_i32(rect.1);
    let width = round_i32(rect.2) - x0;
    let height = round_i32(rect.3) - y0;
    if width < 2 || height < 2 {
        return;
    }
    let (w, h) = (width as usize, height as usize);
    let coverage = rounded_coverage(width, height, radius, 0.0);
    let body = scene
        .ui
        .chat_body_rgb()
        .map(|value| f32::from(value) / 255.0);
    let fluid = if solid < 0.999 {
        scene
            .ui
            .chat_fluid(w, h, scene.now)
            .filter(|values| values.dim() == (h, w, 3))
    } else {
        None
    };
    shadow_under(canvas, rect, radius, shadow_k * alpha_k, scene.ui.scale());
    paint_body(
        canvas,
        body,
        fluid.as_ref(),
        &coverage,
        alpha_k,
        solid,
        x0,
        y0,
    );
    if solid >= 0.999 {
        paint_ring(canvas, scene, &coverage, radius, solid, alpha_k, x0, y0);
    }
}

/// Body colour or fluid, premultiplied by coverage and alpha, written straight
/// onto the canvas (`rgb * alpha` then `_composite_over` in the Python).
#[allow(clippy::too_many_arguments)]
fn paint_body(
    canvas: &mut Array3<f32>,
    body: [f32; 3],
    fluid: Option<&Array3<f32>>,
    coverage: &Array2<f32>,
    alpha_k: f64,
    solid: f64,
    x0: i32,
    y0: i32,
) {
    let (h, w) = coverage.dim();
    let Some(clip) = span(canvas, x0, y0, 0, w, 0, h) else {
        return;
    };
    let canvas_w = canvas_width(canvas);
    let cover = coverage.as_slice().expect("coverage is contiguous");
    let fluid = fluid.map(points);
    let alpha_k = alpha_k as f32;
    let solid = solid as f32;
    let target = points_mut(canvas);
    for row in 0..clip.rows {
        let source_row = clip.src_row + row;
        let source_column = clip.src_column;
        let mattes = &cover[source_row * w + source_column..][..clip.columns];
        let start = ((clip.dst_row + row) * canvas_w + clip.dst_column) * 4;
        let under = &mut target[start..start + clip.columns * 4];
        let fluid_row = fluid.map(|values| &values[(source_row * w + source_column) * 3..]);
        for (column, (bottom, &mate)) in under
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(mattes)
            .enumerate()
        {
            let alpha = mate * alpha_k;
            let keep = 1.0 - alpha;
            for channel in 0..3 {
                let color = match fluid_row {
                    None => body[channel] * alpha,
                    Some(values) => {
                        (body[channel] * solid + values[column * 3 + channel] * (1.0 - solid))
                            * alpha
                    }
                };
                bottom[channel] = bottom[channel] * keep + color;
            }
            bottom[3] = bottom[3] * keep + alpha;
        }
    }
}

/// The 1-pixel line around the settled body.
#[allow(clippy::too_many_arguments)]
fn paint_ring(
    canvas: &mut Array3<f32>,
    scene: &Scene<'_>,
    coverage: &Array2<f32>,
    radius: f64,
    solid: f64,
    alpha_k: f64,
    x0: i32,
    y0: i32,
) {
    let (h, w) = coverage.dim();
    let inset = scene.ui.scale().max(1.0);
    let inner = rounded_coverage(w as i32, h as i32, radius, inset);
    let theme = scene.ui.theme_name();
    let line = card_palette(&theme)
        .get("LINE")
        .copied()
        .map(|color| color.channels())
        .unwrap_or([255, 255, 255]);
    let line_alpha = card_palette(&theme)
        .get("LINE")
        .and_then(|color| color.alpha())
        .map_or(1.0, |value| f64::from(value) / 255.0);
    let line_rgb = line.map(|value| f64::from(value) / 255.0);
    let factor = line_alpha * 2.2 * solid * alpha_k;
    let band = (radius + inset).ceil() as i32 + 2;
    ring_band(canvas, coverage, &inner, line_rgb, factor, x0, y0, band);
}

/// `_composite_frame`: only the `band`-pixel border gets the ring.
#[allow(clippy::too_many_arguments)]
fn ring_band(
    canvas: &mut Array3<f32>,
    coverage: &Array2<f32>,
    inner: &Array2<f32>,
    line: [f64; 3],
    factor: f64,
    x0: i32,
    y0: i32,
    band: i32,
) {
    let (h, w) = coverage.dim();
    let band = band.max(1).min(h as i32).min(w as i32) as usize;
    if band * 2 >= h || band * 2 >= w {
        ring_rows(canvas, coverage, inner, line, factor, x0, y0, 0, w, 0, h);
        return;
    }
    for rows in [0..band, h - band..h] {
        ring_rows(
            canvas, coverage, inner, line, factor, x0, y0, 0, w, rows.start, rows.end,
        );
    }
    let middle = band..h - band;
    for columns in [0..band, w - band..w] {
        ring_rows(
            canvas,
            coverage,
            inner,
            line,
            factor,
            x0,
            y0,
            columns.start,
            columns.end,
            middle.start,
            middle.end,
        );
    }
}

/// Ring pixels of one rectangle of `coverage`.
#[allow(clippy::too_many_arguments)]
fn ring_rows(
    canvas: &mut Array3<f32>,
    coverage: &Array2<f32>,
    inner: &Array2<f32>,
    line: [f64; 3],
    factor: f64,
    x0: i32,
    y0: i32,
    x_from: usize,
    x_to: usize,
    y_from: usize,
    y_to: usize,
) {
    let (_, w) = coverage.dim();
    let Some(clip) = span(canvas, x0, y0, x_from, x_to, y_from, y_to) else {
        return;
    };
    let canvas_w = canvas_width(canvas);
    let cover = coverage.as_slice().expect("coverage is contiguous");
    let hollow = inner.as_slice().expect("coverage is contiguous");
    let target = points_mut(canvas);
    for row in 0..clip.rows {
        let source_row = clip.src_row + row;
        let source_column = clip.src_column;
        let start = ((clip.dst_row + row) * canvas_w + clip.dst_column) * 4;
        let under = &mut target[start..start + clip.columns * 4];
        for (column, bottom) in under.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let index = source_row * w + source_column + column;
            let ring = (cover[index] - hollow[index]).clamp(0.0, 1.0);
            let alpha = f64::from(ring) * factor;
            let cover_alpha = alpha as f32;
            let keep = 1.0 - cover_alpha;
            for channel in 0..3 {
                bottom[channel] = bottom[channel] * keep + (line[channel] * alpha) as f32;
            }
            bottom[3] = bottom[3] * keep + cover_alpha;
        }
    }
}

/// `_shadow_under`: one soft shadow that only shows around the border (DX-1g).
///
/// DECISION: the three concentric bands added up and looked like several
/// stacked shadows. The design's soft falloff (`rounded_shadow`) is used in a
/// single pass that reaches as far as the three bands did; the peak keeps the
/// alpha the three passes left together, so the darkest edge does not change.
fn shadow_under(canvas: &mut Array3<f32>, rect: Rect, radius: f64, k: f64, scale: f64) {
    let k = k * 0.55;
    if k <= 0.01 {
        return;
    }
    let x0 = round_i32(rect.0);
    let y0 = round_i32(rect.1);
    let width = round_i32(rect.2) - x0;
    let height = round_i32(rect.3) - y0;
    if width < 2 || height < 2 {
        return;
    }
    let reach = round_i32(MARGIN * scale * 0.8).max(1);
    let shift = round_i32(scale * 5.0);
    let shadow = rounded_shadow(width, height, radius, reach);
    let peak = 1.0 - (1.0 - k * 0.10).powi(3);
    let (rows, columns) = shadow.dim();
    shade_rows(
        canvas,
        &shadow,
        0,
        columns,
        0,
        rows,
        x0 - reach,
        y0 - reach + shift,
        peak as f32,
    );
}
