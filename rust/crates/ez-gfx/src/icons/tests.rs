//! Unit tests of `icons.py` against vectors generated with NumPy 2.x.

use crate::test_support::fnv1a_f32;

use super::raster;
use super::{colorize, draw_icon, icon_mask, segment_distance, slash_masks, Color, IconError};

const MASKS: &[(&str, f64, f64, Option<u32>, u64)] = &[
    ("microphone", 24.0, 1.0, None, 0xc1c283104756a3b3),
    ("microphone", 19.0, 1.0, None, 0x2dc0de050352e5e8),
    ("microphone", 13.0, 1.0, None, 0x6e224fd46fa86f04),
    ("microphone", 17.0, 1.5, None, 0x032898db26207689),
    ("microphone", 20.0, 1.0, Some(37), 0xf919ac9502164090),
    ("microphone-slash", 24.0, 1.0, None, 0xc6fd755812c44802),
    ("microphone-slash", 19.0, 1.0, None, 0xcfd2364e12013b22),
    ("microphone-slash", 13.0, 1.0, None, 0xbe51d8e764bb581f),
    ("microphone-slash", 17.0, 1.5, None, 0x397b525343b8c5ce),
    ("microphone-slash", 20.0, 1.0, Some(37), 0xd2ae4e7fba1e36fe),
    ("phone", 24.0, 1.0, None, 0xae184436a8603d29),
    ("phone", 19.0, 1.0, None, 0x941482db564265fb),
    ("phone", 13.0, 1.0, None, 0xdd40db657e358fc0),
    ("phone", 17.0, 1.5, None, 0x31fd597183127d29),
    ("phone", 20.0, 1.0, Some(37), 0x746366d6e3c239d3),
    ("hangup", 24.0, 1.0, None, 0xe85cd07fdcda995f),
    ("hangup", 19.0, 1.0, None, 0x93b4df5a2bf51a71),
    ("hangup", 13.0, 1.0, None, 0x8dafb4bc7b996a19),
    ("hangup", 17.0, 1.5, None, 0x1c1850797fbe4d03),
    ("hangup", 20.0, 1.0, Some(37), 0x39403492bf2fd5d2),
    ("chevron-down", 24.0, 1.0, None, 0x5530a60872fae40e),
    ("chevron-down", 19.0, 1.0, None, 0x2c8e34f947bf9400),
    ("chevron-down", 13.0, 1.0, None, 0x37a86508410c579c),
    ("chevron-down", 17.0, 1.5, None, 0x4a11a8d1b14e5c95),
    ("chevron-down", 20.0, 1.0, Some(37), 0x7303e3769c88f751),
    ("chevron-up", 24.0, 1.0, None, 0xddc19192cbbfd103),
    ("chevron-up", 19.0, 1.0, None, 0x4780959d3152463b),
    ("chevron-up", 13.0, 1.0, None, 0x9a4f820c1b18d2a7),
    ("chevron-up", 17.0, 1.5, None, 0x0d67624c2555df79),
    ("chevron-up", 20.0, 1.0, Some(37), 0xf11c03f15eadfbc9),
    ("minus", 24.0, 1.0, None, 0xa2076fa772f0de15),
    ("minus", 19.0, 1.0, None, 0xd8f869afa5770dee),
    ("minus", 13.0, 1.0, None, 0x4fa1692eb689cc72),
    ("minus", 17.0, 1.5, None, 0x8a955b38e97d3fec),
    ("minus", 20.0, 1.0, Some(37), 0x795924531ecd77c7),
    ("check", 24.0, 1.0, None, 0xb183e0479f09da4f),
    ("check", 19.0, 1.0, None, 0xb5a0d1a39749b29e),
    ("check", 13.0, 1.0, None, 0x40fc809fc061f0f5),
    ("check", 17.0, 1.5, None, 0x09c05c11e05ced72),
    ("check", 20.0, 1.0, Some(37), 0x0649606775428e38),
    ("speaker", 24.0, 1.0, None, 0x74e76bcd0c582c2d),
    ("speaker", 19.0, 1.0, None, 0xc1bc6f3736d12ecd),
    ("speaker", 13.0, 1.0, None, 0xbd5e1d379ae6e48a),
    ("speaker", 17.0, 1.5, None, 0x4b0992420532ae1e),
    ("speaker", 20.0, 1.0, Some(37), 0x78d5139f411bd749),
    ("speaker-slash", 24.0, 1.0, None, 0x6b2e5e6911ae0877),
    ("speaker-slash", 19.0, 1.0, None, 0x299228fba9c380f6),
    ("speaker-slash", 13.0, 1.0, None, 0x9d056a04b6e038f9),
    ("speaker-slash", 17.0, 1.5, None, 0xce1fc7334105e8bb),
    ("speaker-slash", 20.0, 1.0, Some(37), 0x87eb34167baac7cb),
    ("close", 24.0, 1.0, None, 0x7bcf50d6ff70d785),
    ("close", 19.0, 1.0, None, 0xf706a5248116ba49),
    ("close", 13.0, 1.0, None, 0x09e8e392a0186c70),
    ("close", 17.0, 1.5, None, 0x523422c32110f3c8),
    ("close", 20.0, 1.0, Some(37), 0x7489dfdbecbd714e),
    ("sparkle", 24.0, 1.0, None, 0xd1447d52f270a039),
    ("sparkle", 19.0, 1.0, None, 0xdc1ae16326042381),
    ("sparkle", 13.0, 1.0, None, 0x7faa9bcdec39e43f),
    ("sparkle", 17.0, 1.5, None, 0x8382c79da9845de7),
    ("sparkle", 20.0, 1.0, Some(37), 0x6f0c82bd3dbf6fca),
    ("mic-line", 24.0, 1.0, None, 0x967f61fca9368dbe),
    ("mic-line", 19.0, 1.0, None, 0x0cbc6947dae62f22),
    ("mic-line", 13.0, 1.0, None, 0xec4c06d2cde67125),
    ("mic-line", 17.0, 1.5, None, 0x8bd1bbe01b7a760c),
    ("mic-line", 20.0, 1.0, Some(37), 0xa378e87bbcba329f),
    ("voice-line", 24.0, 1.0, None, 0x32929822a1bae1e5),
    ("voice-line", 19.0, 1.0, None, 0x27ef24fd116b91d1),
    ("voice-line", 13.0, 1.0, None, 0x236afb5d097cfd88),
    ("voice-line", 17.0, 1.5, None, 0x61c581249cb1ec9c),
    ("voice-line", 20.0, 1.0, Some(37), 0x10ec000b259931a6),
    ("chip", 24.0, 1.0, None, 0x11f1e0f33acd9b08),
    ("chip", 19.0, 1.0, None, 0x8f97557c93103d01),
    ("chip", 13.0, 1.0, None, 0x18b60373abeea6e5),
    ("chip", 17.0, 1.5, None, 0xdbecde4e75b80ae5),
    ("chip", 20.0, 1.0, Some(37), 0xb19041e4c90ef7cd),
    ("sliders", 24.0, 1.0, None, 0x4f9177e5da1728a8),
    ("sliders", 19.0, 1.0, None, 0x085c970b3a2c2e31),
    ("sliders", 13.0, 1.0, None, 0x015363422ad66b35),
    ("sliders", 17.0, 1.5, None, 0xe326525e0fc5eebe),
    ("sliders", 20.0, 1.0, Some(37), 0xfe76b04c60725fdf),
    ("copy", 24.0, 1.0, None, 0x1fdc091bc7f0cdb4),
    ("copy", 19.0, 1.0, None, 0x993c069c574bd50b),
    ("copy", 13.0, 1.0, None, 0x7bb376e601df830c),
    ("copy", 17.0, 1.5, None, 0xd8ad36b0073658ac),
    ("copy", 20.0, 1.0, Some(37), 0x76beb3561a2f4155),
    ("plus", 24.0, 1.0, None, 0x802b70f948c2f51d),
    ("plus", 19.0, 1.0, None, 0x172fb939da50d5ac),
    ("plus", 13.0, 1.0, None, 0x372b91f41ed899ea),
    ("plus", 17.0, 1.5, None, 0xddc2e21b11f95c35),
    ("plus", 20.0, 1.0, Some(37), 0x5257ea3dbb1444a8),
    ("clock", 24.0, 1.0, None, 0x98827a25113537d7),
    ("clock", 19.0, 1.0, None, 0xf8e0df4583be67a5),
    ("clock", 13.0, 1.0, None, 0xc7ea4c406fa97e9a),
    ("clock", 17.0, 1.5, None, 0x632a37b2e549d7a6),
    ("clock", 20.0, 1.0, Some(37), 0xa6229653b927eae7),
    ("send", 24.0, 1.0, None, 0x4f22c36002dcfb67),
    ("send", 19.0, 1.0, None, 0xa9cd0e86bedb4ae9),
    ("send", 13.0, 1.0, None, 0x22e957be0ede3f48),
    ("send", 17.0, 1.5, None, 0x0bb418f32ca77836),
    ("send", 20.0, 1.0, Some(37), 0x69aca09283dea747),
    ("stop", 24.0, 1.0, None, 0xe991993af7eef845),
    ("stop", 19.0, 1.0, None, 0x3cd9c18f1d4533fc),
    ("stop", 13.0, 1.0, None, 0xcebad6ae27456190),
    ("stop", 17.0, 1.5, None, 0x911de6944b85d271),
    ("stop", 20.0, 1.0, Some(37), 0xed540e8c3fb72e0b),
    ("chevron-left", 24.0, 1.0, None, 0x1c467a24118553fd),
    ("chevron-left", 19.0, 1.0, None, 0x1a32e7686988ea26),
    ("chevron-left", 13.0, 1.0, None, 0xbcd9d7e6639ad950),
    ("chevron-left", 17.0, 1.5, None, 0x4a53b6a908ec1071),
    ("chevron-left", 20.0, 1.0, Some(37), 0x3f00269bba834377),
    ("trash", 24.0, 1.0, None, 0x87006951a2776a10),
    ("trash", 19.0, 1.0, None, 0x51d40e01f1f71136),
    ("trash", 13.0, 1.0, None, 0xa14b5a7367a8a38a),
    ("trash", 17.0, 1.5, None, 0x71fe8988413b756e),
    ("trash", 20.0, 1.0, Some(37), 0x5d627d4711071e94),
    ("menu", 24.0, 1.0, None, 0x992f454a59a228d5),
    ("menu", 19.0, 1.0, None, 0xa68aa665cbbee6a5),
    ("menu", 13.0, 1.0, None, 0x6b0caba640acb0df),
    ("menu", 17.0, 1.5, None, 0x96326773e808a9b8),
    ("menu", 20.0, 1.0, Some(37), 0xfb83e62c28b726c6),
    ("chat", 24.0, 1.0, None, 0x96d816505afc5db0),
    ("chat", 19.0, 1.0, None, 0x5b9f115af9060b31),
    ("chat", 13.0, 1.0, None, 0xbd4b662e83cc2899),
    ("chat", 17.0, 1.5, None, 0xdf85a1db3e1b9324),
    ("chat", 20.0, 1.0, Some(37), 0x8abb6b0023c68ac0),
    ("waves", 24.0, 1.0, None, 0x1eb9d30e2d671228),
    ("waves", 19.0, 1.0, None, 0x39481c7f4d269f03),
    ("waves", 13.0, 1.0, None, 0xcafbb4a758468b4f),
    ("waves", 17.0, 1.5, None, 0x424253038d40c6fc),
    ("waves", 20.0, 1.0, Some(37), 0xf74e7d6bcc99bdb5),
    ("alert", 24.0, 1.0, None, 0x95c31ea2e4f46851),
    ("alert", 19.0, 1.0, None, 0x4342a0d15d310342),
    ("alert", 13.0, 1.0, None, 0x30d9b077ea0f800a),
    ("alert", 17.0, 1.5, None, 0x5e067a2cdf047333),
    ("alert", 20.0, 1.0, Some(37), 0x98e00c76fd3b0fda),
    ("compose", 24.0, 1.0, None, 0x7e332d40aa50041d),
    ("compose", 19.0, 1.0, None, 0xa0dc652787fb7942),
    ("compose", 13.0, 1.0, None, 0x86636c67bbe18dfd),
    ("compose", 17.0, 1.5, None, 0x8aa9a0e51f7fbc2f),
    ("compose", 20.0, 1.0, Some(37), 0xb026e726dead39fb),
];

