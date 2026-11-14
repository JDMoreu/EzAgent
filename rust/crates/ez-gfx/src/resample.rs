//! Bit-exact port of Pillow's `Image.resize` for `LANCZOS`, `BICUBIC` and
//! `BILINEAR` (`Resample.c`).
//!
//! Pillow precomputes normalized filter coefficients in `f64`, quantizes them
//! to 22-bit fixed point and runs a horizontal and a vertical pass, choosing
//! which goes first exactly like `ImagingResampleInner` (`Resample.c`).
//! `RGBA` images are resampled premultiplied and converted back, exactly like
//! `Image.resize` does for every filter except `NEAREST`.
//!
//! `BOX`, `HAMMING`, `MKS2013` and `MKS2021` are not ported: no HUD call site
//! uses them and the brief only asks for the filters the HUD resizes with.

use std::f64::consts::PI;

use crate::raster::{Image, Mask, Rgba};

/// Number of fractional bits Pillow keeps from each filter coefficient.
const PRECISION_BITS: u32 = 22;
/// `1 << PRECISION_BITS` as the `f64` factor Pillow multiplies by.
const FIXED_ONE: f64 = 4_194_304.0;
/// Rounding term added before every accumulation (`1 << (PRECISION_BITS - 1)`).
const ROUND: i32 = 1 << (PRECISION_BITS - 1);

/// Pillow's `Image.Resampling` values that the HUD uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resampling {
    /// `Image.Resampling.BILINEAR` (used by the drop canvas for scaled tiles).
    Bilinear,
    /// `Image.Resampling.BICUBIC` (used by the fluid interior and the chat thumbnail).
    Bicubic,
    /// `Image.Resampling.LANCZOS` (used by the brand mark and its mask).
    Lanczos,
}

impl Resampling {
    /// Filter support in source pixels, before the scale factor.
    const fn support(self) -> f64 {
        match self {
            Self::Bilinear => 1.0,
            Self::Bicubic => 2.0,
            Self::Lanczos => 3.0,
        }
    }

    /// The filter kernel, copied from `Resample.c`.
    fn eval(self, x: f64) -> f64 {
        match self {
            Self::Bilinear => bilinear_filter(x),
            Self::Bicubic => bicubic_filter(x),
            Self::Lanczos => lanczos_filter(x),
        }
    }
}

fn bilinear_filter(x: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 {
        1.0 - x
    } else {
        0.0
    }
}

fn bicubic_filter(x: f64) -> f64 {
    const A: f64 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * A
    } else {
        0.0
    }
}

fn sinc_filter(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * PI;
    x.sin() / x
}

fn lanczos_filter(x: f64) -> f64 {
    if (-3.0..3.0).contains(&x) {
        sinc_filter(x) * sinc_filter(x / 3.0)
    } else {
        0.0
    }
}

/// Fixed-point coefficients for one axis, laid out as Pillow's `bounds`/`kk`.
struct Coeffs {
    ksize: usize,
    /// `(first source index, tap count)` for each output position.
    bounds: Vec<(usize, usize)>,
    taps: Vec<i32>,
}

impl Coeffs {
    /// Pillow `precompute_coeffs` followed by `normalize_coeffs_8bpc`, for a
    /// full-image box (`in0 = 0`, `in1 = in_size`).
    fn new(in_size: u32, out_size: u32, filter: Resampling) -> Self {
        let scale = f64::from(in_size) / f64::from(out_size);
        let filterscale = scale.max(1.0);
        let support = filter.support() * filterscale;
        let ksize = support.ceil() as usize * 2 + 1;
        let inv_filterscale = 1.0 / filterscale;
        let mut bounds = Vec::with_capacity(out_size as usize);
        let mut coefficients = vec![0.0; out_size as usize * ksize];

        for xx in 0..out_size as usize {
            let center = (xx as f64 + 0.5) * scale;
            let xmin = ((center - support + 0.5) as i64).max(0);
            let xmax = ((center + support + 0.5) as i64).min(i64::from(in_size));
            let count = (xmax - xmin).max(0) as usize;
            let row = &mut coefficients[xx * ksize..(xx + 1) * ksize];
            let mut sum = 0.0;
            for (x, coefficient) in row.iter_mut().take(count).enumerate() {
                let distance = (x as f64 + xmin as f64 - center + 0.5) * inv_filterscale;
                *coefficient = filter.eval(distance);
                sum += *coefficient;
            }
            if sum != 0.0 {
                for coefficient in row.iter_mut().take(count) {
                    *coefficient /= sum;
                }
            }
            bounds.push((xmin as usize, count));
        }

        let taps = coefficients.iter().map(|&value| quantize(value)).collect();
        Self {
            ksize,
            bounds,
            taps,
        }
    }
}

