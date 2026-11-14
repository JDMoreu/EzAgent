//! RGBA8 raster type and the uint8/float32 conversions the HUD uses.
//!
//! [`Image`] mirrors Pillow's `RGBA` mode: straight alpha, four bytes per
//! pixel, row-major. [`Mask`] mirrors `L` mode (one byte per pixel) and is what
//! `Image.paste` uses as a matte. Both map directly to the bytes the HUD hands
//! to `numpy` today, so the conversions here reproduce those numpy idioms.

use crate::GfxError;

mod views;

pub use views::{Channel, ChannelRow, ChannelRowMut, ChannelView, ChannelViewMut};

/// One straight-alpha RGBA pixel, one byte per channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (`255` opaque).
    pub a: u8,
}

impl Rgba {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    /// Builds a pixel from its four channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Builds an opaque pixel.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Builds a pixel from a raw `[r, g, b, a]` tuple.
    pub const fn from_array(pixel: [u8; 4]) -> Self {
        Self::new(pixel[0], pixel[1], pixel[2], pixel[3])
    }

    /// Returns the pixel as a raw `[r, g, b, a]` tuple.
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl From<[u8; 4]> for Rgba {
    fn from(pixel: [u8; 4]) -> Self {
        Self::from_array(pixel)
    }
}

impl From<Rgba> for [u8; 4] {
    fn from(pixel: Rgba) -> Self {
        pixel.to_array()
    }
}

/// Single-channel 8-bit raster (Pillow mode `L`): a mask or coverage plane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Mask {
    /// Creates a `width`×`height` mask filled with `value`.
    pub fn new(width: u32, height: u32, value: u8) -> Self {
        Self {
            width,
            height,
            data: vec![value; pixel_len(width, height, 1)],
        }
    }

    /// Wraps an existing row-major buffer, checking its length.
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Result<Self, GfxError> {
        check_len(width, height, 1, data.len())?;
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Raw row-major bytes.
    pub fn as_raw(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw row-major bytes.
    pub fn as_raw_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Consumes the mask and returns its buffer.
    pub fn into_raw(self) -> Vec<u8> {
        self.data
    }

    /// Value at `(x, y)`.
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.row(y)[x as usize]
    }

    /// Writes the value at `(x, y)`.
    pub fn set(&mut self, x: u32, y: u32, value: u8) {
        self.row_mut(y)[x as usize] = value;
    }

    /// One row, exactly `width` bytes.
    pub fn row(&self, y: u32) -> &[u8] {
        let stride = self.width as usize;
        &self.data[y as usize * stride..(y as usize + 1) * stride]
    }

    /// One mutable row, exactly `width` bytes.
    pub fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = self.width as usize;
        &mut self.data[y as usize * stride..(y as usize + 1) * stride]
    }
}