const SLASHES: &[(f64, f64, f64, u64, u64)] = &[
    (24.0, 1.0, 0.0, 0xcc3cc02b0b0138bd, 0x242356a012a74465),
    (24.0, 1.0, 0.5, 0x76f720583bd14645, 0xfc93a80f920d7925),
    (19.0, 1.25, 1.0, 0xf3e97abf7b59c865, 0xbea34ebdf3e4d675),
];

const GRIDS: &[(u32, u64, u64)] = &[
    (7, 0x6980e4aa50f7276a, 0x530edbfbc3651586),
    (19, 0x9bb760df8650f0ce, 0x8f942aa6cf15c462),
    (24, 0x2b48ed07df3276a5, 0xe020b9cc58476da5),
];

const SEGMENT_DISTANCES: &[(u32, u64)] = &[
    (7, 0x0a90aee75f1ef6b6),
    (19, 0xa57aac31fdb7ae6f),
    (24, 0x6beb3374445f850c),
];

const COLORS: &[(Color, u64)] = &[
    (Color::Rgb(10, 20, 30), 0xa9a183a8cbaf92c7),
    (Color::Rgba(10, 20, 30, 128), 0x9209cd79b36fed1a),
    (Color::Rgba(255, 255, 255, 0), 0xf3fb6a6deb5af325),
];