/// `normalize_coeffs_8bpc`: round both signs away from zero into `INT32`.
fn quantize(value: f64) -> i32 {
    if value < 0.0 {
        (-0.5 + value * FIXED_ONE) as i32
    } else {
        (0.5 + value * FIXED_ONE) as i32
    }
}

/// `clip8`: the lookup table is a clamp of the 22-bit fixed-point value.
fn clip8(accumulator: i32) -> u8 {
    (accumulator >> PRECISION_BITS).clamp(0, 255) as u8
}

/// One horizontal resampling pass. `dst` must be `out_width * height` pixels.
fn horizontal_pass(
    src: &[u8],
    in_width: u32,
    height: u32,
    out_width: u32,
    channels: usize,
    coeffs: &Coeffs,
    dst: &mut [u8],
) {
    let in_stride = in_width as usize * channels;
    let out_stride = out_width as usize * channels;
    for y in 0..height as usize {
        let input = &src[y * in_stride..(y + 1) * in_stride];
        let output = &mut dst[y * out_stride..(y + 1) * out_stride];
        for (ox, &(xmin, count)) in coeffs.bounds.iter().enumerate() {
            let taps = &coeffs.taps[ox * coeffs.ksize..];
            let mut accumulator = [ROUND; 4];
            for (x, &tap) in taps.iter().take(count).enumerate() {
                let source = &input[(xmin + x) * channels..];
                for (channel, &value) in source.iter().take(channels).enumerate() {
                    accumulator[channel] += i32::from(value) * tap;
                }
            }
            for (channel, &value) in accumulator.iter().take(channels).enumerate() {
                output[ox * channels + channel] = clip8(value);
            }
        }
    }
}

/// One vertical resampling pass. `dst` must be `width * out_height` pixels.
fn vertical_pass(src: &[u8], width: u32, channels: usize, coeffs: &Coeffs, dst: &mut [u8]) {
    let out_stride = width as usize * channels;
    for (oy, &(ymin, count)) in coeffs.bounds.iter().enumerate() {
        let taps = &coeffs.taps[oy * coeffs.ksize..];
        let output = &mut dst[oy * out_stride..(oy + 1) * out_stride];
        for ox in 0..width as usize {
            let mut accumulator = [ROUND; 4];
            for (y, &tap) in taps.iter().take(count).enumerate() {
                let source = &src[((ymin + y) * width as usize + ox) * channels..];
                for (channel, &value) in source.iter().take(channels).enumerate() {
                    accumulator[channel] += i32::from(value) * tap;
                }
            }
            for (channel, &value) in accumulator.iter().take(channels).enumerate() {
                output[ox * channels + channel] = clip8(value);
            }
        }
    }
}

/// Resamples an interleaved 8-bit buffer with 1 or 4 channels.
fn resample(
    src: &[u8],
    in_width: u32,
    in_height: u32,
    out_width: u32,
    out_height: u32,
    channels: usize,
    filter: Resampling,
) -> Vec<u8> {
    if out_width == 0 || out_height == 0 {
        return Vec::new();
    }
    if out_width == in_width && out_height == in_height {
        return src.to_vec();
    }

    // Pillow runs the vertical pass first when the height shrinks more than
    // twice as much as the width; the intermediate 8-bit rounding makes the
    // order observable, so it has to be reproduced.
    let width_shrink = i64::from(in_width) - i64::from(out_width);
    let height_shrink = i64::from(in_height) - i64::from(out_height);
    let vertical_first = height_shrink > 0 && height_shrink > width_shrink * 2;

    if out_width != in_width && !vertical_first {
        let horizontal = horizontal_resample(src, in_width, in_height, out_width, channels, filter);
        if out_height != in_height {
            vertical_resample(
                &horizontal,
                out_width,
                in_height,
                out_height,
                channels,
                filter,
            )
        } else {
            horizontal
        }
    } else if out_height != in_height {
        let vertical = vertical_resample(src, in_width, in_height, out_height, channels, filter);
        if out_width != in_width {
            horizontal_resample(&vertical, in_width, out_height, out_width, channels, filter)
        } else {
            vertical
        }
    } else {
        horizontal_resample(src, in_width, in_height, out_width, channels, filter)
    }
}

fn horizontal_resample(
    src: &[u8],
    in_width: u32,
    height: u32,
    out_width: u32,
    channels: usize,
    filter: Resampling,
) -> Vec<u8> {
    let coeffs = Coeffs::new(in_width, out_width, filter);
    let mut out = vec![0; out_width as usize * height as usize * channels];
    horizontal_pass(
        src, in_width, height, out_width, channels, &coeffs, &mut out,
    );
    out
}

fn vertical_resample(
    src: &[u8],
    width: u32,
    in_height: u32,
    out_height: u32,
    channels: usize,
    filter: Resampling,
) -> Vec<u8> {
    let coeffs = Coeffs::new(in_height, out_height, filter);
    let mut out = vec![0; width as usize * out_height as usize * channels];
    vertical_pass(src, width, channels, &coeffs, &mut out);
    out
}