/// Straight-alpha RGBA8 image, row-major, four bytes per pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Image {
    /// Creates a `width`×`height` image filled with `color` (`Image.new`).
    pub fn new(width: u32, height: u32, color: Rgba) -> Self {
        let mut data = vec![0; pixel_len(width, height, 4)];
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&color.to_array());
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Wraps an `RGBA` byte buffer (`Image.fromarray(data, "RGBA")`).
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Result<Self, GfxError> {
        check_len(width, height, 4, data.len())?;
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Expands an `RGB` byte buffer to opaque RGBA (`Image.fromarray(data, "RGB")`).
    pub fn from_rgb(width: u32, height: u32, data: Vec<u8>) -> Result<Self, GfxError> {
        check_len(width, height, 3, data.len())?;
        let mut out = Vec::with_capacity(pixel_len(width, height, 4));
        for pixel in data.as_chunks::<3>().0 {
            out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        Ok(Self {
            width,
            height,
            data: out,
        })
    }

    /// Interprets `data` as premultiplied `RGBa` and returns straight `RGBA`
    /// bytes, as `Image.fromarray(data, "RGBa").convert("RGBA")` does.
    pub fn from_premultiplied_rgba(
        width: u32,
        height: u32,
        mut data: Vec<u8>,
    ) -> Result<Self, GfxError> {
        check_len(width, height, 4, data.len())?;
        unpremultiply_rgba(&mut data);
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Raw row-major `RGBA` bytes.
    pub fn as_raw(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw row-major `RGBA` bytes.
    pub fn as_raw_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Consumes the image and returns its buffer.
    pub fn into_raw(self) -> Vec<u8> {
        self.data
    }

    /// Pixel at `(x, y)`.
    pub fn pixel(&self, x: u32, y: u32) -> Rgba {
        let row = self.row(y);
        let index = x as usize * 4;
        Rgba::from_array([row[index], row[index + 1], row[index + 2], row[index + 3]])
    }

    /// Writes `color` at `(x, y)`.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        let row = self.row_mut(y);
        let index = x as usize * 4;
        row[index..index + 4].copy_from_slice(&color.to_array());
    }

    /// Iterates pixels in row-major order.
    pub fn pixels(&self) -> impl Iterator<Item = Rgba> + '_ {
        self.data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| Rgba::from(*pixel))
    }

    /// One row, exactly `width * 4` bytes.
    pub fn row(&self, y: u32) -> &[u8] {
        let stride = self.width as usize * 4;
        &self.data[y as usize * stride..(y as usize + 1) * stride]
    }

    /// One mutable row, exactly `width * 4` bytes.
    pub fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = self.width as usize * 4;
        &mut self.data[y as usize * stride..(y as usize + 1) * stride]
    }

    /// Read-only view of one channel.
    pub fn channel(&self, channel: Channel) -> ChannelView<'_> {
        ChannelView {
            data: &self.data,
            width: self.width,
            height: self.height,
            channel,
        }
    }

    /// Mutable view of one channel.
    pub fn channel_mut(&mut self, channel: Channel) -> ChannelViewMut<'_> {
        ChannelViewMut {
            data: &mut self.data,
            width: self.width,
            height: self.height,
            channel,
        }
    }

    /// Splits the image into four `L` masks (`Image.split`).
    pub fn split(&self) -> [Mask; 4] {
        std::array::from_fn(|index| {
            let view = self.channel(Channel::ALL[index]);
            let mut data = Vec::with_capacity(pixel_len(self.width, self.height, 1));
            for y in 0..self.height {
                data.extend(view.row(y).iter());
            }
            Mask {
                width: self.width,
                height: self.height,
                data,
            }
        })
    }

    /// Builds an `RGBA` image from four `L` masks (`Image.merge`).
    pub fn merge(bands: [Mask; 4]) -> Result<Self, GfxError> {
        let width = bands[0].width;
        let height = bands[0].height;
        if bands
            .iter()
            .any(|band| band.width != width || band.height != height)
        {
            return Err(GfxError::SizeMismatch);
        }
        let mut data = Vec::with_capacity(pixel_len(width, height, 4));
        for y in 0..height {
            let rows: [&[u8]; 4] = std::array::from_fn(|index| bands[index].row(y));
            for (((red, green), blue), alpha) in
                rows[0].iter().zip(rows[1]).zip(rows[2]).zip(rows[3])
            {
                data.extend_from_slice(&[*red, *green, *blue, *alpha]);
            }
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Premultiplies the colors: Pillow's `RGBA` → `RGBa` conversion.
    pub fn to_premultiplied_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len());
        for pixel in self.data.as_chunks::<4>().0 {
            let alpha = pixel[3];
            out.extend_from_slice(&[
                muldiv255(pixel[0], alpha),
                muldiv255(pixel[1], alpha),
                muldiv255(pixel[2], alpha),
                alpha,
            ]);
        }
        out
    }

    /// Premultiplied BGRA bytes, the format the layered Win32 window expects.
    pub fn to_bgra_premultiplied(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len());
        for pixel in self.data.as_chunks::<4>().0 {
            let alpha = pixel[3];
            out.extend_from_slice(&[
                muldiv255(pixel[2], alpha),
                muldiv255(pixel[1], alpha),
                muldiv255(pixel[0], alpha),
                alpha,
            ]);
        }
        out
    }

    /// Straight `f32` RGBA in `0..=1` (`np.asarray(img, np.float32) / 255.0`).
    pub fn to_f32(&self) -> Vec<f32> {
        self.data.iter().map(|&v| f32::from(v) / 255.0).collect()
    }

    /// Premultiplied `f32` RGBA in `0..=1`, as the HUD prepares a frame.
    pub fn to_f32_premultiplied(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.data.len());
        for pixel in self.data.as_chunks::<4>().0 {
            let alpha = f32::from(pixel[3]) / 255.0;
            out.push(f32::from(pixel[0]) / 255.0 * alpha);
            out.push(f32::from(pixel[1]) / 255.0 * alpha);
            out.push(f32::from(pixel[2]) / 255.0 * alpha);
            out.push(alpha);
        }
        out
    }

    /// Quantizes straight `f32` RGBA with `(clip(x, 0, 1) * 255 + 0.5) as u8`.
    pub fn from_f32_rgba(width: u32, height: u32, values: &[f32]) -> Result<Self, GfxError> {
        check_len(width, height, 4, values.len())?;
        let data = values.iter().map(|&v| quantize(v)).collect();
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Quantizes premultiplied `f32` RGBA and returns the straight image, as
    /// `Image.fromarray((clip(out) * 255 + 0.5).astype(uint8), "RGBa").convert("RGBA")`.
    pub fn from_f32_premultiplied_rgba(
        width: u32,
        height: u32,
        values: &[f32],
    ) -> Result<Self, GfxError> {
        check_len(width, height, 4, values.len())?;
        let data = values.iter().map(|&v| quantize(v)).collect();
        Self::from_premultiplied_rgba(width, height, data)
    }
}

