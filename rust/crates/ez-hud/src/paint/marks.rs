//! Port of `src/ezagent/voice/hud/marks.py` (F5-02).
//!
//! Marcas sobre la forma de la barra: la órbita de actividad. Una marca por
//! tarea (hasta cinco), en orden de prioridad, nunca texto: en la esfera, arcos
//! cortos agrupados arriba a la derecha; en la píldora, guiones centrados bajo
//! la forma.
//!
//! Cada estado se distingue en algo más que el color («nunca solo color»):
//! `working` respira en menta, `needs_you` es ámbar y más grueso, `failed` es
//! rojo, `waiting` y `stale` son neutros con menos opacidad, y `done` (sin ver)
//! es un punto menta.

use std::f64::consts::TAU;

use ez_gfx::Image;

use crate::tokens::palette;

/// Separación entre la forma y la órbita (px lógicos).
pub const ORBIT_GAP: f64 = 4.5;
/// Grosor de una marca.
pub const MARK_T: f64 = 2.4;
/// Largo de un arco en la esfera (grados).
pub const SLOT_DEG: f64 = 15.0;
/// Hueco entre arcos (grados).
pub const GAP_DEG: f64 = 8.0;
/// Largo de un guion bajo la píldora.
pub const PILL_DASH: f64 = 8.0;
/// Hueco entre guiones.
pub const PILL_GAP: f64 = 5.0;
/// Anillo de contraste con el escritorio.
pub const HALO: f64 = 1.3;
/// Periodo de la respiración de las marcas en curso (s).
pub const BREATH_SECS: f64 = 1.6;

const BREATH_STEPS: f64 = 10.0;

// Estados de `activity.py`, copiados aquí porque `ez-hud` no depende de
// `ez-herdr`.
pub const WORKING: &str = "working";
pub const NEEDS_YOU: &str = "needs_you";
pub const FAILED: &str = "failed";
pub const WAITING: &str = "waiting";
pub const STALE: &str = "stale";
pub const DONE: &str = "done";

/// Tintas de las marcas para un tema.
// DECISION: el `dict` de Python se porta como struct con campos nombrados;
// mismos nombres y mismos valores por defecto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tints {
    pub mint: [u8; 3],
    pub amber: [u8; 3],
    pub red: [u8; 3],
    pub muted: [u8; 3],
    pub halo: [u8; 4],
}

/// `INK["light"]`.
pub const INK_LIGHT: Tints = Tints {
    mint: [12, 112, 117],
    amber: [196, 122, 0],
    red: [200, 36, 26],
    muted: [78, 102, 106],
    halo: [255, 255, 255, 190],
};

/// `INK["dark"]`.
pub const INK_DARK: Tints = Tints {
    mint: [94, 226, 196],
    amber: [255, 192, 82],
    red: [255, 93, 82],
    muted: [138, 164, 163],
    halo: [6, 12, 14, 150],
};

/// `INK`: las tintas de fábrica por tema.
pub const INK: [(&str, Tints); 2] = [("light", INK_LIGHT), ("dark", INK_DARK)];

