//! Port of `src/ezagent/voice/hud/chat_geometry.py` (F2-12).
//!
//! Geometría y curvas del chat: tamaño del panel, a dónde crece, bordes para
//! redimensionar y easing. Todo es puro (sin Win32 ni dibujo).

use std::sync::{Arc, LazyLock};

use ndarray::Array2;
use parking_lot::Mutex;

pub const PANEL_W: f64 = 440.0;
pub const PANEL_H: f64 = 640.0;
pub const MIN_W: f64 = 340.0;
pub const MIN_H: f64 = 380.0;
/// Aire alrededor del panel para la sombra (px lógicos).
pub const MARGIN: f64 = 16.0;
/// Separación mínima al borde de la pantalla.
pub const SCREEN_PAD: f64 = 12.0;
pub const PANEL_RADIUS: f64 = 22.0;
/// Grosor de la zona de agarre de los bordes (px lógicos).
pub const GRIP: f64 = 8.0;
/// DECISION: keep the morph readable while shortening the delay before interacting.
pub const MORPH_SECS: f64 = 0.28;
/// Con movimiento reducido.
pub const FADE_SECS: f64 = 0.12;

/// `Rect` del chat: `(x0, y0, x1, y1)` físicos.
pub type Rect = (f64, f64, f64, f64);
/// `(izquierda, arriba, derecha, abajo)`.
pub type Edges = (bool, bool, bool, bool);
/// Área de trabajo de Windows: `(left, top, right, bottom)`.
pub type WorkArea = (i32, i32, i32, i32);

/// Cursores de Windows (`IDC_*`) para cada borde.
pub const IDC_SIZEWE: i32 = 32644;
pub const IDC_SIZENS: i32 = 32645;
pub const IDC_SIZENWSE: i32 = 32642;
pub const IDC_SIZENESW: i32 = 32643;

// ------------------------------------------------------------------- curvas
pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Aceleración y frenado suaves (cúbica).
pub fn ease(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x.powf(3.0)
    } else {
        1.0 - (-2.0 * x + 2.0).powf(3.0) / 2.0
    }
}

/// Llega, se pasa un pelín (~3 %) y se asienta.
pub fn ease_out_back(x: f64) -> f64 {
    ease_out_back_with(x, 0.75)
}

/// `ease_out_back` con un rebote explícito (el `overshoot` del Python).
pub fn ease_out_back_with(x: f64, overshoot: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let c3 = overshoot + 1.0;
    1.0 + c3 * (x - 1.0).powf(3.0) + overshoot * (x - 1.0).powf(2.0)
}

pub fn lerp(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a) * k
}

pub fn lerp_rect(a: Rect, b: Rect, k: f64) -> Rect {
    (
        lerp(a.0, b.0, k),
        lerp(a.1, b.1, k),
        lerp(a.2, b.2, k),
        lerp(a.3, b.3, k),
    )
}

// ----------------------------------------------------------------- tamaño
/// Tamaño lógico máximo del panel: toda la pantalla menos la separación a los bordes.
pub fn panel_limits(work: WorkArea, scale: f64) -> (f64, f64) {
    (
        MIN_W.max((work.2 - work.0) as f64 / scale - 2.0 * SCREEN_PAD),
        MIN_H.max((work.3 - work.1) as f64 / scale - 2.0 * SCREEN_PAD),
    )
}

/// Tamaño lógico del panel: el recordado o el de fábrica, siempre entre el
/// mínimo y la pantalla.
pub fn panel_size(work: WorkArea, scale: f64, saved: Option<(f64, f64)>) -> (f64, f64) {
    let (max_w, max_h) = panel_limits(work, scale);
    let (want_w, want_h) = saved.unwrap_or((PANEL_W, PANEL_H));
    (MIN_W.max(want_w.min(max_w)), MIN_H.max(want_h.min(max_h)))
}

