//! Bit-exact port of Pillow's `ImageFilter.GaussianBlur` (`BoxBlur.c`).
//!
//! Pillow approximates a Gaussian with three extended box blurs per axis. The
//! fractional box radius comes from `_gaussian_blur_radius` (all arithmetic in
//! `f32`, as in C), and each pass uses 24-bit fixed-point accumulation with
//! Pillow's edge replication. The horizontal passes run first, then the
//! vertical ones, which is equivalent to Pillow's transpose/blur/transpose.

use crate::raster::{Image, Mask};

/// `_imaging.c` always calls `ImagingGaussianBlur` with three passes.
const PASSES: usize = 3;
/// Fixed-point scale of `BoxBlur.c`.
const FIXED_ONE: i64 = 1 << 24;
/// Rounding term added before the final shift.
const ROUND: i64 = 1 << 23;

/// Parameters shared by every line of one box-blur pass.
///
/// shortcut: Pillow's 32-bit accumulator wraps for box radii above ~8.4e6;
/// the HUD stays below 100, so this port uses `i64` and would diverge there.
struct BoxParams {
    radius: i64,
    weight: i64,
    fringe: i64,
    channels: usize,
}

impl BoxParams {
    /// `ImagingHorizontalBoxBlur`'s `radius`, `ww` and `fw`.
    fn new(float_radius: f32, channels: usize) -> Self {
        let radius = float_radius as i64;
        let divisor = float_radius * 2.0 + 1.0;
        let weight = (FIXED_ONE as f32 / divisor) as i64;
        let covered = (radius as u64 * 2 + 1) * weight as u64;
        let fringe = ((FIXED_ONE as u64).saturating_sub(covered) / 2) as i64;
        Self {
            radius,
            weight,
            fringe,
            channels,
        }
    }
}

/// `_gaussian_blur_radius` from `BoxBlur.c`, including its `f32` rounding.
fn gaussian_blur_radius(radius: f32, passes: usize) -> f32 {
    let sigma2 = radius * radius / passes as f32;
    let box_length = (12.0f64 * f64::from(sigma2) + 1.0).sqrt() as f32;
    let integer = ((f64::from(box_length) - 1.0) / 2.0).floor() as f32;
    let mut fraction = (2.0 * integer + 1.0) * (integer * (integer + 1.0) - 3.0 * sigma2);
    fraction /= 6.0 * (sigma2 - (integer + 1.0) * (integer + 1.0));
    integer + fraction
}

/// The `acc` initialisation of `ImagingLineBoxBlur8/32`.
fn initial_accumulator(src: &[u8], last: usize, edge_a: usize, params: &BoxParams) -> [i64; 4] {
    let channels = params.channels;
    let mut accumulator = [0i64; 4];
    for (channel, value) in accumulator.iter_mut().take(channels).enumerate() {
        *value = i64::from(src[channel]) * (params.radius + 1);
    }
    for x in 0..edge_a - 1 {
        for (channel, value) in accumulator.iter_mut().take(channels).enumerate() {
            *value += i64::from(src[x * channels + channel]);
        }
    }
    let tail = params.radius - edge_a as i64 + 1;
    for (channel, value) in accumulator.iter_mut().take(channels).enumerate() {
        *value += i64::from(src[last * channels + channel]) * tail;
    }
    accumulator
}

/// One box-blurred line (`ImagingLineBoxBlur8` / `ImagingLineBoxBlur32`).
fn box_blur_line(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    params: &BoxParams,
    edge_a: usize,
    edge_b: usize,
) {
    let last = width - 1;
    let radius = params.radius as usize;
    let channels = params.channels;
    let mut accumulator = initial_accumulator(src, last, edge_a, params);
    let mut emit = |x: usize, subtract: usize, add: usize, right: usize| {
        for (channel, value) in accumulator.iter_mut().take(channels).enumerate() {
            let added = i64::from(src[add * channels + channel]);
            let removed = i64::from(src[subtract * channels + channel]);
            *value += added - removed;
        }
        for (channel, value) in accumulator.iter().take(channels).enumerate() {
            let left = i64::from(src[subtract * channels + channel]);
            let right = i64::from(src[right * channels + channel]);
            let bulk = value * params.weight + (left + right) * params.fringe;
            dst[x * channels + channel] = ((bulk + ROUND) >> 24) as u8;
        }
    };

    if edge_a <= edge_b {
        for x in 0..edge_a {
            emit(x, 0, x + radius, x + radius + 1);
        }
        for x in edge_a..edge_b {
            emit(x, x - radius - 1, x + radius, x + radius + 1);
        }
        for x in edge_b..=last {
            emit(x, x - radius - 1, last, last);
        }
    } else {
        for x in 0..edge_b {
            emit(x, 0, x + radius, x + radius + 1);
        }
        for x in edge_b..edge_a {
            emit(x, 0, last, last);
        }
        for x in edge_a..=last {
            emit(x, x - radius - 1, last, last);
        }
    }
}

