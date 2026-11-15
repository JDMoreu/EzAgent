//! Parity of the text masks with Pillow (`parity` has no `gfx_text` fixtures).
//!
//! The expected values come from Pillow 13.0.0 / FreeType 2.14.3 through
//! `src/ezagent/voice/hud/text.py`: ink size in physical pixels and an FNV-1a
//! hash of the cropped coverage mask. The mask is read back through the public
//! `draw_text` compositor, so the test also covers the RGBA blend.
//!
//! The workspace pins `freetype-sys 0.23`, which bundles FreeType 2.13.2.
//! Its TrueType interpreter hints accent composites differently (for example
//! U+00B4 renders `[0, 164, 83] [48, 202, 2]` instead of Pillow's
//! `[17, 192, 49] [10, 15, 0]` at 13 px), so the strict byte comparison only
//! runs when the probe below detects Pillow's rasterizer. With the vendored
//! 2.13.2 the test still checks the advance, the height within one pixel and
//! the physical mask size; see `text::tests` for the bounded coverage delta.

use ez_gfx::text::{
    draw_text, text_size, Color, FloatSurface, FontStyle, TextOptions, FS_BODY, FS_MICRO, FS_ROW,
    FS_SECONDARY, FS_TITLE,
};

/// One Pillow case: style token, DPI factor, text, mask width/height and hash.
type Case = (&'static str, f64, &'static str, usize, usize, u64);

/// `hud/peek_paint.py`: the quick view uses two styles outside `tokens.py`.
const FS_TASK: FontStyle = FontStyle::new("FS_TASK", "Geist", 12.5, 500);
const FS_META: FontStyle = FontStyle::new("FS_META", "Geist", 10.5, 400);

