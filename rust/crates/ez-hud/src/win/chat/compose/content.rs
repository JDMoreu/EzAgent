//! Content layer of the chat panel: its fingerprint, its cache and its paint.
//!
//! DX-1f: split from `compose.rs` so every module stays under the line limit.
//! The layer has its own lifecycle: laying it out costs O(history) and it is
//! the only part of the frame that must not run during the morph.

use ndarray::Array3;

use crate::geometry::{rounded_coverage, Rect};
use crate::model::chat::view::ChatView;

use super::raster::{composite_caret, composite_masked, Skip};
use super::{round_i32, Scene};

/// Fingerprint of the content layer: what the painted messages depend on.
///
/// DECISION: the content and the animation are two different things (DX-1,
/// paso 4). The blink of the caret and the tick of the orb used to be part of
/// this fingerprint, so the whole history was repainted twice a second (and
/// thirty times a second while the model was answering).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContentSig {
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
    theme: String,
    width: i32,
    height: i32,
}

/// `_sig`: the view state the painted content depends on.
pub(crate) fn content_signature(view: &ChatView) -> ContentSig {
    ContentSig {
        // DECISION: the version is read on its own (`ChatModel::version`); with
        // `snapshot()` the whole history was copied twice per frame just to
        // read this number (DX-1, paso 6).
        version: view.model.version(),
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
        theme: view.g.theme.clone(),
        width: view.g.width.round_ties_even() as i32,
        height: view.g.height.round_ties_even() as i32,
    }
}

/// Fingerprint and pixels of the cached content layer (`_cache` in Python).
#[derive(Default)]
pub struct ContentCache(pub(super) Option<(ContentSig, ez_gfx::sdf::Layer)>);

impl ContentCache {
    /// Whether the cached layer is the one of this signature.
    pub(crate) fn matches(&self, sig: &ContentSig) -> bool {
        self.0.as_ref().is_some_and(|(cached, _)| cached == sig)
    }

    /// Stores the layer of a signature.
    pub(crate) fn store(&mut self, sig: ContentSig, layer: ez_gfx::sdf::Layer) {
        self.0 = Some((sig, layer));
    }
}

/// `_paint_content`: the view layer, clipped by coverage and the row mask.
///
/// `build` is false during the morph (DX-1f): a missing layer is not laid out,
/// only a cached one is painted.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_content(
    canvas: &mut Array3<f32>,
    scene: &mut Scene<'_>,
    rect: Rect,
    radius: f64,
    mask: &[f32],
    lift: i32,
    content: &mut ContentCache,
    reuse: bool,
    build: bool,
) {
    let sig = content_signature(scene.view);
    // Al cerrar, la capa de contenido es la que dejó el panel abierto (solo
    // cambia la geometría): recalcularla con un historial largo costaba más que
    // los 0,20 s del cierre y se comía todos sus cuadros intermedios.
    let miss = content.0.as_ref().is_none_or(|(cached, _)| cached != &sig);
    if miss {
        if build && !(reuse && content.0.is_some()) {
            content.0 = Some((sig, scene.view.render_content(scene.now, false)));
        } else if !build && content.0.is_none() {
            // DX-1f: durante el morph no hay capa que pintar y no se maqueta.
            return;
        }
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
    // El cursor se pinta aparte: es lo unico que cambia dos veces por segundo
    // y no puede invalidar la capa de contenido (DX-1, paso 4).
    let caret = scene.view.caret_patch(scene.now);
    let skip = caret.as_ref().map(|(cx, cy, patch)| Skip {
        x0: (*cx).max(0) as usize,
        y0: (*cy).max(0) as usize,
        x1: (*cx).max(0) as usize + patch.width() as usize,
        y1: (*cy).max(0) as usize + patch.height() as usize,
    });
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
        skip,
    );
    if let Some((cx, cy, patch)) = caret {
        composite_caret(
            canvas,
            raw,
            stride,
            columns,
            rows,
            patch.as_raw(),
            patch.width() as usize,
            patch.height() as usize,
            cx.max(0) as usize,
            cy.max(0) as usize,
            &coverage,
            mask,
            x0,
            y0 + lift,
        );
    }
}