/// Rectángulo físico del panel: crece desde la forma hacia donde haya más espacio.
///
/// Centrado en la forma y pegado a ella por el lado que mira al centro de la
/// pantalla (una barra abajo despliega hacia arriba, una arriba hacia abajo);
/// siempre dentro de la pantalla.
pub fn panel_rect(form: Rect, work: WorkArea, scale: f64, saved: Option<(f64, f64)>) -> Rect {
    let (logical_w, logical_h) = panel_size(work, scale, saved);
    let (panel_w, panel_h) = (logical_w * scale, logical_h * scale);
    let pad = SCREEN_PAD * scale;
    let center_x = (form.0 + form.2) / 2.0;
    let x0 = (center_x - panel_w / 2.0)
        .min(work.2 as f64 - pad - panel_w)
        .max(work.0 as f64 + pad);
    let middle = (work.1 as f64 + work.3 as f64) / 2.0;
    let y0 = if (form.1 + form.3) / 2.0 > middle {
        let y1 = form.3.min(work.3 as f64 - pad);
        (y1 - panel_h).max(work.1 as f64 + pad)
    } else {
        let y0 = form.1.max(work.1 as f64 + pad);
        let y1 = (y0 + panel_h).min(work.3 as f64 - pad);
        y1 - panel_h
    };
    (x0, y0, x0 + panel_w, y0 + panel_h)
}

// --------------------------------------------------------------- coberturas
/// `round(x, 3)`: la caché del Python redondea el radio antes de usarlo.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round_ties_even() / 1000.0
}

fn abs_axis(len: usize, half: f64) -> Vec<f32> {
    let half = half as f32;
    (0..len)
        .map(|index| (index as f32 + 0.5 - half).abs())
        .collect()
}

fn shifted(values: &[f32], threshold: f64) -> Vec<f32> {
    let threshold = threshold as f32;
    values
        .iter()
        .map(|value| (*value - threshold).max(0.0))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    w: i32,
    h: i32,
    radius: u64,
    inset: u64,
}

type Cache = Mutex<Vec<(CacheKey, Arc<Array2<f32>>)>>;

fn cached(
    cache: &Cache,
    key: CacheKey,
    max: usize,
    build: impl FnOnce() -> Array2<f32>,
) -> Arc<Array2<f32>> {
    {
        let entries = cache.lock();
        if let Some((_, array)) = entries.iter().find(|(other, _)| *other == key) {
            return Arc::clone(array);
        }
    }
    let array = Arc::new(build());
    let mut entries = cache.lock();
    if entries.len() >= max {
        entries.remove(0);
    }
    entries.push((key, Arc::clone(&array)));
    array
}

/// Cobertura (0..1) suave de un rectángulo redondeado `w`×`h` (distancia con signo).
///
/// DECISION: el Python comparte el array con `lru_cache`; aquí la caché es un
/// `Arc` con tope FIFO de 8 entradas (la forma y el radio cambian poco) y el
/// resultado también es de solo lectura.
pub fn rounded_coverage(w: i32, h: i32, radius: f64, inset: f64) -> Arc<Array2<f32>> {
    static CACHE: LazyLock<Cache> = LazyLock::new(|| Mutex::new(Vec::new()));
    let radius = round3(radius);
    let inset = round3(inset);
    let key = CacheKey {
        w,
        h,
        radius: radius.to_bits(),
        inset: inset.to_bits(),
    };
    cached(&CACHE, key, 8, || coverage_array(w, h, radius, inset))
}

