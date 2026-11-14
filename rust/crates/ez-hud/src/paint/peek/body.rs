//! Sombra y cuerpo redondeado de la tarjeta (`peek_paint.py`).

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use ez_gfx::draw::Draw;
use ez_gfx::{Image, Rgba};

use crate::theme::card_palette;

use super::{MARGIN, RADIUS};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BodyKey {
    width: usize,
    height: usize,
    scale: u64,
    theme: String,
}

type BodyCache = Mutex<VecDeque<(BodyKey, Arc<Vec<f32>>)>>;

static BODY_CACHE: LazyLock<BodyCache> = LazyLock::new(|| Mutex::new(VecDeque::new()));

/// Sombra + cuerpo redondeado: float32 RGBA premultiplicado, solo lectura.
pub(super) fn body(width: usize, height: usize, scale: f64, theme: &str) -> Arc<Vec<f32>> {
    let key = BodyKey {
        width,
        height,
        scale: scale.to_bits(),
        theme: theme.to_string(),
    };
    {
        let mut cache = BODY_CACHE.lock();
        if let Some(index) = cache.iter().position(|(cached, _)| *cached == key) {
            let entry = cache.remove(index).expect("el índice existe");
            let data = Arc::clone(&entry.1);
            cache.push_back(entry);
            return data;
        }
    }
    let data = Arc::new(paint_body(width, height, scale, theme));
    let mut cache = BODY_CACHE.lock();
    if cache.len() >= 12 {
        cache.pop_front();
    }
    cache.push_back((key, Arc::clone(&data)));
    data
}

/// `np.asarray(im, np.float32) / 255.0` de un búfer premultiplicado.
fn to_f32(data: &[u8]) -> Vec<f32> {
    data.iter().map(|&value| f32::from(value) / 255.0).collect()
}

/// `Image.reduce(2)` sobre un búfer premultiplicado (promedio de cajas).
fn reduce_two(data: &[u8], width: usize, height: usize) -> (usize, usize, Vec<u8>) {
    let out_w = width.div_ceil(2);
    let out_h = height.div_ceil(2);
    let mut out = vec![0u8; out_w * out_h * 4];
    for oy in 0..out_h {
        for ox in 0..out_w {
            let x0 = ox * 2;
            let y0 = oy * 2;
            let x1 = (x0 + 2).min(width);
            let y1 = (y0 + 2).min(height);
            let count = ((x1 - x0) * (y1 - y0)) as u32;
            for channel in 0..4 {
                let mut sum = 0u32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        sum += u32::from(data[(y * width + x) * 4 + channel]);
                    }
                }
                out[(oy * out_w + ox) * 4 + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
    (out_w, out_h, out)
}

fn paint_body(width: usize, height: usize, scale: f64, theme: &str) -> Vec<f32> {
    let pal = card_palette(theme);
    let super_sample = 2usize;
    let sw = width * super_sample;
    let sh = height * super_sample;
    let factor = super_sample as f64;
    let margin = MARGIN * scale * factor;
    let radius = RADIUS * scale * factor;
    let box_ = [
        margin,
        margin,
        sw as f64 - margin - 1.0,
        sh as f64 - margin - 1.0,
    ];
    let mut shadow = Image::new(sw as u32, sh as u32, Rgba::TRANSPARENT);
    let lift = 3.0 * scale * factor;
    let shadow_alpha = if theme == "dark" { 110 } else { 46 };
    Draw::new(&mut shadow)
        .rounded_rectangle(
            [box_[0], box_[1] + lift, box_[2], box_[3] + lift],
            radius,
            Some(Rgba::new(0, 0, 0, shadow_alpha)),
            None,
            0,
        )
        .expect("el rectángulo redondeado de la sombra es válido");
    let shadow = shadow.gaussian_blur((7.0 * scale * factor) as f32);

    let mut card = Image::new(sw as u32, sh as u32, Rgba::TRANSPARENT);
    let body_fill = pal["BODY"].channels();
    let line = pal["LINE"];
    let outline = Rgba::new(
        line.channels()[0],
        line.channels()[1],
        line.channels()[2],
        line.alpha().unwrap_or(40),
    );
    let line_width = ((scale * factor).round_ties_even() as u32).max(1);
    Draw::new(&mut card)
        .rounded_rectangle(
            box_,
            radius,
            Some(Rgba::new(body_fill[0], body_fill[1], body_fill[2], 255)),
            Some(outline),
            line_width,
        )
        .expect("el rectángulo redondeado de la tarjeta es válido");

    let composed = ez_gfx::alpha_composite(&shadow, &card).expect("mismo tamaño");
    let premultiplied = composed.to_premultiplied_rgba();
    let (_, _, reduced) = reduce_two(&premultiplied, sw, sh);
    to_f32(&reduced)
}
