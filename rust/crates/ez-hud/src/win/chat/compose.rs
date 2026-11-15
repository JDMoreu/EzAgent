//! Port of `src/ezagent/voice/hud/chat_paint.py` (F5-15).
//!
//! Composition of the chat panel: the rounded shape morphs out of the bar,
//! the content appears row by row, and the live orb and status line sit on top.
//! Everything is painted in the window canvas in logical pixels and scaled at
//! the physical size, like the rest of the HUD.
//!
//! The Python let numpy broadcast each pass over the whole canvas; here every
//! pass walks contiguous rows and premultiplies as it writes, so a morph frame
//! allocates nothing but the frame itself: no full-size temporaries, no
//! re-rasterized content, and the canvas is borrowed from a thread pool
//! (`MORPH_SECS = 0.28` leaves room for ~17 frames at 60 fps).

use ez_gfx::raster::Image;
use ez_gfx::resample::Resampling;
use ndarray::Array3;

use crate::geometry::{ease_out_back_with, rounded_coverage, smoothstep, Rect, MARGIN};
use crate::model::chat::header::HEADER_H;
use crate::model::chat::view::ChatView;

use super::window::{AnimState, ChatUi};

mod raster;
mod shapes;
#[cfg(test)]
mod tests;

pub use raster::Frame;
use raster::{blend, composite, composite_masked, frame_from_canvas};
use shapes::{content_lift, content_mask, paint_shape, shape_geometry};

/// Everything that composes one panel frame.
pub struct Scene<'a> {
    /// Bar state the panel reads.
    pub ui: &'a dyn ChatUi,
    /// Chat view with its model, parser and input.
    pub view: &'a mut ChatView,
    /// Lifecycle state of the morph.
    pub anim: AnimState,
    /// Bar shape on screen (physical pixels).
    pub form: Rect,
    /// Smoothed orb activity, 0 to 1.
    pub act: f64,
    /// Instant of the frame.
    pub now: f64,
}

/// Physical rectangle of the window that owns the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRect {
    /// Left edge on screen.
    pub x: i32,
    /// Top edge on screen.
    pub y: i32,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

/// Fingerprint of everything the content cache depends on (`_sig`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sig {
    version: i64,
    scroll_tenths: i64,
    hover: Option<String>,
    input_text: String,
    input_cursor: usize,
    input_selection: Option<(usize, usize)>,
    mode: &'static str,
    in_call: bool,
    pending_delete: Option<i64>,
    status: String,
    flash: (String, bool),
    blink: i64,
    anim_tick: i64,
    theme: String,
    width: i32,
    height: i32,
    window_width: usize,
    window_height: usize,
    scale_bits: u64,
}

/// `_sig`: the view state the painted content depends on.
pub(crate) fn signature(view: &mut ChatView, now: f64, ticking: bool) -> Sig {
    let animating = ticking && view.animating(now);
    // DECISION: Python reads `view._focus_t0` for the caret blink; F5-14 keeps
    // it private, so the phase is derived from the clock instead.
    let blink = if ticking { (now * 2.0) as i64 % 2 } else { 0 };
    Sig {
        version: view.model.snapshot().0,
        scroll_tenths: (view.scroll.offset() * 10.0).round_ties_even() as i64,
        hover: view.hover.clone(),
        input_text: view.input.text().to_string(),
        input_cursor: view.input.cursor(),
        input_selection: view.input.selection(),
        mode: view.mode.as_str(),
        in_call: view.in_call,
        pending_delete: view.pending_delete(),
        status: view.status.clone(),
        flash: view.flash_token(),
        blink,
        anim_tick: if animating { (now * 30.0) as i64 } else { 0 },
        theme: view.g.theme.clone(),
        width: view.g.width.round_ties_even() as i32,
        height: view.g.height.round_ties_even() as i32,
        window_width: 0,
        window_height: 0,
        scale_bits: 0,
    }
}

/// `shape_progress`: on opening the shape overshoots about 2 % and settles.
pub fn shape_progress(p: f64, opening: bool) -> f64 {
    if !opening || p >= 1.0 {
        return p;
    }
    let weight = smoothstep((p - 0.45) / 0.35);
    p + (ease_out_back_with(p, 1.2) - p) * weight
}

/// `row_reveal`: how much of each content row is visible (0..1).
pub fn row_reveal(rows: usize, p: f64, opening: bool) -> Vec<f32> {
    let (base, span, stagger) = if opening {
        (0.46f32, 0.28f32, 0.22f32)
    } else {
        (0.52f32, 0.20f32, 0.20f32)
    };
    let rows = rows.max(1);
    let p = p as f32;
    (0..rows)
        .map(|row| {
            let fraction = if rows == 1 {
                0.0
            } else {
                row as f32 / (rows - 1) as f32
            };
            let x = ((p - (base + stagger * fraction)) / span).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        })
        .collect()
}

