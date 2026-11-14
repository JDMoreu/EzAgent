//! Pruebas de `paint::drop` contra los oráculos de Pillow y NumPy.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use ez_gfx::raster::Image;

use crate::model::drop::{layout_size, DropModel, Update};
use crate::paint::drop::canvas::mute_tint;
use crate::paint::drop::colors::disc;
use crate::paint::drop::fluid::{fluid_grid, form_mask, render_fluid};
use crate::paint::drop::sprite::{button_sprite, SpriteOptions};
use crate::tokens::{set_themed, test_guard};

const DT: f64 = 1.0 / 60.0;

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn fnv_f32(values: &[f32]) -> u64 {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    fnv(&bytes)
}

fn close(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn the_fluid_grid_matches_numpy() {
    let (gx, gy) = fluid_grid("pill");
    assert_eq!(gx.shape(), &[19, 54]);
    assert_eq!(gy.shape(), &[19, 54]);
    let xs = [-2.789_473_8f32, -2.684_210_5, -2.578_947_3];
    for (column, expected) in xs.iter().enumerate() {
        close(gx[[0, column]], *expected, 1e-6);
        close(gx[[18, column]], *expected, 1e-6);
    }
    let ys = [-0.947_368_44f32, -0.842_105_27, -0.736_842_1];
    for (row, expected) in ys.iter().enumerate() {
        close(gy[[row, 0]], *expected, 1e-6);
        close(gy[[row, 53]], *expected, 1e-6);
    }
    let (circle_x, circle_y) = fluid_grid("circle");
    assert_eq!(circle_x.shape(), &[29, 29]);
    assert_eq!(circle_y.shape(), &[29, 29]);
}

#[test]
fn the_form_mask_matches_pillow() {
    let _guard = test_guard();
    let pill = form_mask("pill", 1.0);
    assert_eq!((pill.width(), pill.height()), (108, 38));
    assert_eq!(fnv(pill.as_raw()), 15_577_053_465_889_545_933);
    assert_eq!(pill.get(54, 19), 255);
    assert_eq!(pill.get(0, 0), 0);
    let circle = form_mask("circle", 1.5);
    assert_eq!((circle.width(), circle.height()), (87, 87));
    assert_eq!(fnv(circle.as_raw()), 1_987_492_832_332_655_054);
}

#[test]
fn the_fluid_pixels_match_numpy() {
    let look = Look {
        pose: "listen",
        rms: 0.42,
        ..Look::default()
    };
    let mut model = DropModel::new("pill", Some("sides"));
    warm(&mut model, &look);
    let fluid = render_fluid(&model, 0.0);
    assert_eq!(fluid.shape(), &[19, 54, 3]);
    assert_eq!(
        fnv(fluid.iter().copied().collect::<Vec<_>>().as_slice()),
        17_632_696_372_063_950_709
    );
    assert_eq!(fluid[[9, 27, 0]], 55);
    assert_eq!(fluid[[9, 27, 1]], 194);
    assert_eq!(fluid[[9, 27, 2]], 200);
    assert_eq!(fluid[[0, 0, 0]], 87);
    assert_eq!(fluid[[0, 0, 1]], 206);
    assert_eq!(fluid[[0, 0, 2]], 207);
}

#[test]
fn the_discs_match_pillow() {
    let _guard = test_guard();
    set_themed(None);
    let light = disc("light", 1.0, "disc");
    assert_eq!(light.side, 56);
    assert_eq!(fnv_f32(&light.values), 12_252_678_467_824_092_545);
    let dark = disc("dark", 1.0, "hover");
    assert_eq!(dark.side, 56);
    assert_eq!(fnv_f32(&dark.values), 6_620_202_857_301_098_880);
    let hang = disc("light", 2.0, "hang");
    assert_eq!(hang.side, 112);
    assert_eq!(fnv_f32(&hang.values), 9_755_858_654_358_703_730);
}

#[test]
fn the_button_sprites_match_pillow() {
    let _guard = test_guard();
    set_themed(None);
    let plain = SpriteOptions::default();
    let expected = [
        ("gear", 3_019_845_759_627_597_081u64),
        ("mic", 11_601_324_411_769_761_753),
        ("hang", 12_661_348_836_549_583_745),
        ("call", 16_948_862_628_078_930_099),
        ("yes", 2_067_543_073_450_986_966),
    ];
    for (glyph, hash) in expected {
        let sprite = button_sprite(glyph, "light", 1.0, &plain).expect("glifo de la barra");
        assert_eq!(sprite.width(), 56);
        assert_eq!(fnv(sprite.as_raw()), hash, "{glyph}");
    }
    let muted = button_sprite(
        "mic",
        "dark",
        1.5,
        &SpriteOptions {
            muted: 0.6,
            ..SpriteOptions::default()
        },
    )
    .expect("micrófono");
    assert_eq!(fnv(muted.as_raw()), 14_469_942_733_468_146_275);
    let call = button_sprite(
        "call",
        "dark",
        2.0,
        &SpriteOptions {
            call: true,
            hover: true,
            ..SpriteOptions::default()
        },
    )
    .expect("llamar");
    assert_eq!(call.width(), 112);
    assert_eq!(fnv(call.as_raw()), 17_299_864_043_572_006_450);
}

const MUTE_EXPECTED: [[[u8; 3]; 12]; 3] = [
    [
        [0, 255, 0],
        [23, 231, 37],
        [46, 208, 74],
        [69, 185, 111],
        [92, 162, 148],
        [115, 139, 185],
        [139, 115, 222],
        [162, 92, 3],
        [185, 69, 40],
        [208, 46, 77],
        [231, 23, 114],
        [255, 0, 151],
    ],
    [
        [64, 198, 64],
        [74, 185, 82],
        [86, 171, 100],
        [96, 158, 119],
        [108, 144, 137],
        [119, 132, 156],
        [130, 118, 174],
        [129, 92, 46],
        [140, 78, 64],
        [151, 66, 82],
        [163, 52, 100],
        [174, 39, 119],
    ],
    [
        [124, 147, 124],
        [123, 142, 125],
        [123, 138, 126],
        [123, 134, 126],
        [123, 129, 127],
        [123, 126, 129],
        [123, 121, 130],
        [99, 92, 85],
        [99, 88, 86],
        [100, 84, 87],
        [100, 80, 89],
        [100, 76, 90],
    ],
];

fn gradient() -> Image {
    let mut interior = Image::new(4, 3, ez_gfx::raster::Rgba::TRANSPARENT);
    for index in 0..12u32 {
        let color = ez_gfx::raster::Rgba::opaque(
            (index * 255 / 11) as u8,
            ((11 - index) * 255 / 11) as u8,
            ((index * 37) % 256) as u8,
        );
        interior.set_pixel(index % 4, index / 4, color);
    }
    interior
}

#[test]
fn the_mute_tint_matches_pillow() {
    let interior = gradient();
    for (k, row) in [0.0f64, 0.5, 1.0].iter().zip(MUTE_EXPECTED) {
        let tinted = mute_tint(&interior, *k);
        for (index, expected) in row.iter().enumerate() {
            let pixel = tinted.pixel(index as u32 % 4, index as u32 / 4);
            assert_eq!([pixel.r, pixel.g, pixel.b], *expected, "k={k}");
        }
    }
}

#[test]
fn the_mute_tint_grid_matches_pillow() {
    let mut grid = Image::new(16, 16, ez_gfx::raster::Rgba::TRANSPARENT);
    for index in 0..256u32 {
        let color = ez_gfx::raster::Rgba::opaque(
            ((index * 17) % 256) as u8,
            ((index * 31) % 256) as u8,
            ((index * 47) % 256) as u8,
        );
        grid.set_pixel(index % 16, index / 16, color);
    }
    for (k, hash) in [
        (0.0, 596_122_400_775_536_933u64),
        (0.37, 137_640_133_505_457_637),
        (0.5, 14_501_631_122_206_375_749),
        (0.83, 7_981_714_172_671_090_692),
        (1.0, 1_001_074_771_190_259_879),
    ] {
        let tinted = mute_tint(&grid, k);
        let rgb: Vec<u8> = tinted
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        assert_eq!(fnv(&rgb), hash, "k={k}");
    }
}

#[test]
fn the_card_body_matches_python() {
    let _guard = test_guard();
    set_themed(None);
    let light =
        crate::paint::drop::render_card_body(200, 100, 1.0, "light", false, 0).expect("tema claro");
    assert_eq!(fnv_f32(&light), 8_134_545_511_799_908_973);
    close(light[10 * 200 * 4 + 10 * 4], 0.980_392_16, 1e-7);
    assert_eq!(light[50 * 200 * 4 + 100 * 4 + 3], 1.0);
    let dark =
        crate::paint::drop::render_card_body(320, 200, 1.0, "dark", false, 0).expect("tema oscuro");
    assert_eq!(fnv_f32(&dark), 13_990_025_138_677_072_429);
    close(dark[10 * 320 * 4 + 10 * 4], 0.050_980_393, 1e-7);
    let fresh =
        crate::paint::drop::render_card_body(200, 100, 1.0, "light", true, 0).expect("tema claro");
    assert_eq!(fnv_f32(&fresh), 11_831_572_644_544_543_181);
    let scaled =
        crate::paint::drop::render_card_body(160, 80, 2.0, "dark", true, 0).expect("tema oscuro");
    assert_eq!(fnv_f32(&scaled), 11_750_486_392_403_932_029);
    assert!(crate::paint::drop::render_card_body(10, 10, 1.0, "claro", false, 0).is_err());
}

// --------------------------------------------------------------------- barra
#[derive(Clone, Copy)]
struct Look {
    theme: &'static str,
    base: &'static str,
    layout: &'static str,
    dock: &'static str,
    pose: &'static str,
    muted: bool,
    in_call: bool,
    hover: bool,
    rms: f64,
    orbit: &'static [&'static str],
    pulse: f64,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: "light",
            base: "light",
            layout: "pill",
            dock: "sides",
            pose: "idle",
            muted: false,
            in_call: true,
            hover: true,
            rms: 0.0,
            orbit: &[],
            pulse: 1.0,
        }
    }
}

