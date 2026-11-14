//! Unit tests for the SDF mask helpers.

use std::sync::Arc;

use super::oracle::CASES;
use super::{
    cached_masks, composite_band, fill_circle, fill_round_gradient, fill_round_rect_opts,
    fill_vertical_gradient, fit_text, round_mask, stroke_round_rect, FillOptions, Layer,
};
use crate::raster::Rgba;
use crate::test_support::{fnv1a_f32, unhex};

fn cases(kind: &str) -> Vec<Vec<&'static str>> {
    CASES
        .lines()
        .map(|line| line.split('|').collect::<Vec<_>>())
        .filter(|fields| fields[0] == kind)
        .collect()
}

fn layer(width: u32, height: u32, salt: usize) -> Layer {
    let data = (0..width as usize * height as usize * 4)
        .map(|index| ((index * 37 + 11 + salt) % 256) as f32 / 255.0)
        .collect();
    Layer::from_raw(width, height, data).expect("the oracle emitted a valid layer")
}

fn number(text: &str) -> f64 {
    text.parse().expect("the oracle emitted numbers")
}

fn bounds(text: &str) -> [f64; 4] {
    let values: Vec<f64> = text.split(',').map(number).collect();
    [values[0], values[1], values[2], values[3]]
}

fn pair(text: &str) -> (f64, f64) {
    let values: Vec<f64> = text.split(',').map(number).collect();
    (values[0], values[1])
}

fn color(text: &str) -> Rgba {
    let values: Vec<u8> = text
        .split(':')
        .map(|value| value.parse().expect("the oracle emitted channels"))
        .collect();
    Rgba::new(values[0], values[1], values[2], values[3])
}

fn optional(text: &str) -> Option<f64> {
    if text == "-" {
        None
    } else {
        Some(number(text))
    }
}

fn digest(text: &str) -> u64 {
    u64::from_str_radix(text, 16).expect("the oracle emitted a hash")
}

fn size(fields: &[&str]) -> (u32, u32) {
    (
        fields[1].parse().expect("the oracle emitted a width"),
        fields[2].parse().expect("the oracle emitted a height"),
    )
}

fn stops(text: &str) -> Vec<[f32; 3]> {
    text.split(';')
        .map(|stop| {
            let values: Vec<f32> = stop
                .split(':')
                .map(|value| value.parse().expect("the oracle emitted stops"))
                .collect();
            [values[0], values[1], values[2]]
        })
        .collect()
}

#[test]
fn fill_round_rect_matches_python() {
    let cases = cases("rrect");
    assert!(cases.len() >= 40);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut buf = layer(width, height, 0);
        fill_round_rect_opts(
            &mut buf,
            bounds(fields[3]),
            number(fields[4]),
            color(fields[7]),
            number(fields[8]),
            number(fields[9]),
            FillOptions {
                radius_top: optional(fields[5]),
                radius_bottom: optional(fields[6]),
                replace: fields[10] == "1",
            },
        );
        assert_eq!(fnv1a_f32(buf.as_raw()), digest(fields[11]), "{fields:?}");
    }
}

#[test]
fn round_gradient_matches_python() {
    let cases = cases("grad");
    assert!(cases.len() >= 20);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut buf = layer(width, height, 0);
        fill_round_gradient(
            &mut buf,
            bounds(fields[3]),
            number(fields[4]),
            &stops(fields[5]),
            number(fields[6]),
            number(fields[7]),
        );
        assert_eq!(fnv1a_f32(buf.as_raw()), digest(fields[8]), "{fields:?}");
    }
}

#[test]
fn stroke_matches_python() {
    let cases = cases("stroke");
    assert!(cases.len() >= 20);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut buf = layer(width, height, 0);
        stroke_round_rect(
            &mut buf,
            bounds(fields[3]),
            number(fields[4]),
            color(fields[5]),
            number(fields[6]),
            number(fields[7]),
        );
        assert_eq!(fnv1a_f32(buf.as_raw()), digest(fields[8]), "{fields:?}");
    }
}

#[test]
fn circle_matches_python() {
    let cases = cases("circle");
    assert!(cases.len() >= 20);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut buf = layer(width, height, 0);
        let (cx, cy) = pair(fields[3]);
        fill_circle(
            &mut buf,
            cx,
            cy,
            number(fields[4]),
            color(fields[5]),
            number(fields[6]),
            number(fields[7]),
        );
        assert_eq!(fnv1a_f32(buf.as_raw()), digest(fields[8]), "{fields:?}");
    }
}

