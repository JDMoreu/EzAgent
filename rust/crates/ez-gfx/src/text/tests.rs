//! Unit tests for the text port; vectors live in `vectors.rs`.

use super::vectors::{BASELINE, CAP, DRAWS, LENGTHS, MASKS, NEAR_MASK_HEX, SIZES};
use super::*;
use crate::test_support::{fnv1a, fnv1a_f32, unhex};

fn style_named(name: &str) -> FontStyle {
    font_style(name).expect("the vector names a known style")
}

#[test]
fn sizes_match_pillow() {
    for &(style, scale, text, width, height) in SIZES {
        let size = text_size(text, style, scale, None, "left").expect("valid size");
        assert_eq!(size, (width, height), "{style} {scale} {text:?}");
    }
}

#[test]
fn lengths_match_pillow() {
    for &(style, scale, text, expected) in LENGTHS {
        let font = font::load_font(style_named(style), scale);
        assert_eq!(
            font.length(text) / scale,
            expected,
            "{style} {scale} {text:?}"
        );
    }
}

#[test]
fn cap_height_and_baseline_match_pillow() {
    for &(style, scale, expected) in CAP {
        assert_eq!(
            cap_height(style, scale).expect("valid style"),
            expected,
            "{style} {scale}"
        );
    }
    for &(style, scale, text, expected) in BASELINE {
        assert_eq!(
            baseline_top(text, style, 100.0, scale).expect("valid style"),
            expected,
            "{style} {scale} {text:?}"
        );
    }
}

#[test]
fn masks_match_pillow() {
    for &(style, scale, text, width_px, line_h, width, height, hash) in MASKS {
        let style = style_named(style);
        let display = display_text(text, style);
        let bitmap = bitmap_for(&display, style, scale, style.tracking_em);
        assert_eq!(bitmap.width_px, width_px, "{display:?} {scale}");
        assert_eq!(bitmap.line_h_px, line_h, "{display:?} {scale}");
        assert_eq!(
            (bitmap.alpha.width(), bitmap.alpha.height()),
            (width, height),
            "{display:?} {scale}"
        );
        assert_eq!(fnv1a(bitmap.alpha.as_raw()), hash, "{display:?} {scale}");
    }
}

#[test]
fn masks_stay_within_the_free_type_version_delta() {
    let pillow = unhex(NEAR_MASK_HEX);
    let style = style_named("FS_ROW");
    let bitmap = bitmap_for("Hola", style, 1.0, style.tracking_em);
    let ours = bitmap.alpha.as_raw();
    assert_eq!(ours.len(), pillow.len());
    let mut max_delta = 0i32;
    let mut changed = 0usize;
    let mut sum_delta = 0i64;
    for (left, right) in ours.iter().zip(&pillow) {
        let delta = i32::from(*left) - i32::from(*right);
        max_delta = max_delta.max(delta.abs());
        if delta != 0 {
            changed += 1;
            sum_delta += i64::from(delta.abs());
        }
    }
    // FreeType 2.13.2 (vendored) against Pillow's 2.14.3: subpixel coverage
    // shifts only, with the same box, advances and line metrics.
    assert!(max_delta <= 16, "max delta {max_delta}");
    assert!(changed <= 64, "changed pixels {changed}");
    assert!(sum_delta <= 256, "absolute delta {sum_delta}");
}

#[test]
fn draws_match_pillow() {
    for &(text, style, color, x, y, scale, align, tracking, alpha, hash) in DRAWS {
        let mut data = vec![0.0f32; 64 * 140 * 4];
        let mut surface = FloatSurface::new(&mut data, 140, 64);
        let options = TextOptions {
            scale,
            align: Align::parse(align).expect("known align"),
            tracking,
            alpha,
        };
        let result = draw_text(
            &mut surface,
            text,
            style,
            Color::Rgba(color),
            (x, y),
            options,
        )
        .expect("valid draw");
        assert!(result.0 > 0.0);
        assert_eq!(fnv1a_f32(surface.data), hash, "{text} {style}");
    }
}

#[test]
fn style_tokens_resolve_like_python() {
    assert_eq!(resolve_style(Style::Named("FS_BODY")), Ok(FS_BODY));
    assert_eq!(resolve_style(Style::Named("body")), Ok(FS_BODY));
    assert_eq!(resolve_style(Style::Named(" body ")), Ok(FS_BODY));
    assert_eq!(resolve_style(Style::Named("fs_micro")), Ok(FS_MICRO));
    assert_eq!(
        resolve_style(Style::Named("nope")),
        Err(TextError::UnknownStyle("nope".to_string()))
    );
    assert_eq!(
        TextError::UnknownStyle("nope".to_string()).to_string(),
        "estilo de texto desconocido: 'nope'"
    );
    assert_eq!(font_style("FS_TITLE"), Some(FS_TITLE));
    assert_eq!(font_style("FS_NOPE"), None);
}

