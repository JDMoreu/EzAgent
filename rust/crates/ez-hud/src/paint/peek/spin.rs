//! Indicador en curso de la vista rápida (`peek_paint.py`).

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use crate::tokens::Color;

use super::PeekBuffer;

/// Pasos de la rueda del indicador en curso.
pub const SPIN_STEPS: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SpinKey {
    scale: u64,
    color: [u8; 4],
    step: usize,
    size: u64,
}

type SpinCache = Mutex<VecDeque<(SpinKey, Arc<PeekBuffer>)>>;

static SPIN_CACHE: LazyLock<SpinCache> = LazyLock::new(|| Mutex::new(VecDeque::new()));

fn color_rgba(color: Color) -> [u8; 4] {
    match color {
        Color::Rgb(rgb) => [rgb[0], rgb[1], rgb[2], 255],
        Color::Rgba(rgba) => rgba,
    }
}

/// `segment_distance` de `icons.py`, con todo en `float32`.
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

/// `colorize(mask, color)`: máscara de cobertura → sprite premultiplicado.
fn colorize(cover: &[f32], width: usize, height: usize, color: Color) -> PeekBuffer {
    let rgba = color_rgba(color);
    let alpha = f64::from(rgba[3]) / 255.0;
    let rgb = [
        (f64::from(rgba[0]) / 255.0 * alpha) as f32,
        (f64::from(rgba[1]) / 255.0 * alpha) as f32,
        (f64::from(rgba[2]) / 255.0 * alpha) as f32,
    ];
    let mut sprite = PeekBuffer::new(width, height);
    for (index, &value) in cover.iter().enumerate() {
        let pixel = &mut sprite.data[index * 4..index * 4 + 4];
        pixel[0] = rgb[0] * value;
        pixel[1] = rgb[1] * value;
        pixel[2] = rgb[2] * value;
        pixel[3] = alpha as f32 * value;
    }
    sprite
}

/// Arco de 270° que gira (tarea en curso), cacheado por paso de ángulo.
pub fn spinner(scale: f64, color: Color, angle_step: usize, size: f64) -> PeekBuffer {
    let key = SpinKey {
        scale: ((scale * 1000.0).round_ties_even() / 1000.0).to_bits(),
        color: color_rgba(color),
        step: angle_step % SPIN_STEPS,
        size: size.to_bits(),
    };
    {
        let mut cache = SPIN_CACHE.lock();
        if let Some(index) = cache.iter().position(|(cached, _)| *cached == key) {
            let entry = cache.remove(index).expect("el índice existe");
            let data = Arc::clone(&entry.1);
            cache.push_back(entry);
            return (*data).clone();
        }
    }
    let side = ((size * scale).round_ties_even() as usize).max(4);
    let scale32 = scale as f32;
    let origin = (side as f64 / scale / 2.0) as f32;
    let axis: Vec<f32> = (0..side)
        .map(|index| (index as f32 + 0.5) / scale32 - origin)
        .collect();
    let start = 360.0 * (angle_step % SPIN_STEPS) as f64 / SPIN_STEPS as f64;
    let radius = size * 0.32;
    let points: Vec<(f64, f64)> = (0..19)
        .map(|k| {
            let angle = (start + 270.0 * k as f64 / 18.0).to_radians();
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect();
    let segments: Vec<[f32; 4]> = points
        .windows(2)
        .map(|pair| {
            [
                pair[0].0 as f32,
                pair[0].1 as f32,
                pair[1].0 as f32,
                pair[1].1 as f32,
            ]
        })
        .collect();
    let threshold = 0.9 * scale;
    let mut cover = Vec::with_capacity(side * side);
    for row in 0..side {
        for column in 0..side {
            let distance = segment_distance(axis[column], axis[row], &segments) * scale32;
            cover.push((threshold as f32 - distance + 0.5).clamp(0.0, 1.0));
        }
    }
    let sprite = colorize(&cover, side, side, color);
    if SPIN_CACHE.lock().len() > 96 {
        SPIN_CACHE.lock().clear();
    }
    SPIN_CACHE.lock().push_back((key, Arc::new(sprite.clone())));
    sprite
}
