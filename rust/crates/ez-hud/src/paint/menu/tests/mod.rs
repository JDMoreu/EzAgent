//! Pruebas de `paint::menu` (F5-10a).
//!
//! Los valores esperados salieron de ejecutar `menu.py` con el `.venv` del
//! proyecto (`_draw_palette_row`, `_draw_meter_row`, `_draw_header`,
//! `_draw_group`, `_draw_text_row`, `_draw_color_swatch`, `_layers_static`,
//! `frame_key`, `_palette_stops` y `_meter_fraction`). Las zonas de texto se
//! evitan: la versión de FreeType de `ez-gfx` no es la misma que la de Pillow.

use ez_gfx::sdf::Layer;

use crate::model::menu::geometry::{Menu, MenuCallbacks, MenuOptions};
use crate::model::settings::{MenuSettings, SettingValue};

use super::sections;
use super::{FrameExtras, NoExtras, PaintContext};

mod rows;

fn menu() -> Menu {
    Menu::new(
        MenuSettings::default(),
        MenuOptions::default(),
        MenuCallbacks::default(),
    )
    .expect("opciones válidas")
}

fn opened(now: f64) -> Menu {
    let mut menu = menu();
    menu.open(0.0);
    menu.update(now);
    menu
}

fn pixel(layer: &Layer, x: u32, y: u32) -> [f32; 4] {
    let index = (y as usize * layer.width() as usize + x as usize) * 4;
    let raw = &layer.as_raw()[index..index + 4];
    [raw[0], raw[1], raw[2], raw[3]]
}

fn assert_pixel(actual: [f32; 4], expected: [f64; 4], label: &str) {
    for channel in 0..4 {
        let difference = (f64::from(actual[channel]) - expected[channel]).abs();
        assert!(
            difference <= 1e-6,
            "{label} canal {channel}: {} != {}",
            actual[channel],
            expected[channel]
        );
    }
}

fn draw_layer_by(layer: &mut Layer, menu: &Menu, paint: impl FnOnce(&Menu, &mut PaintContext)) {
    let palette = menu.palette();
    let mut ctx = PaintContext::new(layer, menu.scale, menu.dx, menu.theme, &palette);
    paint(menu, &mut ctx);
}

// --------------------------------------------------------------------------
// Reloj y huella
// --------------------------------------------------------------------------

#[test]
fn layers_static_matches_the_python_clock() {
    let menu = menu();
    assert!(menu.layers_static(0.0));
    let key = menu.frame_key(0.0).expect("el menú quieto tiene huella");
    assert!(!key.open);
    assert_eq!(key.height, 0);
    assert_eq!(key.content_alpha, 0);
    assert_eq!(key.scroll, 0);
    assert_eq!(key.indicator_alpha, 0);

    let mut open = menu;
    open.open(0.0);
    open.update(0.05);
    assert!(!open.layers_static(0.05));
    open.update(0.95);
    assert!(!open.layers_static(0.95));
    open.update(1.0);
    assert!(open.layers_static(1.0));
    let key = open
        .frame_key(1.0)
        .expect("el menú abierto y quieto tiene huella");
    assert!(key.open);
    assert_eq!(key.height, 560_000);
    assert_eq!(key.content_alpha, 1_000);
    assert_eq!(key.scroll, 0);
    assert_eq!(key.indicator_alpha, 0);
}

#[test]
fn layers_static_follows_editing_testing_selector_and_footer() {
    let mut editing = opened(1.0);
    editing.editing_key = Some("wake_phrase".into());
    assert!(!editing.layers_static(1.0));

    let mut testing = opened(1.0);
    testing.testing_t.value = 0.5;
    assert!(!testing.layers_static(1.0));

    let mut selector = opened(1.0);
    selector.selector_key = Some("voice".into());
    selector.selector_closing = false;
    selector.selector_item_start = 1.0;
    assert!(!selector.layers_static(1.1));
    assert!(selector.layers_static(1.2));

    let mut check = menu();
    check.check_until = 5.0;
    assert!(!check.layers_static(4.0));
    assert!(check.layers_static(6.0));

    let mut footer = menu();
    footer.footer_prev = Some(("GUARDADO".into(), "MINT".into(), false));
    assert!(!footer.layers_static(0.0));

    let mut reduced = menu();
    reduced.set_reduced_motion(true);
    reduced.open(0.0);
    reduced.update(0.05);
    assert!(!reduced.layers_static(0.05));
    reduced.update(0.15);
    assert!(reduced.layers_static(0.15));
}

