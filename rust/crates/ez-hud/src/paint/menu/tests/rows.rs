//! Pruebas de las filas en capas aisladas (F5-10a).
//!
//! Los valores esperados salieron de ejecutar `menu.py` con el `.venv` del
//! proyecto. Las zonas de texto se evitan: la versión de FreeType de `ez-gfx`
//! no es la misma que la de Pillow.

use ez_gfx::sdf::Layer;
use ez_gfx::text::FS_BODY;

use crate::model::settings::SettingValue;

use super::super::{controls, fit_text, NoExtras};
use super::{assert_pixel, draw_layer_by, menu, pixel};

#[test]
fn palette_stops_match_python() {
    let menu = menu();
    let light = [
        [14.0, 111.0, 124.0],
        [31.0, 174.0, 159.0],
        [143.0, 227.0, 192.0],
        [255.0, 138.0, 101.0],
    ];
    let dark = [
        [5.0, 38.0, 46.0],
        [12.0, 102.0, 112.0],
        [41.0, 191.0, 169.0],
        [255.0, 122.0, 89.0],
    ];
    assert_eq!(controls::palette_stops(&menu, "light"), light);
    assert_eq!(controls::palette_stops(&menu, "dark"), dark);

    let mut custom = menu;
    custom
        .settings
        .set("theme_primary", &SettingValue::from("#112233"))
        .expect("color");
    custom
        .settings
        .set("theme_secondary", &SettingValue::from("#445566"))
        .expect("color");
    custom
        .settings
        .set("theme_accent", &SettingValue::from("#778899"))
        .expect("color");
    custom
        .settings
        .set("theme_background", &SettingValue::from("#aabbcc"))
        .expect("color");
    assert_eq!(
        controls::palette_stops(&custom, "custom"),
        [
            [170.0, 187.0, 204.0],
            [17.0, 34.0, 51.0],
            [68.0, 85.0, 102.0],
            [119.0, 136.0, 153.0],
        ]
    );

    custom
        .settings
        .set("theme_primary", &SettingValue::from("nope"))
        .expect("color inválido");
    // `settings.set` ignora un color mal escrito y conserva el anterior, así
    // que la caída de `_palette_stops` a «light» no se alcanza desde la API.
    assert_eq!(
        controls::palette_stops(&custom, "custom"),
        [
            [170.0, 187.0, 204.0],
            [17.0, 34.0, 51.0],
            [68.0, 85.0, 102.0],
            [119.0, 136.0, 153.0],
        ]
    );
}

#[test]
fn palette_row_matches_python() {
    let menu = menu();
    let mut layer = Layer::new(320, 120);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_palette_row(menu, ctx, 0.0, 1.0);
    });
    assert_pixel(
        pixel(&layer, 34, 20),
        [0.063_725_49, 0.467_993_1, 0.504_440_6, 1.0],
        "muestra 1",
    );
    assert_pixel(
        pixel(&layer, 70, 25),
        [0.338_408_26, 0.784_964_56, 0.687_419_65, 1.0],
        "muestra 2",
    );
    assert_pixel(
        pixel(&layer, 108, 30),
        [0.419_868_47, 0.244_837_69, 0.182_237_73, 0.433_054_95],
        "muestra 3",
    );
    assert_pixel(
        pixel(&layer, 125, 20),
        [0.024_971_163, 0.198_055_67, 0.230_960_59, 1.0],
        "muestra 4",
    );
    assert_pixel(pixel(&layer, 205, 20), [0.0, 0.0, 0.0, 0.0], "hueco");
    assert_pixel(
        pixel(&layer, 288, 25),
        [0.895_085_87, 0.494_412_15, 0.363_583_2, 0.902_581_45],
        "borde",
    );
}

#[test]
fn active_palette_gets_the_selection_stroke() {
    let mut menu = menu();
    menu.settings
        .set("theme", &SettingValue::from("dark"))
        .expect("tema");
    let mut layer = Layer::new(320, 120);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_palette_row(menu, ctx, 0.0, 1.0);
    });
    assert_pixel(
        pixel(&layer, 34, 20),
        [0.063_725_49, 0.467_993_1, 0.504_440_6, 1.0],
        "claro",
    );
    assert_pixel(
        pixel(&layer, 125, 20),
        [0.024_971_163, 0.198_055_67, 0.230_960_59, 1.0],
        "oscuro",
    );
    assert_pixel(
        pixel(&layer, 288, 25),
        [0.895_085_87, 0.494_412_15, 0.363_583_2, 0.902_581_45],
        "personalizado",
    );
}

