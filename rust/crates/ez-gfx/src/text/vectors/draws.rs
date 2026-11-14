pub(crate) type DrawVector = (
    &'static str,
    &'static str,
    [u8; 4],
    f64,
    f64,
    f64,
    &'static str,
    Option<f64>,
    f64,
    u64,
);

pub(crate) const DRAWS: &[DrawVector] = &[
    (
        "AV",
        "FS_TITLE",
        [20, 200, 100, 200],
        3.0,
        2.0,
        1.5,
        "right",
        Some(0.05),
        0.75,
        10630948102321157165,
    ),
    (
        "HOLA",
        "FS_MICRO",
        [1, 2, 3, 255],
        60.0,
        5.0,
        2.0,
        "left",
        None,
        1.0,
        1830121386526361432,
    ),
    (
        "W.",
        "FS_MICRO",
        [255, 255, 255, 255],
        -5.0,
        -3.0,
        1.25,
        "center",
        None,
        1.0,
        16052611768508144549,
    ),
];

/// Pillow 13 (FreeType 2.14.3) mask for `FS_ROW`/`Hola` at scale 1; the
/// bundled 2.13.2 differs by at most 16/255 per pixel (same box and layout).
pub(crate) const NEAR_MASK_HEX: &str = "b87c0000000084b000000000000000000000e04c000000000000000000b87c0000000084b000000000000000000000e04c000000000000000000b87c0000000084b000000e9deeefa00f0000e04c000022b5f2e27f0100b87c0000000084b00000b9d14845ccbc0000e04c0003d9ac3852e95f00b8ffffffffffffb0002bfe26000022fd2e00e04c0016870d000081ab00b8922c2c2c2c99b00058e700000000e35c00e04c0000023c7aaae4c300b87c0000000084b00059e700000000e45b00e04c0008c5d891638fc400b87c0000000084b0002bfe26000022fd2e00e04c0051ea0400009cc400b87c0000000084b00000b9d04745cbbd0000dc680845fa5b3780f3d915b87c0000000084b000000fa0eff0a310000083fa680088eaf4aa38e66b";