#[test]
fn vertical_gradient_matches_python() {
    let cases = cases("vgrad");
    assert!(cases.len() >= 20);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut buf = layer(width, height, 0);
        fill_vertical_gradient(
            &mut buf,
            bounds(fields[3]),
            color(fields[4]),
            number(fields[5]),
            number(fields[6]),
            number(fields[7]),
        );
        assert_eq!(fnv1a_f32(buf.as_raw()), digest(fields[8]), "{fields:?}");
    }
}

#[test]
fn composite_band_matches_python() {
    let cases = cases("band");
    assert!(cases.len() >= 20);
    for fields in cases {
        let (width, height) = size(&fields);
        let mut dst = layer(width, height, 0);
        let src = layer(width, height, 53);
        let (row0, row1) = pair(fields[3]);
        composite_band(
            &mut dst,
            &src,
            row0,
            row1,
            number(fields[4]),
            number(fields[5]),
            optional(fields[6]),
        )
        .expect("the oracle used matching layers");
        assert_eq!(fnv1a_f32(dst.as_raw()), digest(fields[7]), "{fields:?}");
    }
}

#[test]
fn fit_text_matches_python() {
    let cases = cases("text");
    assert!(cases.len() >= 80);
    for fields in cases {
        let text = decode(fields[2]);
        let expected = decode(fields[3]);
        let measured = fit_text(&text, number(fields[1]), |value| {
            value.chars().count() as f64 * 10.0
        });
        assert_eq!(measured, expected, "text={text:?} max={}", fields[1]);
    }
}

fn decode(hex: &str) -> String {
    if hex == "-" {
        String::new()
    } else {
        String::from_utf8(unhex(hex)).expect("the oracle emitted UTF-8")
    }
}

#[test]
fn masks_are_cached_and_bounded() {
    let box_ = [3.25, 4.5, 40.75, 33.5];
    let first = round_mask(60, 60, box_, 6.0, 6.0, 1.5, false).expect("visible box");
    let second = round_mask(60, 60, box_, 6.0, 6.0, 1.5, false).expect("visible box");
    assert!(Arc::ptr_eq(&first, &second));
    assert!(first
        .coverage
        .iter()
        .all(|cover| (0.0..=1.0).contains(cover)));
    assert!(first.px0 <= first.px1 && first.px1 <= 60);
    assert!(first.py0 <= first.py1 && first.py1 <= 60);

    for index in 0..400 {
        let x0 = (index % 20) as f64 * 0.5;
        let y0 = (index / 20) as f64 * 0.5;
        let scale = 1.0 + index as f64 * 0.001;
        round_mask(
            60,
            60,
            [x0, y0, x0 + 8.0, y0 + 8.0],
            (index % 7) as f64,
            2.0,
            scale,
            false,
        );
    }
    assert_eq!(cached_masks(), 384);
}

#[test]
fn single_stop_gradient_and_empty_stops() {
    let mut buf = layer(20, 10, 0);
    fill_round_gradient(
        &mut buf,
        [1.0, 1.0, 18.0, 8.0],
        5.0,
        &[[30.0, 90.0, 150.0]],
        1.0,
        0.8,
    );
    assert_eq!(fnv1a_f32(buf.as_raw()), 0x942a_f4da_9c72_90ce);
    let before = buf.clone();
    fill_round_gradient(&mut buf, [1.0, 1.0, 18.0, 8.0], 5.0, &[], 1.0, 1.0);
    assert_eq!(buf, before);
}

#[test]
fn degenerate_geometry_draws_nothing() {
    let mut buf = layer(12, 9, 0);
    let before = buf.clone();
    fill_round_rect_opts(
        &mut buf,
        [8.0, 1.0, 3.0, 7.0],
        2.0,
        Rgba::WHITE,
        1.0,
        1.0,
        FillOptions::default(),
    );
    fill_round_rect_opts(
        &mut buf,
        [1.0, 7.0, 8.0, 3.0],
        2.0,
        Rgba::WHITE,
        1.0,
        0.0,
        FillOptions::default(),
    );
    assert_eq!(buf, before);
}

#[test]
fn layer_rejects_wrong_lengths() {
    assert!(Layer::from_raw(2, 2, vec![0.0; 15]).is_err());
    let mut buf = Layer::new(2, 2);
    buf.fill(0.25);
    assert!(buf.as_raw().iter().all(|value| *value == 0.25));
}