fn pixel_len(width: u32, height: u32, channels: usize) -> usize {
    width as usize * height as usize * channels
}

fn check_len(width: u32, height: u32, channels: usize, len: usize) -> Result<(), GfxError> {
    let expected = pixel_len(width, height, channels);
    if len == expected {
        Ok(())
    } else {
        Err(GfxError::BufferLength { expected, got: len })
    }
}

/// Quantizes a premultiplied `f32` RGBA buffer to BGRA bytes, the frame format
/// the layered Win32 window consumes.
///
/// The f32 values are clipped to `0..=1`, scaled with `(x * 255 + 0.5) as u8`
/// and reordered to B, G, R, A, exactly like the HUD's numpy frame tail.
pub fn bgra8_premultiplied(width: u32, height: u32, values: &[f32]) -> Result<Vec<u8>, GfxError> {
    check_len(width, height, 4, values.len())?;
    let mut out = Vec::with_capacity(values.len());
    for pixel in values.as_chunks::<4>().0 {
        out.extend_from_slice(&[
            quantize(pixel[2]),
            quantize(pixel[1]),
            quantize(pixel[0]),
            quantize(pixel[3]),
        ]);
    }
    Ok(out)
}

/// Pillow's `MULDIV255`: `(value * alpha + 128)` divided by 255 with carries.
pub(crate) const fn muldiv255(value: u8, alpha: u8) -> u8 {
    div255(value as u32 * alpha as u32)
}

/// Pillow's `DIV255` over an already multiplied value.
pub(crate) const fn div255(value: u32) -> u8 {
    let tmp = value + 128;
    (((tmp >> 8) + tmp) >> 8) as u8
}

