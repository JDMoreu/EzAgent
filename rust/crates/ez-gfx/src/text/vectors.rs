//! Pillow 13.0.0 / FreeType 2.14.3 vectors embedded in the unit tests.

mod draws;
mod masks;
mod metrics;
mod sizes;

pub(crate) use draws::{DRAWS, NEAR_MASK_HEX};
pub(crate) use masks::MASKS;
pub(crate) use metrics::{BASELINE, CAP, LENGTHS};
pub(crate) use sizes::SIZES;