const DRAW_CASES: &[(&str, f64, f64, f64, Color, f64, f64)] = &[
    (
        "microphone",
        20.0,
        16.5,
        18.0,
        Color::Rgb(255, 0, 0),
        1.0,
        1.0,
    ),
    ("close", 3.0, 3.0, 18.0, Color::Rgba(1, 2, 3, 128), 1.0, 1.0),
    ("chat", 39.0, 32.0, 18.0, Color::Rgb(9, 8, 7), 1.0, 1.0),
    (
        "alert",
        20.0,
        16.5,
        18.0,
        Color::Rgba(255, 255, 255, 255),
        1.0,
        0.5,
    ),
    ("mic", 20.0, 16.5, 18.0, Color::Rgb(255, 0, 0), 1.0, 0.0),
];

const DRAW_HASHES: &[u64] = &[
    0x429e4bdb2f2ba319, // 'microphone', 20.0, 16.5, 18.0, (255, 0, 0), 1.0, 1.0
    0x09881e66c4c8155e, // 'close', 3.0, 3.0, 18.0, (1, 2, 3, 128), 1.0, 1.0
    0x44a491c56b47bf97, // 'chat', 39.0, 32.0, 18.0, (9, 8, 7), 1.0, 1.0
    0xb5af719e6bb2a135, // 'alert', 20.0, 16.5, 18.0, (255, 255, 255, 255), 1.0, 0.5
    0xa214310170ae5525, // 'mic', 20.0, 16.5, 18.0, (255, 0, 0), 1.0, 0.0
];

