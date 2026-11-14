//! Unit tests for the Pillow `ImageDraw` port.

use super::oracle::CASES;
use super::{Draw, DrawError, Surface};
use crate::raster::{Image, Mask, Rgba};
use crate::test_support::{fnv1a, unhex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    L,
    Rgba,
}

#[derive(Clone, Copy, Debug)]
enum InkValue {
    Gray(u8),
    Rgba([u8; 4]),
}

struct Case {
    op: String,
    mode: Mode,
    width: u32,
    height: u32,
    geometry: Vec<(f64, f64)>,
    fill: Option<InkValue>,
    outline: Option<InkValue>,
    radius: f64,
    width_arg: u32,
    hash: u64,
}

fn parse_cases() -> Vec<Case> {
    CASES
        .lines()
        .filter(|line| !line.is_empty())
        .map(parse_case)
        .collect()
}

fn parse_case(line: &str) -> Case {
    let fields: Vec<&str> = line.split('|').collect();
    let op = fields[0].to_string();
    let mode = if fields[1] == "L" {
        Mode::L
    } else {
        Mode::Rgba
    };
    let geometry = fields[4]
        .split(';')
        .map(|pair| {
            let (x, y) = pair.split_once(',').expect("the oracle emitted pairs");
            (
                x.parse().expect("the oracle emitted numbers"),
                y.parse().expect("the oracle emitted numbers"),
            )
        })
        .collect();
    let (radius, width_arg, hash) = if op == "rrect" {
        (
            fields[7].parse().expect("the oracle emitted a radius"),
            fields[8].parse().expect("the oracle emitted a width"),
            u64::from_str_radix(fields[9], 16).expect("the oracle emitted a hash"),
        )
    } else {
        (
            0.0,
            fields[7].parse().expect("the oracle emitted a width"),
            u64::from_str_radix(fields[8], 16).expect("the oracle emitted a hash"),
        )
    };
    Case {
        op,
        mode,
        width: fields[2].parse().expect("the oracle emitted a size"),
        height: fields[3].parse().expect("the oracle emitted a size"),
        geometry,
        fill: parse_ink(mode, fields[5]),
        outline: parse_ink(mode, fields[6]),
        radius,
        width_arg,
        hash,
    }
}

fn parse_ink(mode: Mode, text: &str) -> Option<InkValue> {
    if text == "-" {
        return None;
    }
    let values: Vec<u8> = text
        .split(':')
        .map(|value| value.parse().expect("the oracle emitted channels"))
        .collect();
    Some(match mode {
        Mode::L => InkValue::Gray(values[0]),
        Mode::Rgba => InkValue::Rgba([values[0], values[1], values[2], values[3]]),
    })
}

fn initial_mask(width: u32, height: u32) -> Mask {
    let data = (0..width as usize * height as usize)
        .map(|index| ((index * 37 + 11) % 256) as u8)
        .collect();
    Mask::from_raw(width, height, data).expect("the buffer matches the size")
}

fn initial_image(width: u32, height: u32) -> Image {
    let data = (0..width as usize * height as usize * 4)
        .map(|index| ((index * 37 + 11) % 256) as u8)
        .collect();
    Image::from_rgba(width, height, data).expect("the buffer matches the size")
}

fn apply<S: Surface>(surface: &mut S, case: &Case, to_ink: impl Fn(InkValue) -> S::Ink) {
    let bbox = [
        case.geometry[0].0,
        case.geometry[0].1,
        case.geometry[1].0,
        case.geometry[1].1,
    ];
    let fill = case.fill.map(&to_ink);
    let outline = case.outline.map(&to_ink);
    let mut draw = Draw::new(surface);
    match case.op.as_str() {
        "ellipse" => {
            draw.ellipse(bbox, fill, outline, case.width_arg)
                .expect("the oracle only used ordered boxes");
        }
        "rectangle" => {
            draw.rectangle(bbox, fill, outline, case.width_arg)
                .expect("the oracle only used ordered boxes");
        }
        "line" => draw.line(&case.geometry, fill, case.width_arg),
        "polygon" => {
            draw.polygon(&case.geometry, fill, outline, case.width_arg)
                .expect("the oracle only used polygons with two points or more");
        }
        "rrect" => {
            draw.rounded_rectangle(bbox, case.radius, fill, outline, case.width_arg)
                .expect("the oracle only used boxes Pillow accepted");
        }
        other => panic!("unknown oracle op: {other}"),
    }
}

#[test]
fn every_oracle_case_matches_pillow() {
    let cases = parse_cases();
    assert!(cases.len() >= 300, "the oracle table is complete");
    for (index, case) in cases.iter().enumerate() {
        match case.mode {
            Mode::L => {
                let mut mask = initial_mask(case.width, case.height);
                apply(&mut mask, case, |ink| match ink {
                    InkValue::Gray(value) => value,
                    InkValue::Rgba(_) => 0,
                });
                assert_eq!(
                    fnv1a(mask.as_raw()),
                    case.hash,
                    "{} L {}x{} {:?} width={} fill={:?} outline={:?} case={index}",
                    case.op,
                    case.width,
                    case.height,
                    case.geometry,
                    case.width_arg,
                    case.fill,
                    case.outline
                );
            }
            Mode::Rgba => {
                let mut image = initial_image(case.width, case.height);
                apply(&mut image, case, |ink| match ink {
                    InkValue::Rgba(channels) => Rgba::from_array(channels),
                    InkValue::Gray(value) => Rgba::opaque(value, value, value),
                });
                assert_eq!(
                    fnv1a(image.as_raw()),
                    case.hash,
                    "{} RGBA {}x{} {:?} width={} fill={:?} outline={:?} case={index}",
                    case.op,
                    case.width,
                    case.height,
                    case.geometry,
                    case.width_arg,
                    case.fill,
                    case.outline
                );
            }
        }
    }
}