/// Tintas de las marcas: menta y neutro siguen a la paleta (también la
/// personalizada).
pub fn ink(theme: &str) -> Tints {
    let theme = if theme == "light" { "light" } else { "dark" };
    let factory = INK
        .iter()
        .find(|(name, _)| *name == theme)
        .map(|(_, tints)| *tints)
        .unwrap_or(INK_DARK);
    let palette = palette(theme).expect("el tema se resolvió a claro u oscuro");
    Tints {
        mint: palette["MINT"].channels(),
        muted: palette["TEXT_3"].channels(),
        ..factory
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Dash,
    Dot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Mint,
    Amber,
    Red,
    Muted,
}

#[derive(Clone, Copy)]
struct Style {
    shape: Shape,
    tone: Tone,
    weight: f64,
    alpha: f64,
}

const STYLES: [(&str, Style); 6] = [
    (
        WORKING,
        Style {
            shape: Shape::Dash,
            tone: Tone::Mint,
            weight: 1.0,
            alpha: 1.0,
        },
    ),
    (
        NEEDS_YOU,
        Style {
            shape: Shape::Dash,
            tone: Tone::Amber,
            weight: 1.3,
            alpha: 1.0,
        },
    ),
    (
        FAILED,
        Style {
            shape: Shape::Dash,
            tone: Tone::Red,
            weight: 1.0,
            alpha: 1.0,
        },
    ),
    (
        WAITING,
        Style {
            shape: Shape::Dash,
            tone: Tone::Muted,
            weight: 1.0,
            alpha: 0.75,
        },
    ),
    (
        STALE,
        Style {
            shape: Shape::Dash,
            tone: Tone::Muted,
            weight: 1.0,
            alpha: 0.45,
        },
    ),
    (
        DONE,
        Style {
            shape: Shape::Dot,
            tone: Tone::Mint,
            weight: 1.5,
            alpha: 1.0,
        },
    ),
];

fn style(state: &str) -> Option<Style> {
    STYLES
        .iter()
        .find(|(name, _)| *name == state)
        .map(|(_, style)| *style)
}

fn tone_color(tints: &Tints, tone: Tone) -> [u8; 3] {
    match tone {
        Tone::Mint => tints.mint,
        Tone::Amber => tints.amber,
        Tone::Red => tints.red,
        Tone::Muted => tints.muted,
    }
}

/// Opacidad de las marcas «trabajando», cuantizada para reutilizar caché.
pub fn breath(now: f64, reduced: bool) -> f64 {
    if reduced {
        return 1.0;
    }
    let wave = 0.5 + 0.5 * (TAU * now / BREATH_SECS).sin();
    ((0.45 + 0.55 * wave) * BREATH_STEPS).round_ties_even() / BREATH_STEPS
}

/// Polilíneas de cada hueco (px lógicos, relativas al centro de la forma).
pub fn mark_paths(layout: &str, count: usize, w: f64, h: f64) -> Vec<Vec<(f64, f64)>> {
    let mut paths = Vec::with_capacity(count);
    let count = count as f64;
    if layout == "circle" {
        let radius = w / 2.0 + ORBIT_GAP + MARK_T / 2.0;
        let total = count * SLOT_DEG + (count - 1.0) * GAP_DEG;
        let start = -45.0 - total / 2.0;
        for slot in 0..count as usize {
            let a0 = start + slot as f64 * (SLOT_DEG + GAP_DEG);
            let mut points = Vec::with_capacity(6);
            for step in 0..6 {
                let angle = (a0 + SLOT_DEG * step as f64 / 5.0).to_radians();
                points.push((radius * angle.cos(), radius * angle.sin()));
            }
            paths.push(points);
        }
        return paths;
    }
    let y = h / 2.0 + ORBIT_GAP + MARK_T / 2.0;
    let total = count * PILL_DASH + (count - 1.0) * PILL_GAP;
    for slot in 0..count as usize {
        let x0 = -total / 2.0 + slot as f64 * (PILL_DASH + PILL_GAP);
        paths.push(vec![(x0, y), (x0 + PILL_DASH, y)]);
    }
    paths
}

/// Segmentos `(ax, ay, bx, by)` de una marca; un punto se dibuja como un
/// segmento nulo en su centro.
fn segments(points: &[(f64, f64)], dot: bool) -> Vec<[f32; 4]> {
    if dot {
        let mid = if points.len() > 2 {
            points[points.len() / 2]
        } else {
            (
                (points[0].0 + points[points.len() - 1].0) / 2.0,
                (points[0].1 + points[points.len() - 1].1) / 2.0,
            )
        };
        let mid = [mid.0 as f32, mid.1 as f32];
        return vec![[mid[0], mid[1], mid[0], mid[1]]];
    }
    points
        .windows(2)
        .map(|pair| {
            [
                pair[0].0 as f32,
                pair[0].1 as f32,
                pair[1].0 as f32,
                pair[1].1 as f32,
            ]
        })
        .collect()
}

/// Distancia de un punto al segmento más cercano (unidades de rejilla).
///
/// Todo va en `f32`, como el Python: NumPy mantiene `float32` al operar con
/// escalares Python, y las máscaras de las marcas se cuantizan a 8 bits.
fn segment_distance(px: f32, py: f32, segments: &[[f32; 4]]) -> f32 {
    let mut best = f32::INFINITY;
    for segment in segments {
        let (ax, ay) = (segment[0], segment[1]);
        let dx = segment[2] - ax;
        let dy = segment[3] - ay;
        let length = (dx * dx + dy * dy).max(1e-9);
        let rx = px - ax;
        let ry = py - ay;
        let t = ((rx * dx + ry * dy) / length).clamp(0.0, 1.0);
        let qx = rx - t * dx;
        let qy = ry - t * dy;
        best = best.min((qx * qx + qy * qy).sqrt());
    }
    best
}

fn blend(pixel: &mut [f32; 4], rgb: [f32; 3], alpha: f32) {
    for (channel, value) in rgb.iter().enumerate() {
        pixel[channel] = *value * alpha + pixel[channel] * (1.0 - alpha);
    }
    pixel[3] = alpha + pixel[3] * (1.0 - alpha);
}

/// Dibuja una marca sobre el búfer premultiplicado de la capa.
#[allow(clippy::too_many_arguments)]
fn paint_state(
    out: &mut [[f32; 4]],
    xs: &[f32],
    ys: &[f32],
    state: &str,
    points: &[(f64, f64)],
    scale: f64,
    pulse: f64,
    tints: &Tints,
    halo: [f32; 4],
) {
    let style = style(state).expect("los estados se filtraron por estilo");
    let alpha = if state == WORKING {
        style.alpha * pulse
    } else {
        style.alpha
    };
    let shape = segments(points, style.shape == Shape::Dot);
    let radius = MARK_T * style.weight * scale / 2.0;
    let halo_radius = (radius + HALO * scale) as f32;
    let scale = scale as f32;
    let color = tone_color(tints, style.tone).map(|value| value as f32 / 255.0);
    for (row, gy) in ys.iter().enumerate() {
        for (column, gx) in xs.iter().enumerate() {
            let distance = segment_distance(*gx, *gy, &shape) * scale;
            let pixel = &mut out[row * xs.len() + column];
            let halo_cover =
                (halo_radius - distance + 0.5).clamp(0.0, 1.0) * halo[3] * alpha as f32;
            blend(pixel, [halo[0], halo[1], halo[2]], halo_cover);
            let cover = (radius as f32 - distance + 0.5).clamp(0.0, 1.0) * alpha as f32;
            blend(pixel, color, cover);
        }
    }
}

/// Capa RGBA centrada en la forma con las marcas de `states` (`None` si no hay).
///
/// `w` y `h` son las medidas lógicas actuales de la forma (con su crecimiento).
// DECISION: sin la caché global `_TILES` de Python; la función es pura y quien
// dibuja puede cachear por (layout, states, medidas, escala, tema, pulso).
pub fn orbit_tile(
    layout: &str,
    states: &[&str],
    w: f64,
    h: f64,
    scale: f64,
    theme: &str,
    pulse: f64,
) -> Option<Image> {
    let states: Vec<&str> = states
        .iter()
        .copied()
        .filter(|state| style(state).is_some())
        .collect();
    if states.is_empty() {
        return None;
    }
    let theme = if theme == "light" { "light" } else { "dark" };
    let reach = ORBIT_GAP + MARK_T * 1.6 + HALO + 2.0;
    let side_w = ((w + 2.0 * reach) * scale).ceil() as usize;
    let side_h = ((h + 2.0 * reach) * scale).ceil() as usize;
    let scale32 = scale as f32;
    let center_x = (side_w as f64 / scale / 2.0) as f32;
    let center_y = (side_h as f64 / scale / 2.0) as f32;
    let xs: Vec<f32> = (0..side_w)
        .map(|index| (index as f32 + 0.5) / scale32 - center_x)
        .collect();
    let ys: Vec<f32> = (0..side_h)
        .map(|index| (index as f32 + 0.5) / scale32 - center_y)
        .collect();
    let mut out = vec![[0.0f32; 4]; side_w * side_h];
    let tints = ink(theme);
    let halo = tints.halo.map(|value| value as f32 / 255.0);
    let paths = mark_paths(layout, states.len(), w, h);
    for (state, points) in states.iter().zip(paths.iter()) {
        paint_state(
            &mut out, &xs, &ys, state, points, scale, pulse, &tints, halo,
        );
    }
    let mut data = Vec::with_capacity(out.len() * 4);
    for pixel in out {
        for channel in pixel {
            data.push((channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    Some(
        Image::from_premultiplied_rgba(side_w as u32, side_h as u32, data)
            .expect("el búfer tiene el tamaño de la imagen"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }

    #[test]
    fn breath_quantizes_like_python() {
        assert_eq!(breath(0.0, false), 0.7);
        assert_eq!(breath(0.4, false), 1.0);
        assert_eq!(breath(0.8, false), 0.7);
        assert_eq!(breath(1.2, false), 0.4);
        assert_eq!(breath(0.3, true), 1.0);
    }

    #[test]
    fn the_pill_paths_sit_under_the_shape() {
        let paths = mark_paths("pill", 2, 96.0, 41.0);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec![(-10.5, 26.2), (-2.5, 26.2)]);
        assert_eq!(paths[1], vec![(2.5, 26.2), (10.5, 26.2)]);
    }

    #[test]
    fn the_sphere_arcs_cluster_top_right() {
        let paths = mark_paths("circle", 1, 48.0, 48.0);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].len(), 6);
        let (x, y) = paths[0][0];
        assert!((x - 18.080_214_441_559_004).abs() < 1e-9);
        assert!((y + 23.562_594_206_649_685).abs() < 1e-9);
        let paths = mark_paths("circle", 2, 48.0, 48.0);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1].len(), 6);
        assert!(paths[1][5].0 > paths[0][0].0);
    }

    #[test]
    fn the_inks_follow_the_theme_palette() {
        let light = ink("light");
        assert_eq!(light.mint, [12, 112, 117]);
        assert_eq!(light.amber, [196, 122, 0]);
        assert_eq!(light.red, [200, 36, 26]);
        assert_eq!(light.muted, [78, 102, 106]);
        assert_eq!(light.halo, [255, 255, 255, 190]);
        let dark = ink("dark");
        assert_eq!(dark.mint, [94, 226, 196]);
        assert_eq!(dark.muted, [138, 164, 163]);
        assert_eq!(dark.halo, [6, 12, 14, 150]);
        // Cualquier otro tema cae a oscuro, como `"light" if theme == "light"`.
        assert_eq!(ink("auto"), dark);
        assert_eq!(INK.len(), 2);
    }

    #[test]
    fn an_empty_or_unknown_orbit_draws_nothing() {
        assert!(orbit_tile("pill", &[], 96.0, 41.0, 1.0, "dark", 1.0).is_none());
        assert!(orbit_tile("pill", &["nope"], 96.0, 41.0, 1.0, "dark", 1.0).is_none());
    }

    #[test]
    fn the_pill_orbit_matches_the_pillow_oracle() {
        let tile = orbit_tile("pill", &[WORKING, NEEDS_YOU], 96.0, 41.0, 1.0, "dark", 0.7)
            .expect("dos estados válidos");
        assert_eq!((tile.width(), tile.height()), (120, 65));
        assert_eq!(tile.pixel(53, 58).to_array(), [81, 194, 168, 210]);
        assert_eq!(tile.pixel(66, 58).to_array(), [255, 192, 82, 255]);
        assert_eq!(fnv1a(tile.as_raw()), 0x6910_8E44_7762_4777);
    }

    #[test]
    fn the_sphere_orbit_matches_the_pillow_oracle() {
        let tile =
            orbit_tile("circle", &[DONE], 48.0, 48.0, 1.0, "light", 1.0).expect("un estado válido");
        assert_eq!((tile.width(), tile.height()), (72, 72));
        assert_eq!(tile.pixel(57, 15).to_array(), [12, 112, 117, 255]);
        assert_eq!(tile.pixel(56, 16).to_array(), [31, 123, 128, 248]);
        assert_eq!(fnv1a(tile.as_raw()), 0x0175_B757_F9B6_65E1);
    }
}