const CASES: &[Case] = &[
    ("FS_TITLE", 1.0, "A", 11, 12, 0xdfbf7b4048ebea40),
    ("FS_TITLE", 1.0, "A", 11, 12, 0xdfbf7b4048ebea40),
    ("FS_TITLE", 1.0, "Hola mundo", 93, 12, 0x9eefc7883c241f56),
    ("FS_TITLE", 1.0, "Automático", 91, 12, 0x9bde7f46ce8d258a),
    (
        "FS_TITLE",
        1.0,
        "Según el sistema",
        134,
        14,
        0xdc37dc7153b83622,
    ),
    ("FS_TITLE", 1.0, "Hablando…", 84, 12, 0x3f7d3e1a2094409b),
    ("FS_TITLE", 1.0, "Español", 62, 15, 0xaf66509d0a666dfb),
    ("FS_TITLE", 1.0, "Ñandú", 49, 16, 0xb5512ca1da632ff0),
    ("FS_TITLE", 1.0, "Über", 35, 15, 0x53d77d1860708a35),
    ("FS_TITLE", 1.0, "¿Qué?", 49, 14, 0x85f5825ee0fdcbf8),
    ("FS_TITLE", 1.0, "¡Hola!", 44, 14, 0x6afe7942155331e0),
    ("FS_TITLE", 1.0, "ÁÉÍÓÚ", 49, 15, 0x8eff616dd3a2e5ac),
    ("FS_TITLE", 1.0, "Añadir", 50, 13, 0xf05932b9caec0a72),
    ("FS_TITLE", 1.0, "j y g", 32, 14, 0x758e81ca2e4f274f),
    ("FS_TITLE", 1.0, "W.", 19, 12, 0x6174bfe19483ffdb),
    (
        "FS_TITLE",
        1.0,
        "Revisar el PR de la migración",
        222,
        14,
        0x39bb3b2aa8be7aa1,
    ),
    (
        "FS_TITLE",
        1.0,
        "¿Qué hago con el conflicto?",
        218,
        14,
        0x45fb2d576d7e8089,
    ),
    ("FS_TITLE", 1.0, "—", 14, 2, 0x29843638db1ca52a),
    ("FS_TITLE", 1.0, "…", 10, 2, 0xc2cc5302de600ad2),
    ("FS_TITLE", 1.0, "123 %", 44, 12, 0x43d97b87018cff77),
    ("FS_ROW", 1.0, "A", 10, 10, 0x03d76a072300f91c),
    ("FS_ROW", 1.0, "Hola mundo", 77, 10, 0xe1c2b44401f64706),
    ("FS_ROW", 1.0, "Automático", 74, 11, 0x8f2fc6cbbd6d26e6),
    (
        "FS_ROW",
        1.0,
        "Según el sistema",
        111,
        13,
        0xc648b1587b8b9dcb,
    ),
    ("FS_ROW", 1.0, "Hablando…", 69, 10, 0x15c8bc49de3a5a16),
    ("FS_ROW", 1.0, "Español", 50, 13, 0x4ca4caa5b23b0c9d),
    ("FS_ROW", 1.0, "Ñandú", 40, 14, 0xcab06c95bcbfb7d9),
    ("FS_ROW", 1.0, "Über", 30, 13, 0x438b7c96090da8ba),
    ("FS_ROW", 1.0, "¿Qué?", 42, 13, 0xc04f2defb94c77d3),
    ("FS_ROW", 1.0, "¡Hola!", 36, 12, 0xb402fa182878830c),
    ("FS_ROW", 1.0, "ÁÉÍÓÚ", 40, 14, 0xf8c36251cfd6883a),
    ("FS_ROW", 1.0, "Añadir", 41, 11, 0xa928c5261212624b),
    ("FS_ROW", 1.0, "j y g", 29, 12, 0xf69c6688cfaa40d4),
    ("FS_ROW", 1.0, "W.", 16, 10, 0x609fb5481c9e55fa),
    (
        "FS_ROW",
        1.0,
        "Revisar el PR de la migración",
        188,
        13,
        0x499106501e62bcd0,
    ),
    (
        "FS_ROW",
        1.0,
        "¿Qué hago con el conflicto?",
        184,
        13,
        0x125d2bef52c05a0a,
    ),
    ("FS_ROW", 1.0, "—", 13, 2, 0xadbda3c208e55c04),
    ("FS_ROW", 1.0, "…", 8, 2, 0xb36a16bea3b03593),
    ("FS_ROW", 1.0, "123 %", 38, 10, 0xa187bbfb6ca47929),
    ("FS_BODY", 1.0, "A", 9, 9, 0x2fa7aac038cbb7dd),
    ("FS_BODY", 1.0, "Hola mundo", 69, 9, 0xbd2eae28c01f48be),
    ("FS_BODY", 1.0, "Automático", 68, 9, 0x81195d937eab873c),
    (
        "FS_BODY",
        1.0,
        "Según el sistema",
        101,
        11,
        0x9fac95f0bc3359e5,
    ),
    ("FS_BODY", 1.0, "Hablando…", 63, 9, 0x80776dda7e8fbcf8),
    ("FS_BODY", 1.0, "Español", 48, 12, 0xc99e4615c54db851),
    ("FS_BODY", 1.0, "Ñandú", 39, 12, 0xf6cd7a6e8b5a358e),
    ("FS_BODY", 1.0, "Über", 28, 11, 0xa4e25ead8666c891),
    ("FS_BODY", 1.0, "¿Qué?", 38, 11, 0xbdcd4459f269046a),
    ("FS_BODY", 1.0, "¡Hola!", 32, 11, 0x39fd3020428dedb4),
    ("FS_BODY", 1.0, "ÁÉÍÓÚ", 39, 11, 0x22d1c62ac626b22d),
    ("FS_BODY", 1.0, "Añadir", 40, 10, 0x572aad0cc11a0b7f),
    ("FS_BODY", 1.0, "j y g", 24, 11, 0x59d5dff785d63b4a),
    ("FS_BODY", 1.0, "W.", 15, 9, 0xbe14f5167d2dbcc9),
    (
        "FS_BODY",
        1.0,
        "Revisar el PR de la migración",
        169,
        11,
        0xb75a8d8667c4d9fe,
    ),
    (
        "FS_BODY",
        1.0,
        "¿Qué hago con el conflicto?",
        164,
        11,
        0x69a09c44a389c0d9,
    ),
    ("FS_BODY", 1.0, "—", 12, 2, 0xd11552e7aa3279e9),
    ("FS_BODY", 1.0, "…", 7, 2, 0xb461e80d7c5fac62),
    ("FS_BODY", 1.0, "123 %", 34, 9, 0x64ca51fe4c793c3d),
    ("FS_SECONDARY", 1.0, "A", 9, 9, 0x2796ca8d240aacd2),
    ("FS_SECONDARY", 1.0, "Hola mundo", 69, 9, 0x337b3b5d873aedc1),
    (
        "FS_SECONDARY",
        1.0,
        "Automático",
        68,
        10,
        0x0c0e7b08acedfb23,
    ),
    (
        "FS_SECONDARY",
        1.0,
        "Según el sistema",
        100,
        12,
        0x41130a1fee38da1d,
    ),
    ("FS_SECONDARY", 1.0, "Hablando…", 63, 9, 0xedd58538d77a8a4a),
    ("FS_SECONDARY", 1.0, "Español", 46, 12, 0xa8296e5aa08fdc68),
    ("FS_SECONDARY", 1.0, "Ñandú", 37, 12, 0x11a1f5d61ec48ccf),
    ("FS_SECONDARY", 1.0, "Über", 27, 12, 0x312f92d9288a0ed2),
    ("FS_SECONDARY", 1.0, "¿Qué?", 37, 12, 0xcd68b7a15e56a8be),
    ("FS_SECONDARY", 1.0, "¡Hola!", 33, 11, 0xf7a29ec5bd2f099f),
    ("FS_SECONDARY", 1.0, "ÁÉÍÓÚ", 36, 12, 0x962e3ae9ebcc5440),
    ("FS_SECONDARY", 1.0, "Añadir", 38, 10, 0x06111d61277d17a2),
    ("FS_SECONDARY", 1.0, "j y g", 25, 12, 0x6ecd908e17ac308f),
    ("FS_SECONDARY", 1.0, "W.", 15, 9, 0x85c59777cb6ed278),
    (
        "FS_SECONDARY",
        1.0,
        "Revisar el PR de la migración",
        168,
        12,
        0xc06a4ceb1ee8b1e0,
    ),
    (
        "FS_SECONDARY",
        1.0,
        "¿Qué hago con el conflicto?",
        161,
        12,
        0xcf7d11f202ec832d,
    ),
    ("FS_SECONDARY", 1.0, "—", 11, 3, 0x3d102c4fd5ec1b9a),
    ("FS_SECONDARY", 1.0, "…", 8, 2, 0x1cdee45386a64a99),
    ("FS_SECONDARY", 1.0, "123 %", 34, 9, 0x493dd208c817c273),
    ("FS_MICRO", 1.0, "A", 7, 8, 0x57bc79d32992b6e2),
    ("FS_MICRO", 1.0, "Hola mundo", 80, 8, 0x8c8415437e09dc2e),
    ("FS_MICRO", 1.0, "Automático", 80, 10, 0xceb581c3be4dd22a),
    (
        "FS_MICRO",
        1.0,
        "Según el sistema",
        129,
        10,
        0xbdbcdd2376d5876b,
    ),
    ("FS_MICRO", 1.0, "Hablando…", 72, 8, 0xf7e7c1ee4a60f592),
    ("FS_MICRO", 1.0, "Español", 55, 10, 0x10b71cea08d1611f),
    ("FS_MICRO", 1.0, "Ñandú", 38, 10, 0x71b925ae3847876e),
    ("FS_MICRO", 1.0, "Über", 30, 10, 0x39ff039dedfdcea5),
    ("FS_MICRO", 1.0, "¿Qué?", 38, 12, 0xa71c288f059bcbcd),
    ("FS_MICRO", 1.0, "¡Hola!", 44, 10, 0xb95bbcef0150b0b3),
    ("FS_MICRO", 1.0, "ÁÉÍÓÚ", 38, 10, 0xa0a9cbf040aa7976),
    ("FS_MICRO", 1.0, "Añadir", 47, 10, 0x5a502cd8c0d4eaeb),
    ("FS_MICRO", 1.0, "j y g", 39, 8, 0x23df495690d883de),
    ("FS_MICRO", 1.0, "W.", 13, 8, 0xef74f0dfbae18f71),
    (
        "FS_MICRO",
        1.0,
        "Revisar el PR de la migración",
        233,
        10,
        0x704c2765a50eef8b,
    ),
    (
        "FS_MICRO",
        1.0,
        "¿Qué hago con el conflicto?",
        217,
        12,
        0x6cec0fdf2ef52d46,
    ),
    ("FS_MICRO", 1.0, "—", 7, 2, 0xe3fcef84f39ac0a0),
    ("FS_MICRO", 1.0, "…", 7, 2, 0xa77c9150b687163a),
    ("FS_MICRO", 1.0, "123 %", 39, 8, 0xd86ebbc238450b46),
    ("FS_TASK", 1.0, "A", 9, 9, 0x9f5d6fae0f5053b0),
    ("FS_TASK", 1.0, "Hola mundo", 74, 9, 0x2f6baecaa3e18a5e),
    ("FS_TASK", 1.0, "Automático", 72, 10, 0x0a5a40e04e0a913c),
    (
        "FS_TASK",
        1.0,
        "Según el sistema",
        106,
        12,
        0x9b9fc201a3983feb,
    ),
    ("FS_TASK", 1.0, "Hablando…", 66, 9, 0x421bf0b68e01db68),
    ("FS_TASK", 1.0, "Español", 49, 12, 0xfab2a0e8a50401f4),
    ("FS_TASK", 1.0, "Ñandú", 39, 12, 0x8670a3af1497d41b),
    ("FS_TASK", 1.0, "Über", 29, 12, 0xbd96af107c5997e5),
    ("FS_TASK", 1.0, "¿Qué?", 39, 12, 0x2233e9d864979995),
    ("FS_TASK", 1.0, "¡Hola!", 34, 11, 0x2589058882411f46),
    ("FS_TASK", 1.0, "ÁÉÍÓÚ", 40, 12, 0x84171b57afe08784),
    ("FS_TASK", 1.0, "Añadir", 40, 10, 0x50d22da40e0f61e3),
    ("FS_TASK", 1.0, "j y g", 25, 12, 0xd970061d556e41c8),
    ("FS_TASK", 1.0, "W.", 16, 9, 0xa89ea06d801e3b29),
    (
        "FS_TASK",
        1.0,
        "Revisar el PR de la migración",
        174,
        12,
        0x6d87e1b2fbb8c75b,
    ),
    (
        "FS_TASK",
        1.0,
        "¿Qué hago con el conflicto?",
        171,
        12,
        0xc47e2219cb45a0b2,
    ),
    ("FS_TASK", 1.0, "—", 12, 3, 0x4095cc7fb025c608),
    ("FS_TASK", 1.0, "…", 8, 2, 0x051f6928821d011d),
    ("FS_TASK", 1.0, "123 %", 34, 9, 0xe3d908fad4baf3d2),
    ("FS_META", 1.0, "A", 8, 8, 0x3070019f10ce8450),
    ("FS_META", 1.0, "Hola mundo", 60, 8, 0x4d0482d1c02e78b0),
    ("FS_META", 1.0, "Automático", 58, 8, 0xb83eb93b49e652cf),
    (
        "FS_META",
        1.0,
        "Según el sistema",
        88,
        10,
        0x299ad51da126f6c5,
    ),
    ("FS_META", 1.0, "Hablando…", 54, 8, 0x3ffa4d108c87dbd9),
    ("FS_META", 1.0, "Español", 40, 10, 0x7ead7ec5bba2cf67),
    ("FS_META", 1.0, "Ñandú", 32, 10, 0x3344c8110eec90cf),
    ("FS_META", 1.0, "Über", 25, 10, 0x8877ff82035f0c0c),
    ("FS_META", 1.0, "¿Qué?", 32, 10, 0xe6f254aec747d975),
    ("FS_META", 1.0, "¡Hola!", 27, 10, 0xeffee0fe1ba92859),
    ("FS_META", 1.0, "ÁÉÍÓÚ", 32, 10, 0xfb434d2161bf37af),
    ("FS_META", 1.0, "Añadir", 33, 8, 0x87d2660ab6dd9bde),
    ("FS_META", 1.0, "j y g", 22, 10, 0x9e3e3f5ee110ba1f),
    ("FS_META", 1.0, "W.", 12, 8, 0x50821a77e02c3ef3),
    (
        "FS_META",
        1.0,
        "Revisar el PR de la migración",
        148,
        10,
        0x2c0cef2070fa94e9,
    ),
    (
        "FS_META",
        1.0,
        "¿Qué hago con el conflicto?",
        140,
        10,
        0x38bd4d46312ed6f3,
    ),
    ("FS_META", 1.0, "—", 10, 1, 0x15a91ad31990b716),
    ("FS_META", 1.0, "…", 6, 1, 0x747185c322f1e480),
    ("FS_META", 1.0, "123 %", 30, 8, 0x223d7e79484452c3),
    ("FS_BODY", 1.25, "Automático", 86, 13, 0x7e899f1e08a53433),
    (
        "FS_BODY",
        1.25,
        "Según el sistema",
        126,
        15,
        0x12884c1659dcdfce,
    ),
    ("FS_BODY", 1.25, "Hablando…", 79, 12, 0xec28174144cd8b53),
    ("FS_BODY", 1.25, "Ñandú", 47, 16, 0x2e602e1de3126c47),
    ("FS_BODY", 1.25, "Über", 35, 16, 0x077599b686550e88),
    ("FS_BODY", 1.25, "¿Qué?", 48, 15, 0x33000c1afd78fabf),
    ("FS_BODY", 1.25, "j y g", 31, 15, 0x9621813695b97eb1),
    ("FS_BODY", 1.25, "Hola mundo", 87, 12, 0x018db497d3adb2c0),
    ("FS_BODY", 1.5, "Automático", 108, 15, 0x561fdcabb590e867),
    (
        "FS_BODY",
        1.5,
        "Según el sistema",
        158,
        18,
        0x3bb723ed712f5472,
    ),
    ("FS_BODY", 1.5, "Hablando…", 98, 14, 0x30eda7844989ea63),
    ("FS_BODY", 1.5, "Ñandú", 59, 19, 0x1eade02a59490ba9),
    ("FS_BODY", 1.5, "Über", 43, 18, 0xb3173f3981070ae8),
    ("FS_BODY", 1.5, "¿Qué?", 60, 18, 0x6e0f7fc7dcb79b70),
    ("FS_BODY", 1.5, "j y g", 38, 17, 0x6d1482c00d7d486a),
    ("FS_BODY", 1.5, "Hola mundo", 110, 14, 0x7be0599210a36077),
    ("FS_BODY", 2.0, "Automático", 138, 19, 0x942789139e7ef7be),
    (
        "FS_BODY",
        2.0,
        "Según el sistema",
        208,
        23,
        0xcfb31e8a3e2dcb72,
    ),
    ("FS_BODY", 2.0, "Hablando…", 126, 18, 0x874021c9b7dc7fc5),
    ("FS_BODY", 2.0, "Ñandú", 74, 24, 0x07a1e3c503f0cdf2),
    ("FS_BODY", 2.0, "Über", 55, 24, 0x8726aad4ef5b2234),
    ("FS_BODY", 2.0, "¿Qué?", 77, 22, 0xa2d97272c2fbb7dd),
    ("FS_BODY", 2.0, "j y g", 50, 23, 0x662341ab2506d530),
    ("FS_BODY", 2.0, "Hola mundo", 142, 18, 0x00a07a3ccbf4f055),
    ("FS_ROW", 1.25, "Automático", 96, 14, 0x0d418c5e902cf7f6),
    (
        "FS_ROW",
        1.25,
        "Según el sistema",
        142,
        17,
        0xdcc2622f30b7d6e8,
    ),
    ("FS_ROW", 1.25, "Hablando…", 89, 13, 0x6cf3316f2102e20e),
    ("FS_ROW", 1.25, "Ñandú", 52, 17, 0x155bd37ab78110ff),
    ("FS_ROW", 1.25, "Über", 39, 17, 0xc201a86340d9eaa6),
    ("FS_ROW", 1.25, "¿Qué?", 53, 17, 0xa95857deb16e1ba2),
    ("FS_ROW", 1.25, "j y g", 36, 17, 0xcfd22beb9b5d3ffe),
    ("FS_ROW", 1.25, "Hola mundo", 99, 13, 0xb45cd17731983eb4),
    ("FS_ROW", 1.5, "Automático", 112, 16, 0x564f9dac1c188798),
    (
        "FS_ROW",
        1.5,
        "Según el sistema",
        165,
        19,
        0x431790f78b7b331b,
    ),
    ("FS_ROW", 1.5, "Hablando…", 106, 16, 0x259e9d7dc37067ac),
    ("FS_ROW", 1.5, "Ñandú", 63, 20, 0x5192680e2ea033e0),
    ("FS_ROW", 1.5, "Über", 46, 20, 0x7bdb17925fd0f380),
    ("FS_ROW", 1.5, "¿Qué?", 62, 19, 0x6f5003c14c15107d),
    ("FS_ROW", 1.5, "j y g", 38, 19, 0x4bc498d270409db8),
    ("FS_ROW", 1.5, "Hola mundo", 116, 16, 0xf3c28c069ab4e06b),
    ("FS_ROW", 2.0, "Automático", 150, 21, 0xab22b4ff0eb19a41),
    (
        "FS_ROW",
        2.0,
        "Según el sistema",
        223,
        25,
        0xe7c064b27dc5ea12,
    ),
    ("FS_ROW", 2.0, "Hablando…", 136, 20, 0xa5484f459d9160d5),
    ("FS_ROW", 2.0, "Ñandú", 81, 26, 0x3f0dae9536dac533),
    ("FS_ROW", 2.0, "Über", 60, 25, 0x3fa4f0ee4ed9f21d),
    ("FS_ROW", 2.0, "¿Qué?", 83, 25, 0x3715b4ef06af3d6f),
    ("FS_ROW", 2.0, "j y g", 52, 24, 0xf569d5be1bba6204),
    ("FS_ROW", 2.0, "Hola mundo", 152, 20, 0x1db2afde686fcf52),
];