#[test]
fn explicit_pillow_vectors_match() {
    let mut mask = Mask::new(9, 7, 0);
    Draw::new(&mut mask)
        .ellipse([1.2, 1.4, 7.6, 5.4], Some(255), None, 1)
        .expect("ordered box");
    assert_eq!(
        mask.as_raw(),
        unhex("000000000000000000000000ffffff00000000ffffffffffffff0000ffffffffffffff0000ffffffffffffff00000000ffffff000000000000000000000000")
    );

    let mut mask = Mask::new(12, 9, 0);
    Draw::new(&mut mask).line(&[(1.5, 2.5), (8.2, 7.9), (11.0, 1.0)], Some(180), 3);
    assert_eq!(fnv1a(mask.as_raw()), 0x1045_ddc9_6f9a_ef4d);

    let mut mask = Mask::new(10, 8, 0);
    Draw::new(&mut mask)
        .polygon(
            &[(1.0, 1.0), (8.0, 2.0), (6.0, 7.0)],
            Some(90),
            Some(255),
            1,
        )
        .expect("three points");
    assert_eq!(fnv1a(mask.as_raw()), 0x6555_f2c8_fd27_0899);

    let mut mask = Mask::new(14, 12, 0);
    Draw::new(&mut mask)
        .rounded_rectangle([1.0, 1.0, 12.6, 10.2], 3.5, Some(255), Some(40), 1)
        .expect("ordered box");
    assert_eq!(fnv1a(mask.as_raw()), 0x6dc5_1193_a7ae_9525);

    let mut image = Image::new(9, 7, Rgba::TRANSPARENT);
    Draw::new(&mut image)
        .ellipse(
            [1.2, 1.4, 7.6, 5.4],
            Some(Rgba::opaque(10, 20, 30)),
            Some(Rgba::new(200, 100, 50, 128)),
            2,
        )
        .expect("ordered box");
    assert_eq!(fnv1a(image.as_raw()), 0x4b5d_b18e_9de6_0f68);

    let mut image = Image::new(12, 10, Rgba::TRANSPARENT);
    Draw::new(&mut image)
        .rounded_rectangle(
            [0.6, 0.8, 10.9, 9.1],
            4.2,
            Some(Rgba::new(9, 80, 160, 200)),
            Some(Rgba::opaque(250, 240, 230)),
            2,
        )
        .expect("ordered box");
    assert_eq!(fnv1a(image.as_raw()), 0x6bd7_6831_ebf2_2364);
}

#[test]
fn coordinate_errors_use_pillow_text() {
    let mut mask = Mask::new(4, 4, 0);
    let mut draw = Draw::new(&mut mask);
    assert_eq!(
        draw.rectangle([3.0, 0.0, 1.0, 2.0], Some(1), None, 1),
        Err(DrawError::IncorrectlyOrderedX)
    );
    assert_eq!(
        draw.ellipse([0.0, 3.0, 2.0, 1.0], Some(1), None, 1),
        Err(DrawError::IncorrectlyOrderedY)
    );
    assert_eq!(
        draw.polygon(&[(1.0, 1.0)], Some(1), None, 1),
        Err(DrawError::TooFewCoordinates)
    );
    assert_eq!(
        DrawError::IncorrectlyOrderedX.to_string(),
        "x1 must be greater than or equal to x0"
    );
    // Pillow's rounded_rectangle can also hit a reversed top/bottom rectangle.
    let mut mask = Mask::new(9, 7, 0);
    assert_eq!(
        Draw::new(&mut mask).rounded_rectangle(
            [
                1.1201096553431755,
                4.962030111852236,
                3.59421298807954,
                6.885545524948528
            ],
            0.5,
            Some(200),
            Some(80),
            5
        ),
        Err(DrawError::IncorrectlyOrderedX)
    );
    assert_eq!(
        Draw::new(&mut mask).rounded_rectangle([0.0, 0.0, 9.0, 9.0], -1.0, Some(1), None, 1),
        Err(DrawError::IncorrectlyOrderedX)
    );
}

#[test]
fn omitted_colors_use_pillows_default_ink() {
    let mut image = Image::new(3, 1, Rgba::TRANSPARENT);
    Draw::new(&mut image).line(&[(0.0, 0.0), (2.0, 0.0)], None, 1);
    assert!(image.pixels().all(|pixel| pixel == Rgba::WHITE));

    let mut mask = Mask::new(3, 1, 7);
    Draw::new(&mut mask).line(&[(0.0, 0.0), (2.0, 0.0)], None, 1);
    assert_eq!(mask.as_raw(), &[0, 0, 0]);
}

#[test]
fn one_point_line_draws_nothing_like_pillow() {
    let mut mask = Mask::new(5, 5, 0);
    Draw::new(&mut mask).line(&[(2.0, 2.0)], Some(255), 1);
    assert_eq!(mask.as_raw(), &[0; 25]);
    // A zero-width line is ignored too.
    let mut mask = Mask::new(5, 5, 7);
    Draw::new(&mut mask).line(&[(0.0, 0.0), (4.0, 4.0)], Some(255), 0);
    assert_eq!(mask.as_raw(), &[7; 25]);
}