fn coverage_array(w: i32, h: i32, radius: f64, inset: f64) -> Array2<f32> {
    let w = w.max(2) as usize;
    let h = h.max(2) as usize;
    let limit = w.min(h) as f64 / 2.0;
    let r = radius.min(limit).max(0.0) - inset;
    if r <= 0.0 {
        return coverage_without_corners(w, h, inset, r);
    }
    let (half_w, half_h) = (w as f64 / 2.0, h as f64 / 2.0);
    let xs = abs_axis(w, half_w);
    let ys = abs_axis(h, half_h);
    let sx = shifted(&xs, half_w - inset - r);
    let sy = shifted(&ys, half_h - inset - r);
    let mut out = Array2::<f32>::ones((h, w));
    // DECISION: se acota el ancho de la franja a [0, min(h, w)]; el Python usa
    // el valor tal cual como índice de `slice`, que con un inset absurdo
    // (más negativo que el radio) recorta filas por accidente.
    let band = ((r + inset).ceil() as i64 + 1).clamp(0, w.min(h) as i64) as usize;
    let radius = r as f32;
    let paint = |row: usize, column: usize, out: &mut Array2<f32>| {
        let distance = (sx[column] * sx[column] + sy[row] * sy[row]).sqrt() - radius;
        out[[row, column]] = (0.5 - distance).clamp(0.0, 1.0);
    };
    for row in (0..band).chain(h - band..h) {
        for column in 0..w {
            paint(row, column, &mut out);
        }
    }
    if band * 2 < h {
        for row in band..h - band {
            for column in (0..band).chain(w - band..w) {
                paint(row, column, &mut out);
            }
        }
    }
    out
}

/// Caso general sin esquinas (`r <= 0`): raro y pequeño.
fn coverage_without_corners(w: usize, h: usize, inset: f64, r: f64) -> Array2<f32> {
    let (half_w, half_h) = (w as f64 / 2.0, h as f64 / 2.0);
    let xs = abs_axis(w, half_w);
    let ys = abs_axis(h, half_h);
    let sx = shifted(&xs, half_w - inset - r);
    let sy = shifted(&ys, half_h - inset - r);
    let radius = r as f32;
    let mut out = Array2::<f32>::zeros((h, w));
    for row in 0..h {
        for column in 0..w {
            let distance = (sx[column] * sx[column] + sy[row] * sy[row]).sqrt() - radius;
            out[[row, column]] = (0.5 - distance).clamp(0.0, 1.0);
        }
    }
    out
}

/// Sombra suave (0..1) de un rectángulo redondeado `w`×`h`, de
/// `(h+2·reach)×(w+2·reach)`.
///
/// Vale 1 bajo el rectángulo y cae sin escalones hasta 0 a `reach` píxeles de
/// su borde. Una sola pasada.
pub fn rounded_shadow(w: i32, h: i32, radius: f64, reach: i32) -> Arc<Array2<f32>> {
    static CACHE: LazyLock<Cache> = LazyLock::new(|| Mutex::new(Vec::new()));
    let radius = round3(radius);
    let reach = reach.max(1);
    let key = CacheKey {
        w,
        h,
        radius: radius.to_bits(),
        inset: reach as u64,
    };
    cached(&CACHE, key, 4, || shadow_array(w, h, radius, reach))
}

fn shadow_array(w: i32, h: i32, radius: f64, reach: i32) -> Array2<f32> {
    let w = w.max(2) as usize;
    let h = h.max(2) as usize;
    let r = radius.min(w.min(h) as f64 / 2.0).max(0.0);
    let reach_usize = reach as usize;
    let (wide, tall) = (w + 2 * reach_usize, h + 2 * reach_usize);
    let xs = abs_axis(wide, wide as f64 / 2.0);
    let ys = abs_axis(tall, tall as f64 / 2.0);
    let dx = shifted(&xs, w as f64 / 2.0 - r);
    let dy = shifted(&ys, h as f64 / 2.0 - r);
    let radius = r as f32;
    let reach = reach as f32;
    let mut out = Array2::<f32>::zeros((tall, wide));
    for row in 0..tall {
        for column in 0..wide {
            let distance = (dx[column] * dx[column] + dy[row] * dy[row]).sqrt() - radius;
            let fall = (1.0 - distance / reach).clamp(0.0, 1.0);
            out[[row, column]] = fall * fall;
        }
    }
    out
}

// ------------------------------------------------------------ redimensionar
/// Bordes que agarra el punto `(x, y)` de un panel `w`×`h` (`None` si no toca
/// ninguno). Las esquinas agarran dos bordes a la vez.
pub fn edges_at(x: f64, y: f64, w: f64, h: f64) -> Option<Edges> {
    edges_at_grip(x, y, w, h, GRIP)
}