/// FNV-1a 64-bit, the same hash as the Pillow vector generator.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn style(token: &str) -> FontStyle {
    match token {
        "FS_TITLE" => FS_TITLE,
        "FS_ROW" => FS_ROW,
        "FS_BODY" => FS_BODY,
        "FS_SECONDARY" => FS_SECONDARY,
        "FS_MICRO" => FS_MICRO,
        "FS_TASK" => FS_TASK,
        "FS_META" => FS_META,
        other => panic!("unknown style {other:?}"),
    }
}

/// Draws `text` at the origin and returns `(width, height, coverage)`.
fn mask(text: &str, style: FontStyle, scale: f64) -> (usize, usize, Vec<u8>) {
    let width = 720usize;
    let height = 80usize;
    let mut data = vec![0.0f32; width * height * 4];
    let mut surface = FloatSurface::new(&mut data, width, height);
    let size = draw_text(
        &mut surface,
        text,
        style,
        Color::Rgb([255, 255, 255]),
        (0.0, 0.0),
        TextOptions {
            scale,
            ..TextOptions::default()
        },
    )
    .expect("the case uses a valid style and scale");
    let mask_width = (size.0 * scale).round_ties_even() as usize;
    let mask_height = (size.1 * scale).round_ties_even() as usize;
    let mut coverage = Vec::with_capacity(mask_width * mask_height);
    for y in 0..mask_height {
        for x in 0..mask_width {
            let alpha = surface.data[(y * width + x) * 4 + 3];
            coverage.push((alpha * 255.0).round_ties_even() as u8);
        }
    }
    (mask_width, mask_height, coverage)
}