#[test]
fn meter_fraction_matches_python() {
    let expected = [0.0, 0.0, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.0, 1.0];
    let values = [0.0, -1.0, 1e-5, 0.001, 0.01, 0.1, 0.316_227_766, 1.0];
    for (value, expected) in values.iter().zip(expected) {
        let actual = controls::meter_fraction(*value);
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

#[test]
fn meter_row_matches_python() {
    let mut menu = menu();
    menu.meter_level = 0.01;
    let mut layer = Layer::new(320, 80);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_meter_row(menu, ctx, "input_meter", 0.0, 1.0, 0.0);
    });
    assert_pixel(
        pixel(&layer, 250, 38),
        [0.172_549_02, 0.274_509_82, 0.298_039_23, 1.0],
        "pista",
    );
    assert_pixel(
        pixel(&layer, 50, 38),
        [0.368_627_46, 0.886_274_5, 0.768_627_46, 1.0],
        "nivel",
    );
    assert_pixel(
        pixel(&layer, 98, 38),
        [0.894_117_65, 0.949_019_6, 0.941_176_47, 1.0],
        "pulgar",
    );
    assert_pixel(
        pixel(&layer, 90, 38),
        [0.532_270_13, 0.905_814, 0.822_360_9, 1.0],
        "borde del pulgar",
    );
}

#[test]
fn text_row_background_and_caret_match_python() {
    let mut menu = menu();
    menu.editing_key = Some("wake_phrase".into());
    menu.edit_buffer.clear();
    menu.edit_cursor = 0;
    menu.edit_t0 = 0.0;
    let mut layer = Layer::new(320, 80);
    let mut extras = NoExtras;
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_text_row(menu, ctx, "wake_phrase", 0.0, 1.0, 0.0, &mut extras);
    });
    assert_pixel(
        pixel(&layer, 280, 46),
        [0.172_549_02, 0.274_509_82, 0.298_039_23, 1.0],
        "fondo",
    );
    assert_pixel(
        pixel(&layer, 31, 37),
        [0.368_627_46, 0.886_274_5, 0.768_627_46, 1.0],
        "cursor",
    );
    assert_pixel(
        pixel(&layer, 32, 37),
        [0.270_588_25, 0.580_392_18, 0.533_333_36, 1.0],
        "borde del cursor",
    );
}

#[test]
fn select_row_chevrons_follow_the_selector_fraction() {
    let mut menu = menu();
    let mut layer = Layer::new(320, 80);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_select_row(menu, ctx, "tts_model_variant", 0.0, 22.0, 1.0, false);
    });
    assert_pixel(
        pixel(&layer, 281, 22),
        [0.473_928, 0.563_218, 0.559_784, 0.875_736],
        "chevron abajo",
    );

    menu.selector_key = Some("tts_model_variant".into());
    menu.chevron
        .get_mut("tts_model_variant")
        .expect("fila de selector")
        .value = 0.5;
    let mut layer = Layer::new(320, 80);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_select_row(menu, ctx, "tts_model_variant", 0.0, 22.0, 1.0, false);
    });
    assert_pixel(
        pixel(&layer, 281, 22),
        [0.236_964, 0.281_609, 0.279_892, 0.437_868],
        "chevron a medias",
    );

    let mut layer = Layer::new(320, 80);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_select_row(menu, ctx, "voice", 0.0, 22.0, 1.0, true);
    });
    assert_pixel(
        pixel(&layer, 281, 22),
        [0.189_571, 0.225_287, 0.223_914, 0.350_294],
        "chevron deshabilitado",
    );
}

#[test]
fn text_value_color_uses_text_while_editing() {
    let palette = crate::tokens::DARK.clone();
    assert_eq!(controls::value_color(&palette, true, ""), palette["TEXT"]);
    assert_eq!(
        controls::value_color(&palette, true, "algo"),
        palette["TEXT"]
    );
    assert_eq!(
        controls::value_color(&palette, false, ""),
        palette["TEXT_3"]
    );
    assert_eq!(
        controls::value_color(&palette, false, "algo"),
        palette["TEXT_2"]
    );
}

#[test]
fn color_swatch_matches_python() {
    let mut menu = menu();
    menu.settings
        .set("theme_primary", &SettingValue::from("#ff0000"))
        .expect("color");
    let mut layer = Layer::new(320, 80);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        controls::draw_color_swatch(menu, ctx, "theme_primary", 0.0, 1.0);
    });
    assert_pixel(pixel(&layer, 278, 26), [1.0, 0.0, 0.0, 1.0], "centro");
    assert_pixel(
        pixel(&layer, 278, 15),
        [0.989_103_2, 0.001_062_725_2, 0.001_053_942_4, 0.989_221_8],
        "borde",
    );
}

#[test]
fn fit_text_truncates_with_the_real_ink_width() {
    assert_eq!(fit_text("Hola mundo", FS_BODY, 40.0, 1.0), "Hola…");
    assert_eq!(fit_text("Hola mundo", FS_BODY, 1000.0, 1.0), "Hola mundo");
    assert_eq!(fit_text("", FS_BODY, 100.0, 1.0), "");
}