#[test]
fn errors_match_the_python_texts() {
    assert_eq!(
        text_size("x", "FS_BODY", 0.0, None, "left"),
        Err(TextError::InvalidScale)
    );
    assert_eq!(
        text_size("x", "FS_BODY", 1.0, None, "middle"),
        Err(TextError::UnknownAlign("middle".to_string()))
    );
    assert_eq!(
        TextError::UnknownAlign("middle".to_string()).to_string(),
        "alineación desconocida: 'middle'"
    );
    assert_eq!(
        text_size("x", "FS_BODY", 1.0, Some(f64::NAN), "left"),
        Err(TextError::NonFiniteTracking)
    );
    assert_eq!(
        text_size("x", "FS_BODY", 1.0, Some(f64::INFINITY), "left"),
        Err(TextError::NonFiniteTracking)
    );

    let mut data = vec![0.0f32; 3];
    let mut surface = FloatSurface::new(&mut data, 2, 2);
    assert_eq!(
        draw_text(
            &mut surface,
            "x",
            "FS_BODY",
            Color::Rgb([1, 2, 3]),
            (0.0, 0.0),
            TextOptions::default()
        ),
        Err(TextError::InvalidBuffer)
    );
    let mut data = vec![0.0f32; 16];
    let mut surface = FloatSurface::new(&mut data, 2, 2);
    let options = TextOptions {
        alpha: 1.5,
        ..TextOptions::default()
    };
    assert_eq!(
        draw_text(
            &mut surface,
            "x",
            "FS_BODY",
            Color::Rgb([1, 2, 3]),
            (0.0, 0.0),
            options
        ),
        Err(TextError::InvalidAlpha)
    );
}

#[test]
fn spaces_keep_the_advance_and_line_height() {
    let (width, height) = text_size("  ", "FS_BODY", 1.0, None, "left").expect("size");
    assert_eq!(width, 6.0);
    assert_eq!(height, 18.0);

    let (width, height) = text_size("", "FS_BODY", 1.0, None, "left").expect("size");
    assert_eq!((width, height), (0.0, 18.0));

    let mut data = vec![0.0f32; 20 * 20 * 4];
    let mut surface = FloatSurface::new(&mut data, 20, 20);
    let result = draw_text(
        &mut surface,
        "",
        "FS_BODY",
        Color::Rgb([255, 255, 255]),
        (3.0, 3.0),
        TextOptions::default(),
    )
    .expect("draw");
    assert_eq!(result, (0.0, 0.0));
    assert!(surface.data.iter().all(|value| *value == 0.0));
}

#[test]
fn micro_style_uppercases_and_tracks() {
    let (plain, _) = text_size("hola", "FS_MICRO", 1.0, None, "left").expect("size");
    let (upper, _) = text_size("HOLA", "FS_MICRO", 1.0, None, "left").expect("size");
    let (wide, _) = text_size("HOLA", "FS_MICRO", 1.0, Some(0.5), "left").expect("size");
    assert_eq!(plain, upper);
    assert!(wide > upper);

    let (tracked, _) = text_size("HOLA", "FS_BODY", 1.0, Some(0.2), "left").expect("size");
    let (normal, _) = text_size("HOLA", "FS_BODY", 1.0, None, "left").expect("size");
    assert!(tracked > normal);
}

#[test]
fn alignment_moves_the_anchor_and_clips() {
    let mut data = vec![0.0f32; 20 * 20 * 4];
    let mut surface = FloatSurface::new(&mut data, 20, 20);
    let left = draw_text(
        &mut surface,
        "AV",
        "FS_BODY",
        Color::Rgb([255, 255, 255]),
        (8.0, 5.0),
        TextOptions::default(),
    )
    .expect("draw");
    let left_ink: Vec<usize> = surface
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| pixel[3] > 0.0)
        .map(|(index, _)| index)
        .collect();
    assert!(!left_ink.is_empty());
    assert_eq!(left, (18.0, 9.0));

    let mut data = vec![0.0f32; 20 * 20 * 4];
    let mut surface = FloatSurface::new(&mut data, 20, 20);
    let options = TextOptions {
        align: Align::Right,
        ..TextOptions::default()
    };
    let right = draw_text(
        &mut surface,
        "AV",
        "FS_BODY",
        Color::Rgb([255, 255, 255]),
        (8.0, 5.0),
        options,
    )
    .expect("draw");
    assert_eq!(right, left);
    let right_ink: Vec<usize> = surface
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| pixel[3] > 0.0)
        .map(|(index, _)| index)
        .collect();
    assert!(right_ink.iter().all(|index| index % 20 < 9));
    assert!(right_ink.iter().any(|index| index % 20 < 8));
}

#[test]
fn clear_cache_keeps_the_same_output() {
    let first = text_size("Hola", "FS_BODY", 1.5, None, "left").expect("size");
    clear_cache();
    let second = text_size("Hola", "FS_BODY", 1.5, None, "left").expect("size");
    assert_eq!(first, second);
}

#[test]
fn color_helpers_match_the_two_python_shapes() {
    assert_eq!(Color::Rgb([1, 2, 3]).rgb(), [1, 2, 3]);
    assert_eq!(Color::Rgba([1, 2, 3, 4]).rgb(), [1, 2, 3]);
    assert_eq!(Color::Rgb([1, 2, 3]).alpha(), 255);
    assert_eq!(Color::Rgba([1, 2, 3, 4]).alpha(), 4);
}
