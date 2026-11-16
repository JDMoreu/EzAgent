//! Pruebas de `paint::menu::advanced` (F5-10b).
//!
//! Los valores esperados salieron de ejecutar `menu.py` con el `.venv` del
//! proyecto (`_draw_keycap`, `_draw_secret_value`, `_draw_slider_row`,
//! `_draw_toggle_row`, `_draw_action_row`, `_draw_generic_action_row`,
//! `_draw_selector_list`, `_draw_footer`, `_draw_footer_state`, `_draw_scroll`
//! y `_draw_copy_button`). Las zonas de texto se evitan: la versión de FreeType
//! de `ez-gfx` no es la misma que la de Pillow.

use ez_gfx::sdf::Layer;

use crate::model::menu::geometry::{Menu, MenuCallbacks, MenuOptions};
use crate::model::settings::{MenuSettings, LABELS, SECRET_KEYS, SLIDERS};
use crate::tokens::Rect;

use super::super::PaintContext;
use super::{scaled_rect, BUTTON_RADIUS, KEYCAP_H, TOGGLE_THUMB, TOGGLE_X0};

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

fn layer() -> Layer {
    Layer::new(400, 600)
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

fn draw(menu: &Menu, paint: impl FnOnce(&mut PaintContext<'_>)) -> Layer {
    let palette = menu.palette();
    let mut target = layer();
    {
        let mut ctx = PaintContext::new(&mut target, menu.scale, menu.dx, menu.theme, &palette);
        paint(&mut ctx);
    }
    target
}

/// `pal["TEXT"]` del tema oscuro, para atajos y rótulos.
fn text() -> [f64; 4] {
    [0.894_117_7, 0.949_019_6, 0.941_176_5, 1.0]
}

fn text_2() -> [f64; 4] {
    [0.674_509_8, 0.768_627_5, 0.760_784_3, 1.0]
}

fn track() -> [f64; 4] {
    [0.172_549, 0.274_509_8, 0.298_039_2, 1.0]
}

fn mint_btn() -> [f64; 4] {
    [0.368_627_5, 0.886_274_5, 0.768_627_5, 1.0]
}

fn warm_btn() -> [f64; 4] {
    [1.0, 0.698_039_2, 0.243_137_3, 1.0]
}

// --------------------------------------------------------------------------
// Ayudantes geométricos
// --------------------------------------------------------------------------

#[test]
fn scaled_rect_shrinks_around_the_center() {
    let shrunk = scaled_rect(Rect::new(217.0, 6.0, 289.0, 34.0), 0.96);
    assert!((shrunk.x0 - 218.44).abs() < 1e-9);
    assert!((shrunk.x1 - 287.56).abs() < 1e-9);
    assert!((shrunk.y0 - 6.56).abs() < 1e-9);
    assert!((shrunk.y1 - 33.44).abs() < 1e-9);
    assert_eq!(
        scaled_rect(Rect::new(0.0, 0.0, 10.0, 20.0), 1.0),
        Rect::new(0.0, 0.0, 10.0, 20.0)
    );
    assert!((BUTTON_RADIUS - 14.0).abs() < 1e-9);
}

// --------------------------------------------------------------------------
// Teclas, atajos y secretos
// --------------------------------------------------------------------------

#[test]
fn a_keycap_fills_its_pill_and_advances_past_the_gap() {
    let menu = menu();
    let mut next = 0.0;
    let target = draw(&menu, |ctx| {
        next = super::keycaps::draw_keycap(ctx, 31.0, 60.0, "Ctrl", 1.0, false, false);
    });
    assert_pixel(
        pixel(&target, 35, 60),
        [0.138_039_2, 0.219_607_9, 0.238_431_4, 0.8],
        "relleno de la tecla",
    );
    assert_pixel(
        pixel(&target, 40, 71),
        [0.0, 0.0, 0.0, 0.0],
        "fuera de la tecla",
    );
    assert!(next >= 31.0 + KEYCAP_H + 4.0);
}

#[test]
fn secret_values_are_dots_and_never_the_key() {
    let menu = menu();
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_secret_value(ctx, "abcd", false, 37.0, 1.0);
    });
    assert_pixel(pixel(&target, 33, 37), text_2(), "primer punto");
    assert_pixel(pixel(&target, 57, 37), text_2(), "último punto");
    assert_pixel(
        pixel(&target, 65, 37),
        [0.0, 0.0, 0.0, 0.0],
        "sin quinto punto",
    );
}

#[test]
fn an_empty_secret_paints_the_placeholder_and_editing_still_hides_the_key() {
    let menu = menu();
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_secret_value(ctx, "", false, 37.0, 1.0);
    });
    let layer = &target;
    let ink = (25..120)
        .flat_map(|x| (30..45).map(move |y| pixel(layer, x, y)[3]))
        .fold(0.0_f32, f32::max);
    assert!(ink > 0.0, "«Sin clave» se pinta");
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_secret_value(ctx, "sk-secreto", true, 37.0, 1.0);
    });
    assert!(
        pixel(&target, 33, 37)[3] > 0.0,
        "los puntos siguen ahí al editar"
    );
}