/// One horizontal box-blur pass over a whole frame (`ImagingHorizontalBoxBlur`).
fn horizontal_pass(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    channels: usize,
    float_radius: f32,
) {
    let params = BoxParams::new(float_radius, channels);
    let edge_a = (params.radius + 1).min(width as i64) as usize;
    let edge_b = (width as i64 - params.radius - 1).max(0) as usize;
    let stride = width * channels;
    for y in 0..height {
        let input = &src[y * stride..(y + 1) * stride];
        let output = &mut dst[y * stride..(y + 1) * stride];
        box_blur_line(input, output, width, &params, edge_a, edge_b);
    }
}

/// One vertical box-blur pass. Pillow transposes, blurs horizontally and
/// transposes back; blurring each gathered column gives the same bytes.
fn vertical_pass(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    channels: usize,
    float_radius: f32,
) {
    let params = BoxParams::new(float_radius, channels);
    let edge_a = (params.radius + 1).min(height as i64) as usize;
    let edge_b = (height as i64 - params.radius - 1).max(0) as usize;
    let stride = width * channels;
    let mut line_in = vec![0u8; height * channels];
    let mut line_out = vec![0u8; height * channels];

    for x in 0..width {
        for y in 0..height {
            let offset = y * stride + x * channels;
            line_in[y * channels..(y + 1) * channels]
                .copy_from_slice(&src[offset..offset + channels]);
        }
        box_blur_line(&line_in, &mut line_out, height, &params, edge_a, edge_b);
        for y in 0..height {
            let offset = y * stride + x * channels;
            dst[offset..offset + channels]
                .copy_from_slice(&line_out[y * channels..(y + 1) * channels]);
        }
    }
}

/// Pillow `ImageFilter.GaussianBlur(radius)` on an interleaved buffer.
fn gaussian_blur(src: &[u8], width: u32, height: u32, channels: usize, radius: f32) -> Vec<u8> {
    // DECISION: Pillow raises ValueError for a negative or oversized radius.
    // The HUD only derives positive radii from its scale, so invalid values
    // return a copy instead of adding an error type outside the brief's files.
    if width == 0
        || height == 0
        || !radius.is_finite()
        || radius <= 0.0
        || radius >= 2_147_483_648.0
    {
        return src.to_vec();
    }
    let float_radius = gaussian_blur_radius(radius, PASSES);
    if float_radius == 0.0 {
        return src.to_vec();
    }
    let width = width as usize;
    let height = height as usize;
    let mut current = src.to_vec();
    let mut scratch = vec![0u8; src.len()];
    for _ in 0..PASSES {
        horizontal_pass(
            &current,
            &mut scratch,
            width,
            height,
            channels,
            float_radius,
        );
        std::mem::swap(&mut current, &mut scratch);
    }
    for _ in 0..PASSES {
        vertical_pass(
            &current,
            &mut scratch,
            width,
            height,
            channels,
            float_radius,
        );
        std::mem::swap(&mut current, &mut scratch);
    }
    current
}

impl Image {
    /// Pillow `image.filter(ImageFilter.GaussianBlur(radius))` for `RGBA`.
    ///
    /// Every channel, alpha included, is blurred independently with Pillow's
    /// three box passes.
    pub fn gaussian_blur(&self, radius: f32) -> Image {
        let data = gaussian_blur(self.as_raw(), self.width(), self.height(), 4, radius);
        Image::from_rgba(self.width(), self.height(), data)
            .expect("gaussian blur keeps the RGBA buffer length")
    }
}

impl Mask {
    /// Pillow `image.filter(ImageFilter.GaussianBlur(radius))` for mode `L`.
    pub fn gaussian_blur(&self, radius: f32) -> Mask {
        let data = gaussian_blur(self.as_raw(), self.width(), self.height(), 1, radius);
        Mask::from_raw(self.width(), self.height(), data)
            .expect("gaussian blur keeps the L buffer length")
    }
}

#[cfg(test)]
mod tests {
    use crate::raster::{Image, Mask};
    use crate::test_support::{fnv1a, unhex};