/// `fluid_array`: the coarse RGB fluid stretched to `width`×`height` in 0..1.
pub fn fluid_array(rgb: Option<&Array3<u8>>, width: usize, height: usize) -> Option<Array3<f32>> {
    let rgb = rgb?;
    let (rows, columns, channels) = rgb.dim();
    if rows == 0 || columns == 0 || channels != 3 {
        return None;
    }
    let data = match rgb.as_slice() {
        Some(pixels) => pixels.to_vec(),
        None => {
            let mut data = Vec::with_capacity(rows * columns * 3);
            for row in 0..rows {
                for column in 0..columns {
                    for channel in 0..3 {
                        data.push(rgb[[row, column, channel]]);
                    }
                }
            }
            data
        }
    };
    let image = Image::from_rgb(columns as u32, rows as u32, data).ok()?;
    let width = width.max(2) as u32;
    let height = height.max(2) as u32;
    let resized = image.resize(width, height, Resampling::Bicubic);
    let table = byte_scale();
    let raw = resized.as_raw();
    let mut values = Vec::with_capacity(height as usize * width as usize * 3);
    for pixel in raw.as_chunks::<4>().0 {
        for channel in 0..3 {
            values.push(table[usize::from(pixel[channel])]);
        }
    }
    Array3::from_shape_vec((height as usize, width as usize, 3), values).ok()
}

/// `channel / 255.0` for every byte, the same values the division yields.
fn byte_scale() -> &'static [f32; 256] {
    static TABLE: std::sync::LazyLock<[f32; 256]> =
        std::sync::LazyLock::new(|| core::array::from_fn(|value| value as f32 / 255.0));
    &TABLE
}

/// Fingerprint and pixels of the cached content layer (`_cache` in Python).
#[derive(Default)]
pub struct ContentCache(Option<(Sig, ez_gfx::sdf::Layer)>);

/// Scratch canvas borrowed from the thread pool.
///
/// The morph repaints a full window per frame; reusing the same 11 MB of
/// pages keeps the first touch of every pixel off the frame budget. The
/// canvas is always fully rewritten, so only the size has to match.
struct Canvas(Option<Array3<f32>>);

thread_local! {
    static CANVAS: std::cell::RefCell<Option<Array3<f32>>> = const { std::cell::RefCell::new(None) };
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        let mut buffer = CANVAS.with(|slot| slot.borrow_mut().take());
        match buffer.as_mut() {
            Some(canvas) if canvas.dim() == (height, width, 4) => canvas.fill(0.0),
            _ => buffer = Some(Array3::zeros((height, width, 4))),
        }
        Self(buffer)
    }
}

impl std::ops::Deref for Canvas {
    type Target = Array3<f32>;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("the canvas is always present")
    }
}

impl std::ops::DerefMut for Canvas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("the canvas is always present")
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        let Some(canvas) = self.0.take() else {
            return;
        };
        CANVAS.with(|slot| {
            let mut slot = slot.borrow_mut();
            let bigger = slot
                .as_ref()
                .is_none_or(|cached| cached.len() < canvas.len());
            if bigger {
                *slot = Some(canvas);
            }
        });
    }
}

/// `_compose` without a warm content cache: paints one frame from scratch.
pub fn compose_frame(scene: &mut Scene<'_>, p: f64, live: bool, window: WindowRect) -> Frame {
    let mut content = ContentCache::default();
    compose(scene, p, live, window, &mut content)
}

/// `_compose` using (and refreshing) the panel's content cache.
pub fn compose_cached(
    scene: &mut Scene<'_>,
    p: f64,
    live: bool,
    window: WindowRect,
    content: &mut ContentCache,
) -> Frame {
    compose(scene, p, live, window, content)
}

