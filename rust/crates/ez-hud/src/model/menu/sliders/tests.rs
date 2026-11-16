use parking_lot::Mutex;
use std::sync::Arc;

use crate::model::settings::{
    db_to_rms, MenuSettings, SettingValue, METER_DB_CEIL, METER_DB_FLOOR, SLIDERS,
};

use super::super::geometry::{MenuCallbacks, MenuOptions};
use super::super::rects::{SLIDER_TRACK_X0, SLIDER_TRACK_X1};
use super::*;

fn menu_with(rows: Vec<(&str, &str)>, callbacks: MenuCallbacks) -> Menu {
    let mut menu = Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks)
        .expect("opciones válidas");
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            rows.into_iter()
                .map(|(key, kind)| (key.to_string(), kind.to_string()))
                .collect(),
        )],
        Some("Prueba"),
        Some(0.0),
    );
    menu.open(0.0);
    menu.height(2.0);
    menu
}

type Events = Arc<Mutex<Vec<(String, SettingValue)>>>;

fn recorder() -> (Events, MenuCallbacks) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    let callbacks = MenuCallbacks {
        live_change: Some(Box::new(move |key, value| {
            recorded.lock().push((key.to_string(), value.clone()));
        })),
        ..MenuCallbacks::default()
    };
    (events, callbacks)
}

#[test]
fn the_pointer_maps_to_the_slider_range() {
    let (events, callbacks) = recorder();
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    let middle = SLIDER_TRACK_X0 + (SLIDER_TRACK_X1 - SLIDER_TRACK_X0) / 2.0;
    menu.set_slider_from_x("speed", SLIDER_TRACK_X0, 2.0);
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(0.75)));
    menu.set_slider_from_x("speed", middle, 2.1);
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.0)));
    menu.set_slider_from_x("speed", SLIDER_TRACK_X1 + 500.0, 2.2);
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.25)));
    assert_eq!(
        events.lock().as_slice(),
        &[
            ("speed".to_string(), SettingValue::Float(0.75)),
            ("speed".to_string(), SettingValue::Float(1.0)),
            ("speed".to_string(), SettingValue::Float(1.25)),
        ]
    );
    assert!(menu.slider_changed);
}

#[test]
fn emissions_are_throttled_to_fifty_milliseconds_unless_forced() {
    let (events, callbacks) = recorder();
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    menu.slider_last_emit = 100.0;
    menu.set_slider_from_x("speed", SLIDER_TRACK_X1, 100.01);
    assert_eq!(events.lock().len(), 0);
    assert!(menu.slider_changed);
    assert!(menu.slider_pending.is_some());
    menu.emit_slider(100.06, false);
    assert_eq!(events.lock().len(), 1);
    menu.set_slider_from_x("speed", SLIDER_TRACK_X0, 100.07);
    menu.emit_slider(100.07, true);
    assert_eq!(events.lock().len(), 2);
    assert!(menu.slider_pending.is_none());
}

#[test]
fn an_unchanged_value_does_not_emit() {
    let (events, callbacks) = recorder();
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    menu.settings
        .set("speed", &SettingValue::Float(1.0))
        .expect("clave válida");
    let middle = SLIDER_TRACK_X0 + (SLIDER_TRACK_X1 - SLIDER_TRACK_X0) / 2.0;
    menu.set_slider_from_x("speed", middle, 2.0);
    assert_eq!(events.lock().len(), 0);
    assert!(!menu.slider_changed);
    assert!(menu.slider_pending.is_none());
}

#[test]
fn the_extra_width_moves_the_end_of_the_track() {
    let mut menu = menu_with(vec![("speed", "slider")], MenuCallbacks::default());
    menu.set_width(Some(420.0));
    menu.set_slider_from_x("speed", SLIDER_TRACK_X1 + menu.dx, 2.0);
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.25)));
}

