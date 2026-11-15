use parking_lot::Mutex;
use std::sync::Arc;

use crate::model::settings::{MenuSettings, OptionSource, SettingValue, SettingsValues};

use super::super::geometry::{MenuCallbacks, MenuOptions};
use super::super::pointer::SLIDER_THUMB_HOVER;
use super::super::rects::SLIDER_LINE2_CY;
use super::*;

fn plain_menu(rows: Vec<(&str, &str)>) -> Menu {
    let mut menu = Menu::new(
        MenuSettings::default(),
        MenuOptions::default(),
        MenuCallbacks::default(),
    )
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

fn menu_with_models(count: usize) -> Menu {
    let pairs: Vec<(String, String)> = (0..count)
        .map(|index| (format!("m{index}"), format!("Modelo {index}")))
        .collect();
    let mut values = SettingsValues::new();
    values.insert("stt_model", SettingValue::Text("m0".into()));
    let source = OptionSource::Pairs(pairs);
    let settings = MenuSettings::new(&values, &[], None, &[], Some(&source), None);
    let mut menu = Menu::new(settings, MenuOptions::default(), MenuCallbacks::default())
        .expect("opciones válidas");
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            vec![("stt_model".to_string(), "select".to_string())],
        )],
        Some("Prueba"),
        Some(0.0),
    );
    menu.open(0.0);
    menu.height(2.0);
    menu
}

#[test]
fn a_long_list_scrolls_before_the_menu_does() {
    let mut menu = menu_with_models(7);
    menu.toggle_selector("stt_model", 2.0);
    menu.update(2.5);
    let layout = menu.layout();
    let geometry = menu
        .selector_geometry(&layout, menu.scroll_offset(), None)
        .expect("selector abierto");
    assert_eq!(geometry.options.len(), 7);
    assert!((geometry.visible - 160.0).abs() < 1e-9);
    let (x, y) = (100.0, geometry.top + 10.0);
    assert!(menu.wheel(x, y, -120, 3.0));
    assert_eq!(menu.list_scroll_target, 32.0);
    assert_eq!(menu.list_scroll, 32.0);
    assert!(menu.wheel(x, y, -120, 3.1));
    assert_eq!(menu.list_scroll_target, 64.0);
    assert!(menu.wheel(x, y, -120, 3.2));
    assert_eq!(menu.list_scroll_target, 64.0);
    assert!(menu.wheel(x, y, 120, 3.3));
    assert_eq!(menu.list_scroll_target, 32.0);
}

#[test]
fn a_menu_that_does_not_fit_scrolls_by_forty() {
    let mut menu = plain_menu(vec![
        ("wake_phrase", "text"),
        ("stt_prompt", "text"),
        ("whisper_url", "text"),
        ("llm_model", "text"),
        ("llm_base_url", "text"),
        ("llm_provider", "text"),
        ("llm_profile_name", "text"),
        ("api_key", "text"),
    ]);
    menu.set_max_height(Some(300.0));
    menu.open(2.0);
    menu.height(3.0);
    assert!(menu.scroll_max() > 0.5);
    assert!(menu.wheel(100.0, 100.0, -120, 4.0));
    assert_eq!(menu.scroll_target, 40.0);
    assert!(menu.scroll_tween.active());
    assert!(menu.wheel(100.0, 100.0, 120, 4.1));
    assert_eq!(menu.scroll_target, 0.0);
}

#[test]
fn the_wheel_over_a_slider_moves_one_step_and_hovers_its_thumb() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    let callbacks = MenuCallbacks {
        live_change: Some(Box::new(move |key, value| {
            recorded.lock().push((key.to_string(), value.clone()));
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks)
        .expect("opciones válidas");
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            vec![("speed".to_string(), "slider".to_string())],
        )],
        Some("Prueba"),
        Some(0.0),
    );
    menu.open(0.0);
    menu.height(2.0);
    let layout = menu.layout();
    let row = layout.get("speed").copied().expect("fila del slider");
    let y = row.y0 + SLIDER_LINE2_CY;
    assert!(menu.wheel(100.0, y, -120, 2.0));
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(0.90)));
    assert_eq!(
        menu.thumb_target.get("speed").copied(),
        Some(SLIDER_THUMB_HOVER)
    );
    assert!(menu.thumb.get("speed").is_some_and(|tween| tween.active()));
    assert!(menu.wheel(100.0, y, 120, 2.1));
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(0.95)));
    assert_eq!(events.lock().len(), 2);
}

#[test]
fn a_partial_notch_counts_as_one() {
    let mut menu = plain_menu(vec![("speed", "slider")]);
    let layout = menu.layout();
    let row = layout.get("speed").copied().expect("fila del slider");
    let y = row.y0 + SLIDER_LINE2_CY;
    assert!(menu.wheel(100.0, y, -1, 2.0));
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(0.90)));
}

#[test]
fn the_wheel_is_ignored_when_closed_or_without_a_delta() {
    let mut closed = Menu::new(
        MenuSettings::default(),
        MenuOptions::default(),
        MenuCallbacks::default(),
    )
    .expect("opciones válidas");
    assert!(!closed.wheel(100.0, 100.0, -120, 1.0));
    let mut menu = plain_menu(vec![("speed", "slider")]);
    assert!(!menu.wheel(100.0, 100.0, 0, 2.0));
}