/// The linked rasterizer: Pillow's 2.14.3 or the vendored 2.13.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rasterizer {
    Pillow,
    Vendored,
}

/// U+00B4 at `FS_BODY` 1.0: the two known interpreters render it differently.
fn probe() -> Rasterizer {
    let (width, height, coverage) = mask("\u{00b4}", FS_BODY, 1.0);
    match (width, height, fnv1a(&coverage)) {
        (3, 2, 0x3941_4b22_f6f8_eae0) => Rasterizer::Pillow,
        (3, 2, 0x0d6a_4504_cce2_97be) => Rasterizer::Vendored,
        _ => panic!(
            "unknown rasterizer for U+00B4: {width}x{height} {:?}",
            coverage
        ),
    }
}

#[test]
fn every_text_mask_matches_pillow() {
    let rasterizer = probe();
    let mut cases = 0usize;
    for &(token, scale, text, width, height, hash) in CASES {
        let style = style(token);
        let size = text_size(text, style, scale, None, "left").expect("valid size");
        let expected_width = width as f64 / scale;
        let expected_height = height as f64 / scale;
        assert_eq!(
            size.0, expected_width,
            "{token} {scale} {text:?}: advance differs"
        );
        let (mask_width, mask_height, coverage) = mask(text, style, scale);
        match rasterizer {
            Rasterizer::Pillow => {
                assert_eq!(
                    size.1, expected_height,
                    "{token} {scale} {text:?}: ink height differs"
                );
                assert_eq!(
                    (mask_width, mask_height),
                    (width, height),
                    "{token} {scale} {text:?}: mask size differs"
                );
                assert_eq!(
                    fnv1a(&coverage),
                    hash,
                    "{token} {scale} {text:?}: coverage differs"
                );
            }
            Rasterizer::Vendored => {
                // FreeType 2.13.2 hints accent composites up to one pixel
                // taller; the advance and the coverage width are exact.
                assert!(
                    (size.1 - expected_height).abs() <= 1.0,
                    "{token} {scale} {text:?}: height {} vs {expected_height}",
                    size.1
                );
                assert_eq!(
                    mask_width, width,
                    "{token} {scale} {text:?}: mask width differs"
                );
                assert!(
                    mask_height.abs_diff(height) <= 1,
                    "{token} {scale} {text:?}: mask height {mask_height} vs {height}"
                );
            }
        }
        cases += 1;
    }
    assert_eq!(cases, CASES.len());
    assert!(cases >= 181, "the table lost cases");
}

#[test]
fn empty_text_and_spaces_keep_the_python_metrics() {
    let (width, height) = text_size("", FS_BODY, 1.0, None, "left").expect("size");
    assert_eq!((width, height), (0.0, 18.0));
    let (width, height) = text_size("   ", FS_BODY, 1.0, None, "left").expect("size");
    assert_eq!((width, height), (9.0, 18.0));

    let (mask_width, mask_height, coverage) = mask("", FS_BODY, 1.0);
    assert_eq!((mask_width, mask_height), (0, 0));
    assert!(coverage.is_empty());
    assert_eq!(fnv1a(&coverage), 0xcbf2_9ce4_8422_2325);
}

#[test]
fn the_rasterizer_probe_is_documented() {
    assert!(matches!(probe(), Rasterizer::Pillow | Rasterizer::Vendored));
}