fn step(model: &mut DropModel, now: f64, dt: f64, look: &Look) {
    model.update(&Update {
        now,
        dt,
        pose: look.pose,
        rms: look.rms,
        theme: look.base,
        hover: look.hover,
        muted: look.muted,
        in_call: look.in_call,
        ..Update::default()
    });
}

fn warm(model: &mut DropModel, look: &Look) {
    model.form_hidden = look.pose == "off";
    let mut now = 0.0;
    step(model, now, 0.0, look);
    for _ in 0..72 {
        now += DT;
        step(model, now, DT, look);
    }
}

fn still(look: &Look) -> DropModel {
    let mut model = DropModel::new(look.layout, Some(look.dock));
    warm(&mut model, look);
    model
}

fn transition(before: &Look, after: &Look, t_ms: f64) -> DropModel {
    let mut model = DropModel::new(after.layout, Some(after.dock));
    for index in 0..72 {
        step(&mut model, -1.2 + f64::from(index) * DT, DT, before);
    }
    step(&mut model, 0.0, 0.0, after);
    if t_ms > 0.0 {
        step(&mut model, t_ms / 1000.0, t_ms / 1000.0, after);
    }
    model
}

fn straight_from_bgra(bgra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len());
    for pixel in bgra.as_chunks::<4>().0 {
        let alpha = f32::from(pixel[3]) / 255.0;
        let safe = alpha.max(1e-6);
        for channel in [pixel[2], pixel[1], pixel[0]] {
            let value = if alpha > 0.0 {
                (f32::from(channel) / 255.0) / safe
            } else {
                0.0
            };
            out.push((value.clamp(0.0, 1.0) * 255.0_f32).round_ties_even() as u8);
        }
        out.push((alpha.clamp(0.0, 1.0) * 255.0_f32).round_ties_even() as u8);
    }
    out
}

