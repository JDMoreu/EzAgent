//! Unit tests of `brand.py` against vectors generated with Pillow 13.0.0 and
//! NumPy 2.x.

use crate::raster::Rgba;
use crate::test_support::fnv1a;

use super::{bbox, drops_coverage, drops_mask, mark_image, tile, BrandError, INK, INK_TOP, SVG};

fn mask_hash(mask: &crate::raster::Mask) -> u64 {
    fnv1a(mask.as_raw())
}

#[test]
fn inks_match_python() {
    assert_eq!(INK, [0x07, 0x16, 0x1B]);
    assert_eq!(INK_TOP, [0x0D, 0x2A, 0x31]);
}

#[test]
fn svg_matches_python_byte_for_byte() {
    assert_eq!(fnv1a(SVG.as_bytes()), 0x1048_e35c_e514_75c7);
    assert!(SVG.starts_with(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\" width=\"256\" height=\"256\">"
    ));
    assert!(SVG.ends_with("</svg>\n"));
    assert!(SVG.contains("<rect width=\"48\" height=\"48\" rx=\"10.56\""));
}

#[test]
fn drops_mask_and_bbox_match_pillow() {
    for (side, expected_bbox, expected_hash) in [
        (140, (31, 38, 108, 102), 0x10c2_b126_4d6d_6cba),
        (200, (44, 55, 155, 146), 0x2bed_0549_50bc_eb1d),
    ] {
        let mask = drops_mask(side);
        assert_eq!(bbox(&mask), Some(expected_bbox), "bbox at {side}");
        assert_eq!(mask_hash(&mask), expected_hash, "mask at {side}");
    }
}

#[test]
fn drops_coverage_matches_pillow() {
    for (size, expected) in [(16u32, 0x6391_33b7_8b13_4ea3), (64, 0x8979_ffa4_2527_d530)] {
        let coverage = drops_coverage(size).expect("valid size");
        assert_eq!(coverage.dim(), (size as usize, size as usize));
        assert_eq!(
            crate::test_support::fnv1a_f32(coverage.as_slice().expect("standard layout")),
            expected,
            "coverage at {size}"
        );
    }
    assert_eq!(drops_coverage(0), Err(BrandError::EmptySize));
    let first = drops_coverage(16).expect("valid size");
    let second = drops_coverage(16).expect("cached size");
    assert_eq!(first, second);
}

#[test]
fn fluid_frames_match_numpy() {
    for (size, expected) in [(64u32, 0xf78a_1dbb_6a22_0fab), (256, 0x5391_51d4_aff6_5041)] {
        let frame = super::fluid::fluid_frame(size);
        assert_eq!(fnv1a(&frame), expected, "fluid at {size}");
    }
}

#[test]
fn tiles_match_pillow() {
    for (side, expected) in [(24u32, 0x50f0_262c_8d81_f659), (128, 0x8883_1bd2_a0ec_0fad)] {
        let image = tile(side);
        assert_eq!(fnv1a(image.as_raw()), expected, "tile at {side}");
    }
    let image = tile(24);
    assert_eq!(image.pixel(0, 0), Rgba::TRANSPARENT);
    assert_eq!(image.pixel(12, 1), Rgba::new(12, 41, 48, 255));
    assert_eq!(image.pixel(12, 12), Rgba::new(9, 31, 37, 255));
}

#[test]
fn mark_image_matches_pillow() {
    let image = mark_image(16, true).expect("valid size");
    assert_eq!(fnv1a(image.as_raw()), 0x14be_111e_1b7d_cae9);
    assert_eq!(image.pixel(0, 0), Rgba::TRANSPARENT);
    assert_eq!(image.pixel(8, 8), Rgba::new(36, 138, 128, 255));

    let transparent = mark_image(16, false).expect("valid size");
    assert_eq!(fnv1a(transparent.as_raw()), 0xbc44_7970_1fed_e702);
    assert_eq!(transparent.pixel(0, 0), Rgba::TRANSPARENT);

    let image = mark_image(24, true).expect("valid size");
    assert_eq!(fnv1a(image.as_raw()), 0xefa1_067c_1469_d140);
    assert_eq!(image.pixel(0, 0), Rgba::new(0, 0, 85, 3));

    let image = mark_image(256, true).expect("valid size");
    assert_eq!(fnv1a(image.as_raw()), 0x8a6a_d4c9_2212_6b56);
    assert_eq!(image.pixel(128, 128), Rgba::new(9, 31, 37, 255));

    assert_eq!(mark_image(0, true), Err(BrandError::EmptySize));
}
