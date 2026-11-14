//! `drop.py 794-844`: cuerpo redondeado de la tarjeta de ajustes.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use ez_gfx::draw::Draw;
use ez_gfx::raster::{Image, Rgba};

use crate::theme::{self, CARD_RADIUS};
use crate::tokens::{self, Color, FLARE, R_MENU};

use super::canvas::reduce_rgba;

type Key = (u32, u32, u64, String, bool, i64);

#[derive(Default)]
struct Cache {
    entries: VecDeque<(Key, Arc<[f32]>)>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Cuerpo redondeado de la tarjeta de ajustes: RGBA premultiplicado `0..1`.
///
/// `fresh` y `version` son `false` y `0` en Python (Rust no tiene argumentos
/// por defecto). Se cachea igual que el `lru_cache(maxsize=48)` de Python;
/// `version` es `tokens.palette_version()` y solo entra en la clave de la
/// caché. El resultado es de solo lectura: quien lo use debe copiarlo antes de
/// tocarlo.
// DECISION: se devuelve `Arc<[f32]>` (solo lectura y compartido entre llamadas)
// porque el `lru_cache` de Python devuelve el mismo array marcado `write=False`.
pub fn render_card_body(
    width: u32,
    height: u32,
    scale: f64,
    theme: &str,
    fresh: bool,
    version: i64,
) -> Result<Arc<[f32]>, String> {
    let key = (
        width,
        height,
        scale.to_bits(),
        theme.to_string(),
        fresh,
        version,
    );
    if let Some(hit) = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let position = cache.entries.iter().position(|entry| entry.0 == key)?;
        let entry = cache.entries.remove(position)?;
        let value = entry.1.clone();
        cache.entries.push_back(entry);
        Some(value)
    }) {
        return Ok(hit);
    }
    let built = build_card(width, height, scale, theme, fresh)?;
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.entries.len() >= 48 {
            cache.entries.pop_front();
        }
        cache.entries.push_back((key, built.clone()));
    });
    Ok(built)
}

fn rgba(color: Color) -> [u8; 4] {
    match color {
        Color::Rgb(rgb) => [rgb[0], rgb[1], rgb[2], 255],
        Color::Rgba(rgba) => rgba,
    }
}

fn build_card(
    width: u32,
    height: u32,
    scale: f64,
    theme: &str,
    fresh: bool,
) -> Result<Arc<[f32]>, String> {
    let palette = if fresh {
        theme::card_palette(theme)
    } else {
        tokens::palette(theme)?
    };
    let body = palette["BODY"].channels();
    let line = rgba(palette["LINE"]);
    let ss = 2u32;
    let mut image = Image::new(width * ss, height * ss, Rgba::TRANSPARENT);
    let x0 = f64::from(FLARE) * scale * f64::from(ss);
    let radius = if fresh {
        f64::from(CARD_RADIUS)
    } else {
        f64::from(R_MENU)
    } * scale
        * f64::from(ss);
    Draw::new(&mut image)
        .rounded_rectangle(
            [
                x0,
                0.0,
                f64::from(width * ss) - x0 - 1.0,
                f64::from(height * ss) - 1.0,
            ],
            radius,
            Some(Rgba::opaque(body[0], body[1], body[2])),
            Some(Rgba::from_array(line)),
            (f64::from(ss) * scale).round_ties_even().max(1.0) as u32,
        )
        .expect("tarjeta con coordenadas ordenadas");
    let premultiplied = image.to_premultiplied_rgba();
    let reduced = reduce_rgba(&premultiplied, width * ss, height * ss, ss);
    let mut values: Vec<f32> = reduced.iter().map(|&v| f32::from(v) / 255.0).collect();
    if fresh {
        let top = theme::body_top(theme)
            .channels()
            .map(|v| f32::from(v) / 255.0);
        let bottom = body.map(|v| f32::from(v) / 255.0);
        let denominator = (160.0 * scale).max(1.0) as f32;
        for row in 0..height as usize {
            let k = (row as f32 / denominator).clamp(0.0, 1.0);
            let shift = [
                (top[0] - bottom[0]) * (1.0 - k),
                (top[1] - bottom[1]) * (1.0 - k),
                (top[2] - bottom[2]) * (1.0 - k),
            ];
            for column in 0..width as usize {
                let index = (row * width as usize + column) * 4;
                let alpha = values[index + 3];
                for channel in 0..3 {
                    values[index + channel] =
                        (values[index + channel] + shift[channel] * alpha).clamp(0.0, 1.0);
                }
            }
        }
    }
    Ok(Arc::from(values))
}