fn golden(name: &str) -> (u32, u32, Vec<u8>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../parity/golden/hud/bar")
        .join(name);
    let decoder = png::Decoder::new(BufReader::new(
        File::open(&path).expect("el cuadro dorado existe"),
    ));
    let mut reader = decoder.read_info().expect("PNG con cabecera");
    let mut buffer = vec![0; reader.output_buffer_size().expect("PNG buffer")];
    let info = reader.next_frame(&mut buffer).expect("PNG con fotograma");
    let bytes = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => bytes
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        other => panic!("tipo de color inesperado: {other:?}"),
    };
    (info.width, info.height, rgba)
}

fn compare_frame(model: &DropModel, look: &Look, name: &str) {
    let (logical_w, logical_h) = layout_size(look.layout, Some(look.dock));
    let width = (logical_w * 1.0).round_ties_even().max(1.0) as u32;
    let height = (logical_h * 1.0).round_ties_even().max(1.0) as u32;
    compare_frame_at(model, look, name, (width, height), 1.0);
}

fn compare_frame_at(
    model: &DropModel,
    look: &Look,
    name: &str,
    (width, height): (u32, u32),
    scale: f64,
) {
    let options = crate::paint::drop::RenderOptions {
        width,
        height,
        scale,
        theme: look.theme,
        muted: look.muted,
        in_call: look.in_call,
        menu_open: false,
        now: Some(0.0),
        hover_item: None,
        chat_open: false,
        orbit: look.orbit,
        pulse: look.pulse,
    };
    let frame = crate::paint::drop::render_model(model, &options).expect("barra sin errores");
    assert_eq!(frame.len(), (width * height * 4) as usize);
    let ours = straight_from_bgra(&frame);
    let (golden_w, golden_h, expected) = golden(name);
    assert_eq!((golden_w, golden_h), (width, height), "{name}");
    let mut max_delta = 0i32;
    for (actual, wanted) in ours.iter().zip(&expected) {
        max_delta = max_delta.max((i32::from(*actual) - i32::from(*wanted)).abs());
    }
    // El umbral de `parity/thresholds.toml` es ≤1 nivel por canal en el 99,9 %;
    // los cuadros reconstruibles salen idénticos.
    assert_eq!(max_delta, 0, "{name}: desvío máximo {max_delta} por canal");
}