impl Image {
    /// Pillow `Image.resize((width, height), resampling)`.
    ///
    /// `RGBA` is premultiplied before the passes and unpremultiplied after,
    /// matching `Image.resize`'s `RGBa` round trip.
    // DECISION: Pillow raises ValueError for a zero dimension; the HUD always
    // passes positive sizes, so this port returns an empty image instead of
    // adding an error type outside the brief's files.
    pub fn resize(&self, width: u32, height: u32, resampling: Resampling) -> Image {
        if width == 0 || height == 0 || self.width() == 0 || self.height() == 0 {
            return Image::new(width, height, Rgba::TRANSPARENT);
        }
        if width == self.width() && height == self.height() {
            return self.clone();
        }
        let premultiplied = resample(
            &self.to_premultiplied_rgba(),
            self.width(),
            self.height(),
            width,
            height,
            4,
            resampling,
        );
        Image::from_premultiplied_rgba(width, height, premultiplied)
            .expect("resampling keeps the RGBA buffer length")
    }
}

impl Mask {
    /// Pillow `Image.resize((width, height), resampling)` for mode `L`.
    pub fn resize(&self, width: u32, height: u32, resampling: Resampling) -> Mask {
        if width == 0 || height == 0 || self.width() == 0 || self.height() == 0 {
            return Mask::new(width, height, 0);
        }
        if width == self.width() && height == self.height() {
            return self.clone();
        }
        let out = resample(
            self.as_raw(),
            self.width(),
            self.height(),
            width,
            height,
            1,
            resampling,
        );
        Mask::from_raw(width, height, out).expect("resampling keeps the L buffer length")
    }
}

