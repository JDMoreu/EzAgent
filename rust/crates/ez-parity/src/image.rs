//! Comparador de cuadros del HUD contra `parity/golden/` (§4.4).

use serde::Deserialize;

/// Bloque `[pixels.shapes]` / `[pixels.text]` de `parity/thresholds.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct PixelThresholds {
    pub max_channel_delta_p999: Option<u32>,
    pub max_channel_delta_any: Option<u32>,
    pub max_channel_delta_p995: Option<u32>,
    pub ssim_min: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thresholds {
    pub pixels: PixelPixels,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PixelPixels {
    pub shapes: PixelThresholds,
    pub text: PixelThresholds,
}

impl Thresholds {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
}

/// Diferencia por canal entre dos imágenes RGBA del mismo tamaño.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageDelta {
    pub max: u32,
    pub p995: u32,
    pub p999: u32,
    pub ssim: f64,
}

pub fn decode(path: &std::path::Path) -> Result<image::RgbaImage, String> {
    image::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))
        .map(|image| image.to_rgba8())
}

fn percentile(histogram: &[u64; 256], total: u64, fraction: f64) -> u32 {
    if total == 0 {
        return 0;
    }
    let target = (total as f64 * fraction).ceil() as u64;
    let mut seen = 0_u64;
    for (delta, count) in histogram.iter().enumerate() {
        seen += count;
        if seen >= target {
            return delta as u32;
        }
    }
    255
}

/// SSIM en escala de grises, bloques de 8x8, promedio global.
///
/// shortcut: una sola escala y ventana uniforme; suficiente para texto sobre
/// fondos planos, subir a multi-escala si aparecen diferencias espurias.
pub fn ssim(left: &image::RgbaImage, right: &image::RgbaImage) -> f64 {
    if left.dimensions() != right.dimensions() {
        return 0.0;
    }
    let (width, height) = left.dimensions();
    let mut total = 0.0;
    let mut blocks = 0.0;
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let block = gray_block(left, right, x, y);
            total += block;
            blocks += 1.0;
            x += 8;
        }
        y += 8;
    }
    if blocks == 0.0 {
        return 1.0;
    }
    total / blocks
}

fn gray(value: &image::Rgba<u8>) -> f64 {
    let [r, g, b, _] = value.0;
    0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)
}

fn gray_block(left: &image::RgbaImage, right: &image::RgbaImage, x0: u32, y0: u32) -> f64 {
    const C1: f64 = 6.5025;
    const C2: f64 = 58.5225;
    let (width, height) = left.dimensions();
    let mut count = 0.0;
    let (mut sum_a, mut sum_b, mut sum_aa, mut sum_bb, mut sum_ab) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for y in y0..(y0 + 8).min(height) {
        for x in x0..(x0 + 8).min(width) {
            let a = gray(left.get_pixel(x, y));
            let b = gray(right.get_pixel(x, y));
            count += 1.0;
            sum_a += a;
            sum_b += b;
            sum_aa += a * a;
            sum_bb += b * b;
            sum_ab += a * b;
        }
    }
    if count == 0.0 {
        return 1.0;
    }
    let mean_a = sum_a / count;
    let mean_b = sum_b / count;
    let var_a = sum_aa / count - mean_a * mean_a;
    let var_b = sum_bb / count - mean_b * mean_b;
    let covariance = sum_ab / count - mean_a * mean_b;
    let numerator = (2.0 * mean_a * mean_b + C1) * (2.0 * covariance + C2);
    let denominator = (mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2);
    if denominator == 0.0 {
        return 1.0;
    }
    numerator / denominator
}

pub fn compare(left: &image::RgbaImage, right: &image::RgbaImage) -> ImageDelta {
    if left.dimensions() != right.dimensions() {
        return ImageDelta {
            max: u32::MAX,
            p995: 255,
            p999: 255,
            ssim: 0.0,
        };
    }
    let mut histogram = [0_u64; 256];
    let mut max = 0_u32;
    let mut total = 0_u64;
    for (a, b) in left.pixels().zip(right.pixels()) {
        for channel in 0..4 {
            let delta = a.0[channel].abs_diff(b.0[channel]);
            histogram[delta as usize] += 1;
            total += 1;
            max = max.max(u32::from(delta));
        }
    }
    ImageDelta {
        max,
        p995: percentile(&histogram, total, 0.995),
        p999: percentile(&histogram, total, 0.999),
        ssim: ssim(left, right),
    }
}

/// Veredicto según el umbral que corresponde (forma o texto).
pub fn within(delta: &ImageDelta, thresholds: &PixelThresholds) -> bool {
    if delta.max == u32::MAX {
        return false;
    }
    if let Some(limit) = thresholds.max_channel_delta_any {
        if delta.max > limit {
            return false;
        }
    }
    if let Some(limit) = thresholds.max_channel_delta_p999 {
        if delta.p999 > limit {
            return false;
        }
    }
    if let Some(limit) = thresholds.max_channel_delta_p995 {
        if delta.p995 > limit {
            return false;
        }
    }
    if let Some(limit) = thresholds.ssim_min {
        if delta.ssim < limit {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(value: u8, extra: Option<(u32, u8)>) -> image::RgbaImage {
        let mut image =
            image::RgbaImage::from_pixel(32, 32, image::Rgba([value, value, value, 255]));
        if let Some((x, delta)) = extra {
            image.put_pixel(x, 0, image::Rgba([value + delta, value, value, 255]));
        }
        image
    }

    #[test]
    fn identical_frames_pass() {
        let frame = flat(120, None);
        let delta = compare(&frame, &frame);
        assert_eq!(delta.max, 0);
        assert!((delta.ssim - 1.0).abs() < 1e-12);
    }

    #[test]
    fn one_pixel_off_by_one_is_within_shape_thresholds() {
        let left = flat(120, None);
        let right = flat(120, Some((3, 1)));
        let delta = compare(&left, &right);
        assert_eq!(delta.max, 1);
        let thresholds = PixelThresholds {
            max_channel_delta_p999: Some(1),
            max_channel_delta_any: Some(3),
            max_channel_delta_p995: None,
            ssim_min: None,
        };
        assert!(within(&delta, &thresholds));
    }

    #[test]
    fn bigger_difference_fails() {
        let left = flat(120, None);
        let right = flat(120, Some((3, 9)));
        let delta = compare(&left, &right);
        assert!(!within(
            &delta,
            &PixelThresholds {
                max_channel_delta_p999: Some(1),
                max_channel_delta_any: Some(3),
                max_channel_delta_p995: None,
                ssim_min: None,
            }
        ));
    }

    #[test]
    fn size_mismatch_fails() {
        let left = flat(10, None);
        let right = image::RgbaImage::from_pixel(16, 16, image::Rgba([10, 10, 10, 255]));
        assert!(!within(
            &compare(&left, &right),
            &PixelThresholds {
                max_channel_delta_p999: Some(1),
                max_channel_delta_any: Some(3),
                max_channel_delta_p995: None,
                ssim_min: None,
            }
        ));
    }

    #[test]
    fn reads_repository_thresholds() {
        let path = std::path::Path::new("../../../parity/thresholds.toml");
        let thresholds = Thresholds::load(path).expect("thresholds.toml legible");
        assert_eq!(thresholds.pixels.shapes.max_channel_delta_p999, Some(1));
        assert_eq!(thresholds.pixels.text.ssim_min, Some(0.995));
    }
}