    const SOURCE_RGBA: &str = "207bf742fe0ce079dc4e0dd7c2cb8a707ae94c6cbc62caf6953f5eb65aead64825705d39ba5bab47e0905a4cc0874b735c1f4cd8f4d9b6c84daab4b969f9fc4fa4fc3feb7ce0bd8268fde8640ec4a661ddd94f83b3be580638cf5ae0dd3b3f78e14b69568f8be6251c0e96bb36e3a54b1d48881434f9e79cf6952ddd65ba97bf179b038a9453595ac15d837839ce0f0a80689911818bdd13561bbebdd28c9981cfc8bf519bf7efc956ebd8d434586851e0c0031725e475ef24ad65b80d7232f63f6d9e3e8c8b8652c52231f1ec3800d186a122e1b2c698f3ec455c8208743d28208424c54b2a66d0a12eddc5903f1e7f9c2ce03fe432cc28eae9b1a37bf94eb35d7dafe2afeb929871aec7d77339395721b2f2cb7c2c2d1487bd9a3466a9d316550c4ec3afde9bdbec5519e5473a66a1e30cd74438541d2dedcd737eeecc19b583c85c84b872c03cc45284ce55f438b0330708572688cd644b6cfd979c893cf837799dc6634435771f42d4b221e0b3b5bd1a6fec342d9554930f47b476082d9ffb41feb37b53b2fac5d44853bc891c9de8e8e09298fa05e97598d2ea955972e10331e28256e2d4c73935a0d7d3de26d88ae009d78b29b4afbec4190b3fc9ea48a3e3ed3341c86dcc8dfaa43bbba916a30ac0ba6e5ce2889bf32a1d95a793c4ae1143644aba1ad18dfe498f35b6e857f4b6d5dc2a093470e6e2f05354a43e54358075c8bbb3f5129529d8c39db4fc75e2743b4a697fa9db84a3e67549234bac0bae392c849a63c8b3b67e845271108588326b790a287a7da90dca00ec";
    const SOURCE_L: &str = "c9c613c85f047041b460057ffbfeffbf75e9ff60c8742c665765b75edc7c4f59feb354b95502d750c196f446a21bffc6972e2203ee0da966048c41458407447a9280e35edf3ef2adcde22c4ec2712eefcbc8de85d3ba46a171ce03508f394fa600b591";

    fn rgba_source() -> Image {
        Image::from_rgba(13, 11, unhex(SOURCE_RGBA)).expect("the oracle emitted a valid image")
    }

    fn mask_source() -> Mask {
        Mask::from_raw(11, 9, unhex(SOURCE_L)).expect("the oracle emitted a valid mask")
    }

    #[test]
    fn rgba_gaussian_blur_matches_pillow() {
        let cases = [
            (0.0, 0xb2fb_1d36_70f2_16f5),
            (0.1, 0x49cc_dd6d_9a64_6702),
            (0.5, 0x6217_dd8f_f5ad_b0e8),
            (1.0, 0x20e6_8e7b_74de_e328),
            (1.7, 0xb207_9248_5631_8517),
            (3.0, 0x3730_5fc5_b461_48a1),
            (4.0, 0x0996_2e89_3cdc_8870),
            (7.4, 0xeb6b_3348_19d1_445c),
            (12.0, 0x4b6c_b52a_65a1_b4ab),
        ];
        let source = rgba_source();
        for (radius, expected) in cases {
            let blurred = source.gaussian_blur(radius);
            assert_eq!((blurred.width(), blurred.height()), (13, 11));
            assert_eq!(fnv1a(blurred.as_raw()), expected, "radius {radius}");
        }
    }

    #[test]
    fn mask_gaussian_blur_matches_pillow() {
        let cases = [
            (0.0, 0xded1_4698_d602_eac5),
            (0.3, 0x70ce_2d7d_455a_5ac3),
            (2.2, 0xabf1_bbea_2de4_6d78),
            (4.7, 0x2824_e12f_c76e_96ed),
            (12.0, 0x09fd_2a1e_188f_438f),
        ];
        let source = mask_source();
        for (radius, expected) in cases {
            let blurred = source.gaussian_blur(radius);
            assert_eq!((blurred.width(), blurred.height()), (11, 9));
            assert_eq!(fnv1a(blurred.as_raw()), expected, "radius {radius}");
        }
    }

    #[test]
    fn invalid_radius_and_tiny_images_are_safe() {
        let source = rgba_source();
        assert_eq!(source.gaussian_blur(0.0), source);
        assert_eq!(source.gaussian_blur(-1.0), source);
        assert_eq!(source.gaussian_blur(f32::NAN), source);
        assert_eq!(source.gaussian_blur(f32::INFINITY), source);
        assert_eq!(source.gaussian_blur(2_147_483_648.0), source);

        let single = Image::new(1, 1, crate::raster::Rgba::new(10, 20, 30, 40));
        assert_eq!(single.gaussian_blur(1.5), single);
        let mask = Mask::new(1, 1, 9);
        assert_eq!(mask.gaussian_blur(4.0), mask);
    }
}