/// `_compose`: paints one frame of the panel (BGRA premultiplied).
pub(crate) fn compose(
    scene: &mut Scene<'_>,
    p: f64,
    live: bool,
    window: WindowRect,
    content: &mut ContentCache,
) -> Frame {
    let scale = scene.ui.scale();
    let margin = scale * MARGIN;
    let canvas_w = window.width.max(1);
    let canvas_h = window.height.max(1);
    let mut canvas = Canvas::new(canvas_w, canvas_h);
    let panel = panel_local(canvas_w, canvas_h, margin);
    let opening = scene.anim != AnimState::Closing;
    let form = form_local(scene.form, window);
    let reduced = scene.ui.reduced();
    let fly = form.is_some() && !reduced;
    let (rect, radius, solid, alpha, shadow) =
        shape_geometry(scene, p, opening, panel, scale, reduced, fly, form);
    paint_shape(&mut canvas, scene, rect, radius, solid, alpha, shadow);
    let x0 = round_i32(rect.0);
    let y0 = round_i32(rect.1);
    let rows = (round_i32(rect.3) - y0).max(1) as usize;
    let mask = content_mask(rows, p, opening, fly, reduced, alpha);
    if mask.iter().copied().fold(0.0f32, f32::max) > 0.01 {
        let lift = content_lift(&mask, fly, opening, scale);
        paint_content(
            &mut canvas,
            scene,
            rect,
            radius,
            &mask,
            lift,
            content,
            scene.anim == AnimState::Open,
        );
        if live {
            paint_live(
                &mut canvas,
                scene,
                x0,
                y0,
                round_i32(rect.2),
                rows,
                &mask,
                lift,
            );
        }
    }
    frame_from_canvas(&canvas)
}

/// Composits the live orb and status line over the content.
#[allow(clippy::too_many_arguments)]
fn paint_live(
    canvas: &mut Array3<f32>,
    scene: &mut Scene<'_>,
    x0: i32,
    y0: i32,
    x1: i32,
    rows: usize,
    mask: &[f32],
    lift: i32,
) {
    let layers = live_layers(
        scene,
        (x1 - x0).max(1) as usize,
        rows,
        scene.now,
        1.0,
        mask,
        Some(x0),
        Some(y0 + lift),
    );
    for (lx, ly, layer) in layers {
        composite(canvas, &layer, lx, ly);
    }
}

/// `_stable`: the still panel is cached and only the orb and status line move.
pub(crate) fn stable(
    scene: &mut Scene<'_>,
    base: &mut Option<(Sig, Frame)>,
    content: &mut ContentCache,
    window: WindowRect,
) -> Frame {
    let mut key = signature(scene.view, scene.now, true);
    key.window_width = window.width;
    key.window_height = window.height;
    key.scale_bits = scene.ui.scale().to_bits();
    if base.as_ref().is_none_or(|(cached, _)| cached != &key) {
        *base = Some((key, compose(scene, 1.0, false, window, content)));
    }
    let mut frame = base
        .as_ref()
        .map(|(_, frame)| frame.clone())
        .unwrap_or_else(|| Frame::new(window.width, window.height));
    let margin = scene.ui.scale() * MARGIN;
    let width = window
        .width
        .saturating_sub((2.0 * margin).round_ties_even() as usize);
    let layers = live_layers(
        scene,
        width.max(1),
        window.height,
        scene.now,
        1.0,
        &[1.0],
        None,
        None,
    );
    for (x0, y0, layer) in layers {
        blend(&mut frame, &layer.view(), x0, y0);
    }
    frame
}

fn panel_local(width: usize, height: usize, margin: f64) -> Rect {
    let margin = margin.round_ties_even();
    (
        margin,
        margin,
        width as f64 - margin,
        height as f64 - margin,
    )
}

fn form_local(form: Rect, window: WindowRect) -> Option<Rect> {
    let local = (
        form.0 - f64::from(window.x),
        form.1 - f64::from(window.y),
        form.2 - f64::from(window.x),
        form.3 - f64::from(window.y),
    );
    let cx = (local.0 + local.2) / 2.0;
    let cy = (local.1 + local.3) / 2.0;
    (0.0 <= cx && cx <= window.width as f64 && 0.0 <= cy && cy <= window.height as f64)
        .then_some(local)
}

/// `_paint_content`: the view layer, clipped by coverage and the row mask.
#[allow(clippy::too_many_arguments)]
fn paint_content(
    canvas: &mut Array3<f32>,
    scene: &mut Scene<'_>,
    rect: Rect,
    radius: f64,
    mask: &[f32],
    lift: i32,
    content: &mut ContentCache,
    ticking: bool,
) {
    let sig = signature(scene.view, scene.now, ticking);
    if content.0.as_ref().is_none_or(|(cached, _)| cached != &sig) {
        content.0 = Some((sig, scene.view.render(scene.now)));
    }
    let layer = match content.0.as_ref() {
        Some((_, layer)) => layer,
        None => return,
    };
    let x0 = round_i32(rect.0);
    let y0 = round_i32(rect.1);
    let width = round_i32(rect.2) - x0;
    let height = round_i32(rect.3) - y0;
    if width <= 0 || height <= 0 {
        return;
    }
    let (w, h) = (width as usize, height as usize);
    let coverage = rounded_coverage(width, height, radius, 0.0);
    let columns = w.min(layer.width() as usize);
    let rows = h.min(layer.height() as usize).min(mask.len());
    if columns == 0 || rows == 0 {
        return;
    }
    let raw = layer.as_raw();
    let stride = layer.width() as usize;
    composite_masked(
        canvas,
        raw,
        stride,
        &coverage,
        mask,
        columns,
        rows,
        x0,
        y0 + lift,
    );
}