fn compare_still(look: &Look, name: &str) {
    compare_frame(&still(look), look, name);
}

#[test]
fn the_bar_frames_match_the_golden_png() {
    let _guard = test_guard();
    set_themed(None);
    compare_still(
        &Look {
            theme: "dark",
            base: "dark",
            ..Look::default()
        },
        "dark-100-pill-sides-idle-m0-o0@1200.png",
    );
    compare_still(
        &Look {
            theme: "light",
            base: "light",
            hover: false,
            ..Look::default()
        },
        "light-100-pill-sides-idle-m0-o0-rest@1200.png",
    );
    compare_still(
        &Look {
            theme: "dark",
            base: "dark",
            pose: "mute",
            muted: true,
            ..Look::default()
        },
        "dark-100-pill-sides-mute-m1-o0@1200.png",
    );
    compare_still(
        &Look {
            theme: "dark",
            base: "dark",
            layout: "circle",
            dock: "bottom",
            orbit: &["needs_you", "failed", "working", "waiting", "stale"],
            ..Look::default()
        },
        "dark-100-circle-bottom-idle-m0-o5-orbit-full@1200.png",
    );
    let dpi = Look {
        theme: "light",
        base: "light",
        ..Look::default()
    };
    compare_frame_at(
        &still(&dpi),
        &dpi,
        "light-150-pill-sides-idle-m0-o0-dpi@1200.png",
        (462, 93),
        1.5,
    );
}

#[test]
fn the_bar_emerge_frame_matches_the_golden_png() {
    let _guard = test_guard();
    set_themed(None);
    let before = Look {
        theme: "dark",
        base: "dark",
        hover: false,
        ..Look::default()
    };
    let after = Look {
        theme: "dark",
        base: "dark",
        ..Look::default()
    };
    compare_frame(
        &transition(&before, &after, 16.0),
        &after,
        "anim-emerge-dark-100-pill-sides-idle-m0-o0@16.png",
    );
}