fn mask_hash(mask: &ndarray::Array2<f32>) -> u64 {
    fnv1a_f32(
        mask.as_slice()
            .expect("the rasterizer returns a standard layout"),
    )
}

#[test]
fn grids_match_numpy() {
    for &(side, expected_x, expected_y) in GRIDS {
        let (px, py) = raster::grid(side);
        assert_eq!(fnv1a_f32(&px), expected_x, "gx at {side}");
        assert_eq!(fnv1a_f32(&py), expected_y, "gy at {side}");
    }
}

#[test]
fn segment_distance_matches_numpy() {
    let segments = [[1.0, 2.0, 3.0, 4.0], [5.0, 5.0, 7.0, 8.0]];
    for &(side, expected) in SEGMENT_DISTANCES {
        let (px, py) = raster::grid(side);
        let distance = segment_distance(&px, &py, &segments);
        assert_eq!(fnv1a_f32(&distance), expected, "side {side}");
    }
}

#[test]
fn masks_match_numpy() {
    for &(name, size, scale, pixels, expected) in MASKS {
        let mask = icon_mask(name, size, scale, pixels).expect("valid icon");
        assert_eq!(
            mask_hash(&mask),
            expected,
            "{name} size={size} scale={scale} pixels={pixels:?}"
        );
    }
}

#[test]
fn slash_masks_match_numpy() {
    for &(size, scale, progress, gap_expected, line_expected) in SLASHES {
        let (gap, line) = slash_masks(size, scale, progress, None).expect("valid slash");
        assert_eq!(
            mask_hash(&gap),
            gap_expected,
            "gap {size} {scale} {progress}"
        );
        assert_eq!(
            mask_hash(&line),
            line_expected,
            "line {size} {scale} {progress}"
        );
    }
}

