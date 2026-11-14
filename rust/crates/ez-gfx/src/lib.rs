//! Raster and composition primitives for the HUD.
//!
//! This crate owns the RGBA8 image type that the rest of the graphics stack
//! consumes. The arithmetic that Pillow historically did for the HUD is ported
//! bit for bit: `Image.new`, `Image.fromarray`, `Image.alpha_composite`,
//! `Image.paste` (with an `L` mask), `Image.linear_gradient("L")`,
//! `Image.split`/`Image.merge`, and the `RGBA`/`RGBa` conversions.
//!
//! Images keep straight (non-premultiplied) alpha. Premultiplied frames, which
//! the layered Win32 window expects, are produced on demand with
//! [`Image::to_premultiplied_rgba`] and [`Image::to_bgra_premultiplied`].

use std::fmt;

pub mod blur;
pub mod brand;
pub mod composite;
pub mod draw;
pub mod icons;
pub mod raster;
pub mod resample;
pub mod sdf;
pub mod text;

pub use composite::{alpha_composite, linear_gradient};
pub use raster::{bgra8_premultiplied, Channel, ChannelView, ChannelViewMut, Image, Mask, Rgba};

/// Failure of a raster or composition helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxError {
    /// A raw buffer does not have the length implied by the dimensions.
    BufferLength {
        /// Bytes the dimensions require.
        expected: usize,
        /// Bytes received.
        got: usize,
    },
    /// Two operands that must share dimensions do not.
    SizeMismatch,
}

impl fmt::Display for GfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferLength { expected, got } => {
                write!(
                    f,
                    "buffer length {got} does not match the expected {expected}"
                )
            }
            Self::SizeMismatch => f.write_str("images do not match"),
        }
    }
}

impl std::error::Error for GfxError {}

#[cfg(test)]
pub(crate) mod test_support {
    /// FNV-1a over raw bytes, used to pin whole Pillow oracle tables.
    pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }

    /// FNV-1a over `f32` values in their little-endian byte form.
    pub(crate) fn fnv1a_f32(values: &[f32]) -> u64 {
        let mut bytes = Vec::with_capacity(values.len() * 4);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fnv1a(&bytes)
    }

    /// Decodes a lowercase hex string produced by the Python oracle.
    pub(crate) fn unhex(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|index| {
                u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                    .expect("the oracle emitted valid hex")
            })
            .collect()
    }
}