#[test]
fn a_hotkey_paints_keycaps_and_a_capture_field() {
    let menu = menu();
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_hotkey_value(&menu, ctx, "hotkey", 10.0, 1.0, 1.0);
    });
    assert_pixel(
        pixel(&target, 35, 47),
        [0.138_039_2, 0.219_607_9, 0.238_431_4, 0.8],
        "tecla del atajo",
    );

    let mut capturing = menu;
    capturing.capturing = true;
    capturing.editing_key = Some("hotkey".into());
    capturing.capture_msg = Some(("Combinación no válida".into(), 5.0));
    let target = draw(&capturing, |ctx| {
        super::keycaps::draw_hotkey_value(&capturing, ctx, "hotkey", 10.0, 1.0, 1.0);
    });
    assert_pixel(
        pixel(&target, 25, 47),
        [0.086_274_5, 0.137_254_9, 0.149_019_6, 0.5],
        "campo de captura",
    );
}

#[test]
fn capture_modifiers_use_accent_keycaps_and_the_ellipsis() {
    let mut menu = menu();
    menu.capturing = true;
    menu.editing_key = Some("hotkey".into());
    menu.capture_mods = ["ctrl".to_string(), "shift".to_string()].into();
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_hotkey_value(&menu, ctx, "hotkey", 10.0, 1.0, 1.0);
    });
    assert_pixel(
        pixel(&target, 35, 47),
        [0.137_098, 0.272_078_4, 0.260_549, 0.59],
        "relleno acentuado sobre el campo",
    );
}

#[test]
fn the_copy_button_hovers_with_its_round_background() {
    let mut menu = menu();
    let secret = *SECRET_KEYS.first().expect("hay una clave secreta");
    menu.settings
        .set(
            secret,
            &crate::model::settings::SettingValue::from("sk-123456"),
        )
        .expect("clave válida");
    menu.copy_hover = Some(secret.to_string());
    let target = draw(&menu, |ctx| {
        super::keycaps::draw_copy_button(&menu, ctx, secret, 0.0, 1.0, 1.0);
    });
    assert_pixel(
        pixel(&target, 267, 37),
        [0.109_803_9, 0.176_470_6, 0.2, 1.0],
        "fondo del botón copiar",
    );
}

// --------------------------------------------------------------------------
// Deslizadores e interruptores
// --------------------------------------------------------------------------

#[test]
fn a_slider_fills_its_track_up_to_the_value() {
    let spec = SLIDERS.get("speed").expect("speed es un deslizador");
    let mut menu = menu();
    let target = {
        menu.settings
            .set(
                "speed",
                &crate::model::settings::SettingValue::from(spec.minimum),
            )
            .expect("clave válida");
        draw(&menu, |ctx| {
            super::rows::draw_slider_row(&menu, ctx, "speed", 0.0, 1.0);
        })
    };
    assert_pixel(pixel(&target, 100, 38), track(), "pista sin relleno");

    menu.settings
        .set(
            "speed",
            &crate::model::settings::SettingValue::from(spec.maximum),
        )
        .expect("clave válida");
    let target = draw(&menu, |ctx| {
        super::rows::draw_slider_row(&menu, ctx, "speed", 0.0, 1.0);
    });
    assert_pixel(pixel(&target, 100, 38), mint_btn(), "pista llena");
    assert_pixel(pixel(&target, 288, 38), text(), "pulgar en el máximo");

    menu.settings
        .set(
            "speed",
            &crate::model::settings::SettingValue::from((spec.minimum + spec.maximum) / 2.0),
        )
        .expect("clave válida");
    let target = draw(&menu, |ctx| {
        super::rows::draw_slider_row(&menu, ctx, "speed", 0.0, 1.0);
    });
    assert_pixel(pixel(&target, 100, 38), mint_btn(), "mitad izquierda");
    assert_pixel(pixel(&target, 200, 38), track(), "mitad derecha");
}

#[test]
fn the_slider_thumb_is_white_with_a_hairline_in_the_light_theme() {
    let mut menu = menu();
    menu.theme = "light";
    menu.settings
        .set(
            "speed",
            &crate::model::settings::SettingValue::from(
                SLIDERS
                    .get("speed")
                    .expect("speed es un deslizador")
                    .minimum,
            ),
        )
        .expect("clave válida");
    let target = draw(&menu, |ctx| {
        super::rows::draw_slider_row(&menu, ctx, "speed", 0.0, 1.0);
    });
    assert_pixel(pixel(&target, 31, 38), [1.0, 1.0, 1.0, 1.0], "pulgar claro");
}

#[test]
fn a_toggle_moves_its_thumb_and_changes_the_track() {
    let mut menu = menu();
    let target = draw(&menu, |ctx| {
        super::rows::draw_toggle_row(&menu, ctx, "show_advanced", 20.0, 1.0);
    });
    assert_pixel(pixel(&target, 278, 20), track(), "pista apagada");
    assert_pixel(
        pixel(&target, (TOGGLE_X0 + TOGGLE_THUMB / 2.0 + 2.0) as u32, 20),
        text_2(),
        "pulgar apagado",
    );

    menu.settings
        .set(
            "show_advanced",
            &crate::model::settings::SettingValue::Bool(true),
        )
        .expect("clave válida");
    menu.toggle_pos
        .get_mut("show_advanced")
        .expect("clave de interruptor")
        .value = 1.0;
    menu.toggle_color
        .get_mut("show_advanced")
        .expect("clave de interruptor")
        .value = 1.0;
    let target = draw(&menu, |ctx| {
        super::rows::draw_toggle_row(&menu, ctx, "show_advanced", 20.0, 1.0);
    });
    assert_pixel(pixel(&target, 260, 20), mint_btn(), "pista encendida");
    assert_pixel(
        pixel(&target, 278, 20),
        [0.023_529_4, 0.133_333_3, 0.117_647_1, 1.0],
        "pulgar encendido",
    );
    assert!(LABELS.get("show_advanced").is_some());
}

mod actions;