#[test]
fn colorize_matches_numpy() {
    let mask = icon_mask("speaker", 24.0, 1.0, None).expect("valid icon");
    for &(color, expected) in COLORS {
        let sprite = colorize(&mask, color);
        let values = sprite
            .as_slice()
            .expect("colorize builds a standard layout");
        assert_eq!(fnv1a_f32(values), expected, "{color:?}");
    }
}

#[test]
fn draw_icon_matches_numpy() {
    for (&(name, cx, cy, size, color, scale, alpha), &expected) in
        DRAW_CASES.iter().zip(DRAW_HASHES)
    {
        let mut buffer = vec![0.0f32; 40 * 33 * 4];
        draw_icon(&mut buffer, 40, 33, name, cx, cy, size, color, scale, alpha)
            .expect("valid draw");
        assert_eq!(fnv1a_f32(&buffer), expected, "{name} alpha={alpha}");
    }
}

#[test]
fn aliases_resolve_to_the_same_mask() {
    for (alias, canonical) in [
        ("mic", "microphone"),
        ("mic-off", "microphone-slash"),
        ("mic_off", "microphone-slash"),
        ("microphone-off", "microphone-slash"),
        ("call", "phone"),
        ("chevron", "chevron-down"),
        ("chevron_down", "chevron-down"),
        ("chevron_up", "chevron-up"),
        ("dash", "minus"),
        ("volume", "speaker"),
        ("volume-off", "speaker-slash"),
        ("speaker-off", "speaker-slash"),
        ("mute", "speaker-slash"),
        ("gear", "menu"),
    ] {
        let aliased = icon_mask(alias, 24.0, 1.0, None).expect("alias");
        let direct = icon_mask(canonical, 24.0, 1.0, None).expect("canonical");
        assert_eq!(aliased, direct, "{alias} != {canonical}");
    }
}

#[test]
fn errors_match_the_python_messages() {
    let unknown = icon_mask("no-existe", 24.0, 1.0, None).expect_err("unknown icon");
    assert_eq!(unknown.to_string(), "icono desconocido: 'no-existe'");
    assert_eq!(unknown, IconError::Unknown("no-existe".to_string()));

    let zero_size = icon_mask("microphone", 0.0, 1.0, None).expect_err("size 0");
    assert_eq!(
        zero_size.to_string(),
        "size y scale deben ser mayores que cero"
    );
    assert!(icon_mask("microphone", 24.0, 0.0, None).is_err());

    let mut buffer = vec![0.0f32; 8];
    let shape = draw_icon(
        &mut buffer,
        3,
        3,
        "microphone",
        1.0,
        1.0,
        18.0,
        Color::Rgb(0, 0, 0),
        1.0,
        1.0,
    )
    .expect_err("short buffer");
    assert_eq!(
        shape.to_string(),
        "buf debe ser float32 con forma (H, W, 4)"
    );

    let mut buffer = vec![0.0f32; 8 * 8 * 4];
    let alpha = draw_icon(
        &mut buffer,
        8,
        8,
        "microphone",
        4.0,
        4.0,
        18.0,
        Color::Rgb(0, 0, 0),
        1.0,
        1.5,
    )
    .expect_err("alpha out of range");
    assert_eq!(alpha.to_string(), "alpha debe estar en [0, 1]");
    assert!(draw_icon(
        &mut buffer,
        8,
        8,
        "microphone",
        4.0,
        4.0,
        18.0,
        Color::Rgb(0, 0, 0),
        1.0,
        f64::NAN,
    )
    .is_err());
    assert!(draw_icon(
        &mut buffer,
        8,
        8,
        "microphone",
        4.0,
        4.0,
        18.0,
        Color::Rgb(0, 0, 0),
        1.0,
        -0.1,
    )
    .is_err());
}

#[test]
fn clear_cache_keeps_the_same_pixels() {
    let before = icon_mask("sparkle", 22.0, 1.25, None).expect("valid icon");
    let sprite_before = colorize(&before, Color::Rgba(1, 2, 3, 200));
    super::clear_cache();
    let after = icon_mask("sparkle", 22.0, 1.25, None).expect("valid icon");
    let sprite_after = colorize(&after, Color::Rgba(1, 2, 3, 200));
    assert_eq!(before, after);
    assert_eq!(sprite_before, sprite_after);
}