fn unpremultiply_rgba(data: &mut [u8]) {
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = pixel[3] as u32;
        if alpha != 0 && alpha != 255 {
            for value in &mut pixel[..3] {
                *value = ((255 * *value as u32) / alpha).min(255) as u8;
            }
        }
    }
}

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::{Image, Mask, Rgba};
    use crate::test_support::{fnv1a, fnv1a_f32, unhex};

    const IMG2: &str = "a4f8627fc4b9d65004a95098ce9aefb2b9316dae64a3a4d52d7e28f282337a7dbcb3e0e42bc38557c02d074ff13cbcc7abfb4b82e735385586d1cdabf60a5365030203c62900eff7bebcbffa96c0953ba70f14277fad5ce788ee054331414883ba019b0fac6fe14cf55bf26d9228f9b7ff8cffacee6e8dfc7b96f9a72f9b9be2f73c07153995ecfb9d98bd63b64e63e9a88b09284fa5855cbe36928dca27448f34bf155aac941847";
    const IMG3: &str = "5b09e63c1ec9e77671e0240b13a98c9eb6b273c82270289cbddc546b92f367563b39eb54f48e32562835810a48836bfc";

    fn image(width: u32, height: u32, hex: &str) -> Image {
        Image::from_rgba(width, height, unhex(hex)).expect("the oracle emitted a valid image")
    }

    #[test]
    fn new_fills_every_pixel_and_checks_lengths() {
        let image = Image::new(2, 3, Rgba::opaque(1, 2, 3));
        assert_eq!((image.width(), image.height()), (2, 3));
        assert!(image.pixels().all(|pixel| pixel == Rgba::opaque(1, 2, 3)));
        assert_eq!(image.as_raw().len(), 2 * 3 * 4);

        let mut editable = image.clone();
        editable.set_pixel(1, 2, Rgba::new(4, 5, 6, 7));
        assert_eq!(editable.pixel(1, 2), Rgba::new(4, 5, 6, 7));
        assert_eq!(editable.row(2)[4..8], [4, 5, 6, 7]);

        assert_eq!(
            Image::from_rgba(2, 2, vec![0; 15]).expect_err("short buffer"),
            crate::GfxError::BufferLength {
                expected: 16,
                got: 15
            }
        );
        assert!(Image::from_rgb(1, 1, vec![1, 2, 3, 4]).is_err());
    }

    #[test]
    fn from_rgb_expands_to_opaque_alpha() {
        let image = Image::from_rgb(2, 1, vec![1, 2, 3, 4, 5, 6]).expect("six bytes");
        assert_eq!(image.pixel(0, 0), Rgba::opaque(1, 2, 3));
        assert_eq!(image.pixel(1, 0), Rgba::opaque(4, 5, 6));
    }

    #[test]
    fn masks_validate_and_expose_rows() {
        let mut mask = Mask::new(3, 2, 10);
        assert_eq!((mask.width(), mask.height()), (3, 2));
        assert_eq!(mask.get(2, 1), 10);
        mask.set(1, 0, 7);
        assert_eq!(mask.row(0), [10, 7, 10]);
        mask.as_raw_mut()[5] = 9;
        assert_eq!(mask.get(2, 1), 9);
        assert_eq!(mask.into_raw().len(), 6);
        assert!(Mask::from_raw(2, 2, vec![0; 3]).is_err());
    }

    #[test]
    fn premultiply_matches_pillow_exhaustive_table() {
        let mut data = Vec::with_capacity(256 * 256 * 4);
        for value in 0..256u32 {
            for alpha in 0..256u32 {
                data.extend_from_slice(&[
                    value as u8,
                    (255 - value) as u8,
                    (value / 3) as u8,
                    alpha as u8,
                ]);
            }
        }
        let image = Image::from_rgba(256, 256, data).expect("the table is complete");
        let premultiplied = image.to_premultiplied_rgba();
        assert_eq!(fnv1a(&premultiplied), 0x7abc_4c56_ba62_5a35);
        let back = Image::from_premultiplied_rgba(256, 256, premultiplied).expect("full buffer");
        assert_eq!(fnv1a(back.as_raw()), 0x2a01_f70a_82cc_3273);
    }

    #[test]
    fn float_conversions_match_numpy() {
        let values: Vec<f32> = (0..140)
            .map(|index| index as f32 / 139.0 * 1.4 - 0.2)
            .collect();
        assert_eq!(fnv1a_f32(&values), 0xf612_c143_3225_b386);
        let quantized = Image::from_f32_rgba(7, 5, &values).expect("140 floats");
        assert_eq!(fnv1a(quantized.as_raw()), 0xfb99_8d70_0236_7169);
        let straight = Image::from_f32_premultiplied_rgba(7, 5, &values).expect("140 floats");
        assert_eq!(fnv1a(straight.as_raw()), 0x646b_3265_cffd_5438);

        let source = image(7, 6, IMG2);
        assert_eq!(fnv1a_f32(&source.to_f32()), 0x0429_39f8_2d93_ca8a);
        assert_eq!(
            fnv1a_f32(&source.to_f32_premultiplied()),
            0x0eac_fe74_56b0_8cfb
        );
        assert_eq!(
            fnv1a(&source.to_bgra_premultiplied()),
            0x8310_7a6f_8a9e_d546
        );
        let bgra = super::bgra8_premultiplied(7, 5, &values).expect("140 floats");
        assert_eq!(fnv1a(&bgra), 0xecaf_ce17_c937_2289);
    }

    #[test]
    fn split_and_merge_round_trip() {
        let image = image(4, 3, IMG3);
        let bands = image.split();
        let hashes = [
            0xbf5b_4a1a_fdc8_1ebc,
            0xdacd_a31a_d9b4_7998,
            0x2036_5f95_c253_2561,
            0x9849_50bb_c1df_bd43,
        ];
        for (band, expected) in bands.iter().zip(hashes) {
            assert_eq!(fnv1a(band.as_raw()), expected);
        }
        let merged = Image::merge(bands).expect("bands share dimensions");
        assert_eq!(merged, image);
        let mut other = image.split();
        other[3] = Mask::new(3, 3, 0);
        assert!(Image::merge(other).is_err());
    }
}
