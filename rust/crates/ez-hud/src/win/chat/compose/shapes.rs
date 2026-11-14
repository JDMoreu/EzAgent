//! Shape, body, ring and shadow of the chat panel (`chat_paint.py`).

use ndarray::{Array2, Array3};

use crate::geometry::{lerp, lerp_rect, rounded_coverage, smoothstep, Rect, MARGIN, PANEL_RADIUS};
use crate::theme::card_palette;

use super::raster::{composite_frame, composite_over, shade_frame};
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
        paint_ring(
            canvas, scene, &coverage, radius, solid, alpha_k, x0, y0, w, h,
        );
    }
}

/// Body colour or fluid, premultiplied by coverage and alpha.
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
    let mut rgb = Array3::<f32>::zeros((h, w, 3));
    for row in 0..h {
        for column in 0..w {
            for channel in 0..3 {
                let base = fluid.map_or(body[channel], |values| values[[row, column, channel]]);
                rgb[[row, column, channel]] =
                    body[channel] * solid as f32 + base * (1.0 - solid as f32);
            }
        }
    }
    let alpha = coverage.mapv(|value| value * alpha_k as f32);
    if fluid.is_none() {
        let mut source = Array3::<f32>::zeros((h, w, 3));
        for row in 0..h {
            for column in 0..w {
                for channel in 0..3 {
                    source[[row, column, channel]] = body[channel] * alpha[[row, column]];
                }
            }
        }
        composite_over(canvas, &source.view(), &alpha.view(), x0, y0);
    } else {
        for row in 0..h {
            for column in 0..w {
                for channel in 0..3 {
                    rgb[[row, column, channel]] *= alpha[[row, column]];
                }
            }
        }
        composite_over(canvas, &rgb.view(), &alpha.view(), x0, y0);
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
    w: usize,
    h: usize,
) {
    let width = w as i32;
    let height = h as i32;
    let inner = rounded_coverage(width, height, radius, scene.ui.scale().max(1.0));
    let ring = Array2::from_shape_fn((h, w), |(row, column)| {
        (coverage[[row, column]] - inner[[row, column]]).clamp(0.0, 1.0)
    });
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
    let mut source = Array3::<f32>::zeros((h, w, 4));
    for row in 0..h {
        for column in 0..w {
            let alpha = f64::from(ring[[row, column]]) * line_alpha * 2.2 * solid * alpha_k;
            for channel in 0..3 {
                source[[row, column, channel]] = (line_rgb[channel] * alpha) as f32;
            }
            source[[row, column, 3]] = alpha as f32;
        }
    }
    let band = (radius + scene.ui.scale().max(1.0)).ceil() as i32 + 2;
    composite_frame(canvas, &source.view(), x0, y0, band);
}

/// `_shadow_under`: three stacked rounded shadows that only show at the border.
fn shadow_under(canvas: &mut Array3<f32>, rect: Rect, radius: f64, k: f64, scale: f64) {
    let k = k * 0.55;
    if k <= 0.01 {
        return;
    }
    let x0 = round_i32(rect.0);
    let y0 = round_i32(rect.1);
    let width = round_i32(rect.2) - x0;
    let height = round_i32(rect.3) - y0;
    let grow = round_i32(MARGIN * scale * 0.8).max(0);
    let shift = round_i32(scale * 5.0);
    for step in 0..3 {
        let extra = grow * (step + 1) / 3;
        let coverage = rounded_coverage(
            width + 2 * extra,
            height + 2 * extra,
            radius + f64::from(extra),
            0.0,
        );
        let band = (0.3 * (radius + f64::from(extra))) as i32 + extra + shift + 2;
        shade_frame(
            canvas,
            &coverage.mapv(|value| value * (k * 0.10) as f32).view(),
            x0 - extra,
            y0 - extra + shift,
            band,
        );
    }
}
