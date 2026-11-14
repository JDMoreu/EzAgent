//! `drop.py 536-552, 685-736`: lienzo de composición, reducción y tinte de pausa.

use ez_gfx::raster::{Image, Mask, Rgba};
use ez_gfx::resample::Resampling;

use super::{MUTE_DIM, MUTE_KEEP};

/// `Pillow Image.reduce(factor)` sobre RGBA premultiplicado (media de caja).
///
/// // shortcut: asume que las dimensiones son múltiplos del factor, como los
/// // dos llamadores del HUD (discos y tarjeta); con un resto Pillow redondea
/// // el tamaño y promedia menos píxeles.
pub(super) fn reduce_rgba(data: &[u8], width: u32, height: u32, factor: u32) -> Vec<u8> {
    let step = factor as usize;
    let (width, height) = (width as usize, height as usize);
    let (out_width, out_height) = (width / step, height / step);
    let expected = step * step;
    let amend = (expected / 2) as u32;
    // Pillow: `(ss + amend) >> 2` para 2×2 y 4×4, multiplicador fijo para el resto.
    let multiplier = (4_294_967_296.0 / (256.0 * expected as f64)) as u32;
    let mut out = vec![0u8; out_width * out_height * 4];
    for out_y in 0..out_height {
        for out_x in 0..out_width {
            for channel in 0..4 {
                let mut sum = amend;
                for y in 0..step {
                    for x in 0..step {
                        let index = ((out_y * step + y) * width + out_x * step + x) * 4 + channel;
                        sum += u32::from(data[index]);
                    }
                }
                out[(out_y * out_width + out_x) * 4 + channel] = match step {
                    2 => (sum >> 2) as u8,
                    4 => (sum >> 4) as u8,
                    _ => (sum.wrapping_mul(multiplier) >> 24) as u8,
                };
            }
        }
    }
    out
}

/// Lienzo RGBA donde cada elemento se compone sobre los demás.
pub(super) struct Canvas {
    pub image: Image,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            image: Image::new(width, height, Rgba::TRANSPARENT),
        }
    }

    /// `_Canvas.paste`: escala, funde el alfa y compone con recorte.
    pub fn paste(&mut self, tile: &Image, cx: f64, cy: f64, scale: f64, alpha: f64) {
        if alpha <= 0.01 || scale <= 0.02 {
            return;
        }
        let resized = if (scale - 1.0).abs() > 0.02 {
            let width = (f64::from(tile.width()) * scale).round_ties_even().max(1.0) as u32;
            let height = (f64::from(tile.height()) * scale)
                .round_ties_even()
                .max(1.0) as u32;
            Some(tile.resize(width, height, Resampling::Bilinear))
        } else {
            None
        };
        let base = resized.as_ref().unwrap_or(tile);
        let faded = if alpha < 0.99 {
            // `point(lambda v: int(v * f))` trunca hacia cero.
            let mut copy = base.clone();
            for pixel in copy.as_raw_mut().chunks_mut(4) {
                pixel[3] = (f64::from(pixel[3]) * alpha) as u8;
            }
            Some(copy)
        } else {
            None
        };
        let final_tile = faded.as_ref().unwrap_or(base);
        // No hace falta recortar: `alpha_composite` ya recorta en los bordes.
        let x = (cx - f64::from(final_tile.width()) / 2.0).round_ties_even() as i32;
        let y = (cy - f64::from(final_tile.height()) / 2.0).round_ties_even() as i32;
        self.image.alpha_composite(final_tile, (x, y));
    }
}

/// `ImageDraw` reemplaza el alfa del interior por la máscara de la forma.
pub(super) fn put_alpha(image: &mut Image, mask: &Mask) {
    debug_assert_eq!(
        (image.width(), image.height()),
        (mask.width(), mask.height())
    );
    let alpha = mask.as_raw();
    for (index, pixel) in image.as_raw_mut().chunks_mut(4).enumerate() {
        pixel[3] = alpha.get(index).copied().unwrap_or(0);
    }
}

/// `round(value, 2)` con el redondeo a par de Python.
// shortcut: `(x * 100).round_ties_even()` puede diferir de `_Py_dg_dtoa` en un
// empate exacto a 0,005; para `grow` (≈1) solo cambiaría el tamaño de la órbita.
pub(super) fn round2(value: f64) -> f64 {
    (value * 100.0).round_ties_even() / 100.0
}

/// `ImageEnhance.Color(...).enhance(k)` seguido de `Brightness(...).enhance(k)`.
///
/// Pillow convierte a `L` (luma 299/587/114), vuelve a `RGB` y mezcla con
/// `in1 + alpha * (in2 - in1)` en `f32` (FMA) antes de truncar.
pub(super) fn mute_tint(interior: &Image, k: f64) -> Image {
    let color = (1.0 - (1.0 - MUTE_KEEP) * k) as f32;
    let brightness = (1.0 - (1.0 - MUTE_DIM) * k) as f32;
    let mut out = interior.clone();
    for pixel in out.as_raw_mut().chunks_mut(4) {
        let luma = (u32::from(pixel[0]) * 299
            + u32::from(pixel[1]) * 587
            + u32::from(pixel[2]) * 114
            + 500)
            / 1000;
        for value in pixel.iter_mut().take(3) {
            // Pillow mezcla con `in1 + alpha * (in2 - in1)` y FMA en float32:
            // el resultado se redondea una sola vez antes de truncar.
            let gray = luma as f32;
            let delta = f32::from(*value) - gray;
            let mixed = delta.mul_add(color, gray) as u8;
            *value = f32::from(mixed).mul_add(brightness, 0.0) as u8;
        }
    }
    out
}