/// `_live_layers`: premultiplied `(x, y, RGBA)` layers of the orb and status line.
#[allow(clippy::too_many_arguments)]
fn live_layers(
    scene: &mut Scene<'_>,
    width: usize,
    _height: usize,
    now: f64,
    gain: f64,
    mask: &[f32],
    origin_x: Option<i32>,
    origin_y: Option<i32>,
) -> Vec<(i32, i32, Array3<f32>)> {
    let Some(orb) = scene.view.orb else {
        return Vec::new();
    };
    let scale = scene.ui.scale();
    let margin = (MARGIN * scale).round_ties_even() as i32;
    let origin_x = origin_x.unwrap_or(margin);
    let origin_y = origin_y.unwrap_or(margin);
    let row = ((orb.1 * scale) as i32).clamp(0, mask.len() as i32 - 1) as usize;
    let k = f64::from(mask[row]) * gain;
    if k <= 0.01 {
        return Vec::new();
    }
    let diameter = ((orb.2 * scale).round_ties_even() as i32).max(6) as usize;
    let Some(fluid) = scene
        .ui
        .chat_fluid(diameter, diameter, now)
        .filter(|values| values.dim() == (diameter, diameter, 3))
    else {
        return Vec::new();
    };
    let alpha = rounded_coverage(diameter as i32, diameter as i32, diameter as f64 / 2.0, 0.0);
    let mut layer = Array3::<f32>::zeros((diameter, diameter, 4));
    for row in 0..diameter {
        for column in 0..diameter {
            let matte = alpha[[row, column]] * k as f32;
            for channel in 0..3 {
                layer[[row, column, channel]] = fluid[[row, column, channel]] * matte;
            }
            layer[[row, column, 3]] = matte;
        }
    }
    let mut out = vec![(
        (origin_x as f64 + orb.0 * scale - diameter as f64 / 2.0).round_ties_even() as i32,
        (origin_y as f64 + orb.1 * scale - diameter as f64 / 2.0).round_ties_even() as i32,
        layer,
    )];
    if let Some(line) = status_line(scene, width, now, k, origin_x, origin_y) {
        out.push(line);
    }
    out
}

/// `_status_line`: a thin wave under the header tinted with the fluid.
fn status_line(
    scene: &Scene<'_>,
    width: usize,
    now: f64,
    k: f64,
    origin_x: i32,
    origin_y: i32,
) -> Option<(i32, i32, Array3<f32>)> {
    let act = scene.act * k;
    if act <= 0.02 {
        return None;
    }
    let scale = scene.ui.scale();
    let pad = (16.0 * scale).round_ties_even() as i32;
    let line_width = (width as i32 - 2 * pad).max(2) as usize;
    let line_height = (2.0 * scale).round_ties_even().max(2.0) as usize;
    let fluid = scene
        .ui
        .chat_fluid(line_width, line_height, now)
        .filter(|values| values.dim() == (line_height, line_width, 3))?;
    let phase = if scene.ui.reduced() { 0.0 } else { now * 0.9 };
    let mut layer = Array3::<f32>::zeros((line_height, line_width, 4));
    for column in 0..line_width {
        let x = if line_width <= 1 {
            0.0
        } else {
            column as f64 / (line_width - 1) as f64
        };
        let wave = 0.5 + 0.5 * (2.0 * std::f64::consts::PI * (x * 1.6 - phase)).sin();
        let edge = 1.0f64.min(x.min(1.0 - x) * 12.0);
        let alpha = ((0.18 + 0.82 * wave * wave) * act * 0.9 * edge) as f32;
        for row in 0..line_height {
            for channel in 0..3 {
                layer[[row, column, channel]] = fluid[[row, column, channel]] * alpha;
            }
            layer[[row, column, 3]] = alpha;
        }
    }
    Some((
        origin_x + pad,
        origin_y + ((HEADER_H - 1.5) * scale).round_ties_even() as i32,
        layer,
    ))
}

pub(super) fn round_i32(value: f64) -> i32 {
    value.round_ties_even() as i32
}