#[cfg(test)]
mod tests {
    use super::Resampling;
    use crate::raster::{Image, Mask, Rgba};
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
    fn rgba_resize_matches_pillow() {
        let cases = [
            (Resampling::Lanczos, 5, 4, 0x41d2_aff2_0a77_c903),
            (Resampling::Lanczos, 7, 4, 0xa43c_db14_63f0_683c),
            (Resampling::Lanczos, 17, 14, 0xe872_e7c1_e517_2c21),
            (Resampling::Lanczos, 5, 18, 0xb6ae_9865_61bc_5975),
            (Resampling::Lanczos, 20, 3, 0x9fb9_5d72_2a8f_2737),
            (Resampling::Lanczos, 26, 22, 0x7e42_19da_cca8_a2f3),
            (Resampling::Lanczos, 13, 5, 0x2be4_7367_626b_14ec),
            (Resampling::Lanczos, 7, 11, 0x93e3_64fd_ea2a_3eb8),
            (Resampling::Bicubic, 5, 4, 0x7108_bfd5_1de2_b2a7),
            (Resampling::Bicubic, 7, 4, 0x3af5_44f8_af37_875c),
            (Resampling::Bicubic, 17, 14, 0x3468_2e72_e96e_007f),
            (Resampling::Bicubic, 5, 18, 0xd24c_b0ff_55f4_ca59),
            (Resampling::Bicubic, 20, 3, 0x23d1_fef7_8943_8d9f),
            (Resampling::Bicubic, 26, 22, 0xeb29_dcbb_64c9_de2b),
            (Resampling::Bicubic, 13, 5, 0x319f_f8d0_6bcb_7fee),
            (Resampling::Bicubic, 7, 11, 0x2a94_b3e8_88bc_a921),
            (Resampling::Bilinear, 5, 4, 0x40a1_0ba8_f83c_5f44),
            (Resampling::Bilinear, 7, 4, 0x4a88_fb03_a37c_684d),
            (Resampling::Bilinear, 17, 14, 0x7473_4028_c3bf_fbed),
            (Resampling::Bilinear, 5, 18, 0x9cdd_1f33_81eb_e461),
            (Resampling::Bilinear, 20, 3, 0xe520_b7e6_6e04_7d04),
            (Resampling::Bilinear, 26, 22, 0x84a7_4857_5cd3_d148),
        ];
        let source = rgba_source();
        for (filter, width, height, expected) in cases {
            let resized = source.resize(width, height, filter);
            assert_eq!((resized.width(), resized.height()), (width, height));
            assert_eq!(
                fnv1a(resized.as_raw()),
                expected,
                "{filter:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn mask_resize_matches_pillow() {
        let cases = [
            (Resampling::Lanczos, 5, 4, 0x8065_6c8c_ecc3_e4ad),
            (Resampling::Lanczos, 7, 4, 0xe752_fa17_e743_0a73),
            (Resampling::Lanczos, 17, 14, 0x99ad_1fe3_4934_d305),
            (Resampling::Lanczos, 5, 18, 0xb38a_e789_8ed6_b0aa),
            (Resampling::Lanczos, 20, 3, 0x5f07_f3e4_4e46_af8f),
            (Resampling::Lanczos, 26, 22, 0xeba5_9235_51f3_f92b),
            (Resampling::Lanczos, 11, 4, 0x89d2_95e7_c96a_e9c2),
            (Resampling::Lanczos, 5, 9, 0xfdc0_54ce_8a72_6112),
            (Resampling::Bicubic, 5, 4, 0xf18d_5a0b_fd66_8085),
            (Resampling::Bicubic, 7, 4, 0x881b_c49c_5321_0591),
            (Resampling::Bicubic, 17, 14, 0x3cf1_fbda_21c4_17ff),
            (Resampling::Bicubic, 5, 18, 0xbeae_7d73_7fcd_1750),
            (Resampling::Bicubic, 20, 3, 0x1f4a_c96c_5a5f_76b1),
            (Resampling::Bicubic, 26, 22, 0xe3d3_9a02_e222_81bb),
            (Resampling::Bicubic, 11, 4, 0x97f0_35dc_49a4_581c),
            (Resampling::Bicubic, 5, 9, 0x666d_1fc4_9f2b_2fc0),
            (Resampling::Bilinear, 5, 4, 0xf2bc_1171_b0d6_f749),
            (Resampling::Bilinear, 7, 4, 0x0230_5123_4286_e88a),
            (Resampling::Bilinear, 17, 14, 0x7351_c191_002e_9134),
            (Resampling::Bilinear, 5, 18, 0xa8af_401a_48c3_93f0),
            (Resampling::Bilinear, 20, 3, 0x71ee_ed90_b20a_8e32),
            (Resampling::Bilinear, 26, 22, 0x6e25_38d3_e96e_e155),
            (Resampling::Bilinear, 11, 4, 0x18dc_7e33_3806_cafb),
            (Resampling::Bilinear, 5, 9, 0xce3f_ff2a_b2cc_fd97),
        ];
        let source = mask_source();
        for (filter, width, height, expected) in cases {
            let resized = source.resize(width, height, filter);
            assert_eq!((resized.width(), resized.height()), (width, height));
            assert_eq!(
                fnv1a(resized.as_raw()),
                expected,
                "{filter:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn rgba_resize_premultiplies_like_pillow() {
        let mut data = Vec::with_capacity(6 * 5 * 4);
        for index in 0..6 * 5 {
            let alpha = [0, 255, 1, 128, 254][index % 5];
            data.extend_from_slice(&[
                (index * 37 % 256) as u8,
                (index * 73 % 256) as u8,
                (index * 109 % 256) as u8,
                alpha,
            ]);
        }
        let source = Image::from_rgba(6, 5, data).expect("six times five pixels");
        let cases = [
            (Resampling::Lanczos, 3, 2, 0xee75_3804_e872_1e9e),
            (Resampling::Lanczos, 9, 8, 0x9612_270d_fdf5_266b),
            (Resampling::Lanczos, 4, 3, 0xec67_5848_7d7c_3d70),
            (Resampling::Bicubic, 3, 2, 0x74ef_8709_3bf8_d4d3),
            (Resampling::Bicubic, 9, 8, 0xa797_0791_a80e_a37a),
            (Resampling::Bicubic, 4, 3, 0xf4ee_4142_4603_957f),
            (Resampling::Bilinear, 3, 2, 0x90a4_e3d1_d06e_ea79),
            (Resampling::Bilinear, 9, 8, 0xef5c_fe22_b63a_5dd5),
            (Resampling::Bilinear, 4, 3, 0x8100_ca02_8da0_1bfe),
        ];
        for (filter, width, height, expected) in cases {
            let resized = source.resize(width, height, filter);
            assert_eq!(
                fnv1a(resized.as_raw()),
                expected,
                "{filter:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn identity_and_degenerate_sizes_are_safe() {
        let image = Image::new(4, 3, Rgba::new(9, 8, 7, 6));
        assert_eq!(image.resize(4, 3, Resampling::Lanczos), image);
        assert_eq!(image.resize(0, 3, Resampling::Lanczos).as_raw().len(), 0);
        assert_eq!(image.resize(4, 0, Resampling::Bicubic).as_raw().len(), 0);

        let empty = Image::new(0, 0, Rgba::TRANSPARENT);
        let grown = empty.resize(5, 2, Resampling::Lanczos);
        assert_eq!((grown.width(), grown.height()), (5, 2));
        assert!(grown.as_raw().iter().all(|&value| value == 0));

        let mask = Mask::new(4, 3, 5);
        assert_eq!(mask.resize(4, 3, Resampling::Bicubic), mask);
        assert_eq!(mask.resize(0, 3, Resampling::Bilinear).as_raw().len(), 0);
    }
}