#[test]
fn the_meter_converts_the_position_to_decibels() {
    let (events, callbacks) = recorder();
    let mut menu = menu_with(vec![("input_meter", "meter")], callbacks);
    menu.meter_last_emit = 2.0;
    menu.set_meter_from_x(SLIDER_TRACK_X0, 2.01);
    assert_eq!(
        menu.settings.get("min_rms"),
        Some(&SettingValue::Float(db_to_rms(METER_DB_FLOOR)))
    );
    assert_eq!(events.lock().len(), 0);
    assert!(menu.meter_changed && menu.meter_pending.is_some());
    menu.emit_meter(2.06);
    assert_eq!(events.lock().len(), 1);
    menu.set_meter_from_x(SLIDER_TRACK_X1, 2.07);
    assert_eq!(
        menu.settings.get("min_rms"),
        Some(&SettingValue::Float(db_to_rms(-20.0)))
    );
    assert_eq!(events.lock().len(), 1);
    menu.emit_meter(3.0);
    assert_eq!(events.lock().len(), 2);
}

#[test]
fn an_unknown_slider_key_is_ignored() {
    let mut menu = menu_with(vec![("speed", "slider")], MenuCallbacks::default());
    let before = menu.settings.get("speed").cloned();
    menu.set_slider_from_x("no_existe", 200.0, 2.0);
    assert_eq!(menu.settings.get("speed").cloned(), before);
    assert_eq!(SLIDERS.len(), 16);
}

fn decibels(rms: f64) -> f64 {
    20.0 * rms.log10()
}

fn drag_to(menu: &mut Menu, fraction: f64) -> f64 {
    let x = SLIDER_TRACK_X0 + fraction * (SLIDER_TRACK_X1 - SLIDER_TRACK_X0);
    menu.set_meter_from_x(x, 1.0);
    match menu.settings.get("min_rms") {
        Some(SettingValue::Float(value)) => *value,
        other => panic!("min_rms no es un número: {other:?}"),
    }
}

#[test]
fn the_meter_track_is_the_whole_decibel_scale() {
    let spec = SLIDERS.get("min_rms").expect("min_rms");
    assert!((decibels(spec.minimum) - METER_DB_FLOOR).abs() < 1e-9);
    assert!((decibels(spec.maximum) - METER_DB_CEIL).abs() < 1e-9);

    let mut menu = menu_with(vec![("input_meter", "meter")], MenuCallbacks::default());
    assert!((decibels(drag_to(&mut menu, 0.0)) - METER_DB_FLOOR).abs() <= 0.05);
    assert!((decibels(drag_to(&mut menu, 1.0)) - METER_DB_CEIL).abs() <= 0.05);
    for fraction in [0.05, 0.25, 0.5, 0.75, 0.95] {
        let rms = drag_to(&mut menu, fraction);
        let drawn = crate::paint::menu::controls::meter_fraction(rms);
        let span = METER_DB_CEIL - METER_DB_FLOOR;
        assert!(
            (drawn - fraction).abs() <= 0.5 / span,
            "{fraction} -> {drawn}"
        );
    }
    let low = decibels(drag_to(&mut menu, 0.10));
    let high = decibels(drag_to(&mut menu, 0.90));
    assert!((high - low - 0.8 * (METER_DB_CEIL - METER_DB_FLOOR)).abs() <= 1.0);
}

#[test]
fn the_default_threshold_sits_inside_the_range_and_survives_quantizing() {
    use crate::model::settings::DEFAULT_VALUES;
    let settings = MenuSettings::default();
    let value = match settings.get("min_rms") {
        Some(SettingValue::Float(value)) => *value,
        other => panic!("min_rms no es un número: {other:?}"),
    };
    let default = match DEFAULT_VALUES.get("min_rms") {
        Some(SettingValue::Float(value)) => *value,
        other => panic!("min_rms por defecto no es un número: {other:?}"),
    };
    assert!((value - default).abs() / default < 0.06);
    assert!(db_to_rms(METER_DB_FLOOR) < value && value < db_to_rms(METER_DB_CEIL));
}