/// `edges_at` con un grosor de agarre explícito.
pub fn edges_at_grip(x: f64, y: f64, w: f64, h: f64, grip: f64) -> Option<Edges> {
    if !(-1.0 <= x && x <= w + 1.0 && -1.0 <= y && y <= h + 1.0) {
        return None;
    }
    let edges = (x <= grip, y <= grip, x >= w - grip, y >= h - grip);
    if edges.0 || edges.1 || edges.2 || edges.3 {
        Some(edges)
    } else {
        None
    }
}

/// Cursor de Windows para esos bordes (flechas horizontal, vertical o diagonal).
pub fn edge_cursor(edges: Edges) -> i32 {
    let (left, top, right, bottom) = edges;
    if (left || right) && (top || bottom) {
        if (left && top) || (right && bottom) {
            IDC_SIZENWSE
        } else {
            IDC_SIZENESW
        }
    } else if left || right {
        IDC_SIZEWE
    } else {
        IDC_SIZENS
    }
}

/// Rectángulo físico del panel tras arrastrar `edges` `(dx, dy)` píxeles.
///
/// El borde opuesto queda fijo; el tamaño se acota entre el mínimo y la
/// pantalla (con su separación a los bordes), y nunca se sale del área de
/// trabajo.
pub fn resize_rect(edges: Edges, rect: Rect, dx: f64, dy: f64, work: WorkArea, scale: f64) -> Rect {
    let (left, top, right, bottom) = edges;
    let (mut x0, mut y0, mut x1, mut y1) = rect;
    let pad = SCREEN_PAD * scale;
    let (min_w, min_h) = (MIN_W * scale, MIN_H * scale);
    let (lo_x, hi_x) = (work.0 as f64 + pad, work.2 as f64 - pad);
    let (lo_y, hi_y) = (work.1 as f64 + pad, work.3 as f64 - pad);
    if left {
        x0 = (x0 + dx).min(x1 - min_w).max(lo_x);
    }
    if right {
        x1 = (x1 + dx).max(x0 + min_w).min(hi_x);
    }
    if top {
        y0 = (y0 + dy).min(y1 - min_h).max(lo_y);
    }
    if bottom {
        y1 = (y1 + dy).max(y0 + min_h).min(hi_y);
    }
    (x0, y0, x1, y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: WorkArea = (0, 0, 1920, 1040);

    #[test]
    fn curves_are_monotonic_and_clamped() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.5), 0.5);
        assert_eq!(smoothstep(2.0), 1.0);
        assert_eq!(ease(0.25), 0.0625);
        assert!((ease(0.75) - 0.9375).abs() < 1e-12);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease_out_back(0.0), 0.0);
        assert_eq!(ease_out_back(1.0), 1.0);
        assert!(ease_out_back(0.8) > 1.0);
        assert_eq!(ease_out_back_with(0.8, 0.75), ease_out_back(0.8));
        assert_eq!(lerp(10.0, 20.0, 0.25), 12.5);
        assert_eq!(
            lerp_rect((0.0, 0.0, 10.0, 10.0), (10.0, 20.0, 30.0, 40.0), 0.5),
            (5.0, 10.0, 20.0, 25.0)
        );
    }

    #[test]
    fn panel_size_respects_the_saved_value_and_the_screen() {
        assert_eq!(panel_size(WORK, 1.0, None), (PANEL_W, PANEL_H));
        assert_eq!(panel_size(WORK, 1.0, Some((500.0, 700.0))), (500.0, 700.0));
        assert_eq!(panel_size(WORK, 1.0, Some((10.0, 10.0))), (MIN_W, MIN_H));
        let (max_w, max_h) = panel_limits(WORK, 1.0);
        assert_eq!(
            panel_size(WORK, 1.0, Some((9999.0, 9999.0))),
            (max_w, max_h)
        );
        assert!(panel_size((0, 0, 800, 600), 1.0, Some((9999.0, 9999.0))).1 <= 576.0);
    }

    #[test]
    fn panel_rect_grows_toward_the_free_side() {
        let bottom = panel_rect((800.0, 1000.0, 1000.0, 1030.0), WORK, 1.0, None);
        assert_eq!(bottom.3, 1028.0);
        assert_eq!(bottom.1, 1028.0 - PANEL_H);
        let top = panel_rect((800.0, 0.0, 1000.0, 30.0), WORK, 1.0, None);
        assert_eq!(top.1, 12.0);
        assert_eq!(top.3, 12.0 + PANEL_H);
        let wide = panel_rect((0.0, 0.0, 4.0, 30.0), WORK, 1.0, None);
        assert_eq!(wide.0, 12.0);
        assert_eq!(wide.2, 12.0 + PANEL_W);
    }

    #[test]
    fn edges_and_cursors_match_windows_ids() {
        let (w, h) = (400.0, 600.0);
        assert_eq!(
            edges_at(2.0, 300.0, w, h),
            Some((true, false, false, false))
        );
        assert_eq!(
            edges_at(398.0, 300.0, w, h),
            Some((false, false, true, false))
        );
        assert_eq!(
            edges_at(200.0, 598.0, w, h),
            Some((false, false, false, true))
        );
        assert_eq!(edges_at(2.0, 2.0, w, h), Some((true, true, false, false)));
        assert_eq!(edges_at(200.0, 300.0, w, h), None);
        assert_eq!(edges_at(-30.0, 300.0, w, h), None);
        assert_eq!(edges_at(200.0, 900.0, w, h), None);
        assert_eq!(edge_cursor((true, false, false, false)), IDC_SIZEWE);
        assert_eq!(edge_cursor((false, true, false, false)), IDC_SIZENS);
        assert_eq!(edge_cursor((true, true, false, false)), IDC_SIZENWSE);
        assert_eq!(edge_cursor((false, false, true, true)), IDC_SIZENWSE);
        assert_eq!(edge_cursor((true, false, false, true)), IDC_SIZENESW);
    }

    #[test]
    fn resizing_pins_the_opposite_edge_and_clamps() {
        let rect = (100.0, 100.0, 500.0, 500.0);
        let right = resize_rect((false, false, true, false), rect, 50.0, 99.0, WORK, 1.0);
        assert_eq!(right, (100.0, 100.0, 550.0, 500.0));
        let left = resize_rect((true, false, false, false), rect, -80.0, 0.0, WORK, 1.0);
        assert_eq!(left, (20.0, 100.0, 500.0, 500.0));
        let tiny = resize_rect((true, true, false, false), rect, 9999.0, 9999.0, WORK, 1.0);
        assert_eq!(tiny.2 - tiny.0, MIN_W);
        assert_eq!(tiny.3 - tiny.1, MIN_H);
        let wide = resize_rect((true, false, false, false), rect, -9999.0, 0.0, WORK, 1.0);
        assert_eq!(wide.0, SCREEN_PAD);
        let huge = resize_rect((true, true, true, true), rect, 9999.0, 9999.0, WORK, 1.0);
        assert!(huge.2 <= WORK.2 as f64 - SCREEN_PAD && huge.3 <= WORK.3 as f64 - SCREEN_PAD);
        let scaled = resize_rect((false, false, true, false), rect, -9999.0, 0.0, WORK, 1.5);
        assert!((scaled.2 - scaled.0 - MIN_W * 1.5).abs() < 1e-9);
    }

    #[test]
    fn coverage_and_shadow_are_soft_at_the_border() {
        let coverage = rounded_coverage(40, 30, 10.0, 0.0);
        assert_eq!(coverage.dim(), (30, 40));
        assert!((coverage[[15, 20]] - 1.0).abs() < 1e-6);
        assert!(coverage[[0, 0]] < 0.01);
        assert!(coverage[[0, 20]] > 0.99);
        let cropped = rounded_coverage(10, 10, -4.0, 0.0);
        assert_eq!(cropped.dim(), (10, 10));
        let shadow = rounded_shadow(40, 30, 10.0, 6);
        assert_eq!(shadow.dim(), (42, 52));
        assert!((shadow[[21, 26]] - 1.0).abs() < 1e-6);
        assert!(shadow[[0, 0]] < 0.01);
    }
}