#[test]
fn signature_is_stable_for_equal_menus_and_changes_with_content() {
    let base = menu();
    let other = menu();
    // La huella incluye `palette_version()`, un contador global que otras
    // pruebas del crate incrementan en paralelo; si cambió entre las dos
    // medidas, comparar los valores no dice nada.
    let version = crate::tokens::palette_version();
    let base_signature = base.layer_signature_value(0.0);
    let other_signature = other.layer_signature_value(0.0);
    if crate::tokens::palette_version() == version {
        assert_eq!(base_signature, other_signature);
    }

    let mut retitled = menu();
    retitled.title = "OTRA SECCIÓN".into();
    assert_ne!(
        base.layer_signature_value(0.0),
        retitled.layer_signature_value(0.0)
    );

    let mut themed = menu();
    themed
        .settings
        .set("theme", &SettingValue::from("dark"))
        .expect("tema");
    assert_ne!(
        base.layer_signature_value(0.0),
        themed.layer_signature_value(0.0)
    );

    let mut scaled = menu();
    scaled.set_scale(1.25).expect("escala válida");
    assert_ne!(
        base.layer_signature_value(0.0),
        scaled.layer_signature_value(0.0)
    );

    let mut hovered = menu();
    hovered.hover_id = Some("voice".into());
    assert_ne!(
        base.layer_signature_value(0.0),
        hovered.layer_signature_value(0.0)
    );

    let mut measured = menu();
    measured.meter_level = 0.5;
    assert_ne!(
        base.layer_signature_value(0.0),
        measured.layer_signature_value(0.0)
    );
}

// --------------------------------------------------------------------------
// Cabecera y grupos
// --------------------------------------------------------------------------

#[test]
fn header_halo_and_close_icon_match_python() {
    let mut menu = menu();
    menu.blocks
        .get_mut("header")
        .expect("bloque de cabecera")
        .value = 1.0;
    menu.content_alpha.value = 1.0;
    menu.button_hover
        .get_mut("hide")
        .expect("botón de ocultar")
        .value = 1.0;
    let layout = menu.layout();
    let mut layer = Layer::new(320, 120);
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        sections::draw_header(menu, ctx, &layout, 0.0);
    });
    assert_pixel(pixel(&layer, 289, 72), [0.08, 0.08, 0.08, 0.08], "velo");
    assert_pixel(
        pixel(&layer, 298, 62),
        [0.08, 0.08, 0.08, 0.08],
        "velo derecho",
    );
    assert_pixel(
        pixel(&layer, 289, 62),
        [0.674_509_8, 0.768_627_46, 0.760_784_3, 1.0],
        "icono",
    );
    assert_pixel(
        pixel(&layer, 289, 74),
        [0.0, 0.0, 0.0, 0.0],
        "borde del velo",
    );
}

#[test]
fn group_background_hover_and_hairline_match_python() {
    let mut menu = menu();
    menu.blocks.get_mut("look").expect("bloque de forma").value = 1.0;
    menu.content_alpha.value = 1.0;
    menu.row_hover
        .get_mut("ui_layout")
        .expect("fila de forma")
        .value = 0.5;
    let layout = menu.layout();
    let rows = menu
        .shown_groups()
        .into_iter()
        .find(|(name, _rows)| name == "look")
        .expect("el grupo existe")
        .1;
    let mut layer = Layer::new(320, 1000);
    let mut extras = NoExtras;
    draw_layer_by(&mut layer, &menu, |menu, ctx| {
        sections::draw_group(menu, ctx, "look", &rows, &layout, 0.0, 0.0, &mut extras);
    });
    assert_pixel(
        pixel(&layer, 150, 113),
        [0.094_117_65, 0.152_941_18, 0.172_549_02, 1.0],
        "hover",
    );
    assert_pixel(
        pixel(&layer, 295, 200),
        [0.078_431_375, 0.129_411_77, 0.149_019_61, 1.0],
        "fondo",
    );
    assert_pixel(
        pixel(&layer, 100, 154),
        [0.102_422_15, 0.153_517_89, 0.172_318_35, 1.0],
        "hairline",
    );
    assert_pixel(pixel(&layer, 310, 200), [0.0, 0.0, 0.0, 0.0], "fuera");
}

// --------------------------------------------------------------------------
// Composición completa
// --------------------------------------------------------------------------

#[test]
fn draw_composes_the_menu_like_python() {
    let mut menu = opened(1.0);
    menu.meter_target = 0.01;
    menu.meter_level = 0.01;
    menu.button_hover
        .get_mut("hide")
        .expect("botón de ocultar")
        .value = 1.0;
    let mut buf = Layer::new(320, 640);
    menu.draw(&mut buf, 1.0);

    assert_eq!(menu.layer_shape, Some((640, 320, 4)));
    assert!(menu.layers.is_some());
    assert_pixel(pixel(&buf, 289, 72), [0.08, 0.08, 0.08, 0.08], "velo");
    assert_pixel(
        pixel(&buf, 295, 200),
        [0.078_431_375, 0.129_411_77, 0.149_019_61, 1.0],
        "grupo",
    );
    assert_pixel(
        pixel(&buf, 100, 154),
        [0.102_422_15, 0.153_517_89, 0.172_318_35, 1.0],
        "hairline",
    );
    assert_pixel(
        pixel(&buf, 70, 350),
        [0.338_408_26, 0.784_964_56, 0.687_419_65, 1.0],
        "muestra",
    );
    assert_pixel(
        pixel(&buf, 115, 350),
        [0.078_431_375, 0.129_411_77, 0.149_019_61, 1.0],
        "hueco",
    );
    assert_pixel(
        pixel(&buf, 250, 516),
        [0.172_549_02, 0.274_509_82, 0.298_039_23, 1.0],
        "pista",
    );
    assert_pixel(
        pixel(&buf, 50, 516),
        [0.368_627_46, 0.886_274_5, 0.768_627_46, 1.0],
        "nivel",
    );
    assert_pixel(
        pixel(&buf, 98, 516),
        [0.894_117_65, 0.949_019_6, 0.941_176_47, 1.0],
        "pulgar",
    );
    assert_pixel(pixel(&buf, 310, 200), [0.0, 0.0, 0.0, 0.0], "fuera");

    let maximum = buf
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0].max(pixel[1]).max(pixel[2]) - pixel[3])
        .fold(f32::MIN, f32::max);
    assert!(maximum <= 0.0, "el RGB premultiplicado no supera el alfa");
}

#[test]
fn draw_reuses_the_layers_and_reacts_to_a_resize() {
    let mut menu = opened(1.0);
    let mut buf = Layer::new(320, 640);
    // `palette_version()` es global y otra prueba puede subirlo a mitad;
    // en ese caso la segunda pasada repinta y cambia la huella guardada,
    // así que solo se compara si el contador no se movió.
    let version = crate::tokens::palette_version();
    menu.draw(&mut buf, 1.0);
    let signature = menu.layer_signature.expect("huella guardada");
    let first = pixel(&buf, 295, 200);
    menu.draw(&mut buf, 1.0);
    if crate::tokens::palette_version() == version {
        assert_eq!(menu.layer_signature, Some(signature));
    }
    assert_eq!(pixel(&buf, 295, 200), first);

    let mut wider = Layer::new(400, 640);
    menu.draw(&mut wider, 1.0);
    assert_eq!(menu.layer_shape, Some((640, 400, 4)));
}

#[test]
fn closed_menu_draws_nothing() {
    let mut menu = menu();
    let mut buf = Layer::new(320, 640);
    menu.draw(&mut buf, 0.0);
    assert!(menu.layers.is_none());
    assert!(buf.as_raw().iter().all(|value| *value == 0.0));

    let mut closed = opened(1.0);
    closed.close(1.0);
    closed.update(2.0);
    let mut buf = Layer::new(320, 640);
    closed.draw(&mut buf, 2.0);
    assert!(buf.as_raw().iter().all(|value| *value == 0.0));
}

/// El trait debe poder implementarse desde `advanced.rs` sin tocar este crate.
#[test]
fn frame_extras_defaults_are_no_ops() {
    struct Empty;
    impl FrameExtras for Empty {}
    let mut extras = Empty;
    let menu = menu();
    let palette = crate::tokens::DARK.clone();
    let mut layer = Layer::new(320, 64);
    let mut ctx = PaintContext::new(&mut layer, 1.0, 0.0, "dark", &palette);
    extras.draw_slider_row(&menu, &mut ctx, "speed", 0.0, 1.0);
    extras.draw_toggle_row(&menu, &mut ctx, "smart_turn", 0.0, 1.0);
    assert!(layer.as_raw().iter().all(|value| *value == 0.0));
}
