use std::cell::RefCell;
use std::rc::Rc;

use crate::model::settings::{MenuSettings, SettingValue};

use super::super::geometry::{MenuCallbacks, MenuOptions};
use super::super::rects::SLIDER_TRACK_X0;
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

fn center(menu: &Menu, key: &str) -> (f64, f64) {
    let layout = menu.layout();
    let row = layout.get(key).expect("fila de prueba");
    (100.0, (row.y0 + row.y1) / 2.0)
}

#[test]
fn a_click_on_select_opens_the_selector() {
    let mut menu = menu_with(vec![("theme", "select")], MenuCallbacks::default());
    let (x, y) = center(&menu, "theme");
    assert!(menu.pointer_down(x, y, 2.0));
    assert!(menu.pointer_up(x, y, 2.0));
    assert_eq!(menu.selector_key.as_deref(), Some("theme"));
}

#[test]
fn a_toggle_flips_its_value_and_notifies() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&events);
    let callbacks = MenuCallbacks {
        live_change: Some(Box::new(move |key, value| {
            recorded.borrow_mut().push((key.to_string(), value.clone()));
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("smart_turn", "toggle")], callbacks);
    let (x, y) = center(&menu, "smart_turn");
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert_eq!(
        menu.settings.get("smart_turn"),
        Some(&SettingValue::Bool(false))
    );
    assert_eq!(
        events.borrow().as_slice(),
        &[("smart_turn".to_string(), SettingValue::Bool(false))]
    );
    assert!(menu
        .toggle_pos
        .get("smart_turn")
        .is_some_and(|t| t.active()));
}

#[test]
fn dragging_a_slider_emits_the_final_value_on_release() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&events);
    let callbacks = MenuCallbacks {
        live_change: Some(Box::new(move |key, value| {
            recorded.borrow_mut().push((key.to_string(), value.clone()));
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    let layout = menu.layout();
    let row = layout.get("speed").copied().expect("fila del slider");
    let y = row.y0 + super::super::rects::SLIDER_LINE2_CY;
    let middle = SLIDER_TRACK_X0 + (super::super::rects::SLIDER_TRACK_X1 - SLIDER_TRACK_X0) / 2.0;
    menu.pointer_down(middle, y, 2.0);
    assert!(menu.slider_drag.is_some());
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.0)));
    menu.pointer_up(middle, y, 2.1);
    assert!(menu.slider_drag.is_none());
    let events = events.borrow();
    assert!(events.len() >= 2);
    assert_eq!(events.last().expect("emisión final").0, "speed");
    assert_eq!(
        events.last().expect("emisión final").1,
        SettingValue::Float(1.0)
    );
}

#[test]
fn dragging_the_meter_updates_min_rms() {
    let mut menu = menu_with(vec![("input_meter", "meter")], MenuCallbacks::default());
    let layout = menu.layout();
    let row = layout.get("input_meter").copied().expect("medidor");
    let y = row.y0 + 38.0;
    menu.pointer_down(SLIDER_TRACK_X0, y, 2.0);
    assert!(menu.meter_drag);
    let low = menu.settings.get("min_rms").cloned().expect("umbral");
    menu.pointer_move(
        SLIDER_TRACK_X0 + (super::super::rects::SLIDER_TRACK_X1 - SLIDER_TRACK_X0),
        y,
        2.1,
    );
    let high = menu.settings.get("min_rms").cloned().expect("umbral");
    menu.pointer_up(
        SLIDER_TRACK_X0 + (super::super::rects::SLIDER_TRACK_X1 - SLIDER_TRACK_X0),
        y,
        2.2,
    );
    assert!(!menu.meter_drag);
    assert_ne!(low, high);
}

#[test]
fn clicking_a_text_row_starts_and_ends_the_edit() {
    let states = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&states);
    let callbacks = MenuCallbacks {
        text_edit: Some(Box::new(move |active| recorded.borrow_mut().push(active))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("wake_phrase", "text")], callbacks);
    let (x, y) = center(&menu, "wake_phrase");
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert_eq!(menu.editing_key.as_deref(), Some("wake_phrase"));
    assert_eq!(menu.edit_buffer, "EzAgent, escúchame");
    assert_eq!(menu.edit_cursor, menu.edit_buffer.chars().count());
    let (other_x, other_y) = (100.0, 20.0);
    menu.pointer_down(other_x, other_y, 2.1);
    assert!(menu.editing_key.is_none());
    assert_eq!(states.borrow().as_slice(), &[true, false]);
}

#[test]
fn a_failed_save_restores_the_baseline_and_stays_dirty() {
    let callbacks = MenuCallbacks {
        save: Some(Box::new(|_values| Some(false))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    menu.settings
        .set("speed", &SettingValue::Float(1.2))
        .expect("clave válida");
    let (x, y) = menu.save_rect(&menu.layout()).center();
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert!(menu.settings.dirty());
    assert!(menu.error_until > 2.0);
    assert!(menu.check_until == 0.0);
}

#[test]
fn the_test_button_asks_the_app_to_start_the_voice_test() {
    let tested = Rc::new(RefCell::new(0));
    let counter = Rc::clone(&tested);
    let callbacks = MenuCallbacks {
        test: Some(Box::new(move |_values| *counter.borrow_mut() += 1)),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("test_voice", "action")], callbacks);
    let button = menu.test_rect(&menu.layout(), 0.0);
    let (x, y) = button.center();
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert_eq!(*tested.borrow(), 1);
    assert!(menu.testing_t.active());
}

#[test]
fn a_secret_row_copies_and_marks_the_check() {
    let copied = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&copied);
    let callbacks = MenuCallbacks {
        copy: Some(Box::new(move |text| {
            recorded.borrow_mut().push(text.to_string());
            true
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("api_key", "text")], callbacks);
    menu.settings
        .set("api_key", &SettingValue::Text("sk-123".into()))
        .expect("clave válida");
    let layout = menu.layout();
    let row = layout.get("api_key").copied().expect("fila secreta");
    let button = menu.copy_rect(row.y0);
    let (x, y) = button.center();
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert_eq!(copied.borrow().as_slice(), &["sk-123".to_string()]);
    assert!(menu.copied.get("api_key").is_some_and(|until| *until > 2.0));
}

#[test]
fn a_palette_click_changes_the_theme() {
    let mut menu = menu_with(vec![("theme_preview", "palette")], MenuCallbacks::default());
    let layout = menu.layout();
    let row = layout.get("theme_preview").copied().expect("paleta");
    menu.pointer_down(40.0, row.y0 + 15.0, 2.0);
    menu.pointer_up(40.0, row.y0 + 15.0, 2.0);
    assert_eq!(
        menu.settings.get("theme"),
        Some(&SettingValue::Text("light".into()))
    );
}

#[test]
fn pressing_save_calls_the_callback_and_cleans_the_dirty_flag() {
    let calls = Rc::new(RefCell::new(0));
    let counter = Rc::clone(&calls);
    let callbacks = MenuCallbacks {
        save: Some(Box::new(move |_values| {
            *counter.borrow_mut() += 1;
            Some(true)
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("speed", "slider")], callbacks);
    menu.settings
        .set("speed", &SettingValue::Float(1.2))
        .expect("clave válida");
    let (x, y) = menu.save_rect(&menu.layout()).center();
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert_eq!(*calls.borrow(), 1);
    assert!(!menu.settings.dirty());
    assert!(menu.check_until > 2.0);
}

#[test]
fn destructive_actions_need_a_second_click() {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&actions);
    let callbacks = MenuCallbacks {
        action: Some(Box::new(move |key| {
            recorded.borrow_mut().push(key.to_string());
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("new_conversation", "action")], callbacks);
    let layout = menu.layout();
    let button = menu.action_rect(&layout, "new_conversation", 0.0);
    let (x, y) = button.center();
    menu.pointer_down(x, y, 2.0);
    menu.pointer_up(x, y, 2.0);
    assert!(actions.borrow().is_empty());
    assert!(menu.is_armed("new_conversation", 2.1));
    menu.pointer_down(x, y, 3.1);
    menu.pointer_up(x, y, 3.1);
    assert_eq!(
        actions.borrow().as_slice(),
        &["new_conversation".to_string()]
    );
    assert!(!menu.is_armed("new_conversation", 3.2));
}

#[test]
fn hiding_from_the_header_closes_and_notifies() {
    let hidden = Rc::new(RefCell::new(0));
    let counter = Rc::clone(&hidden);
    let callbacks = MenuCallbacks {
        hide: Some(Box::new(move || *counter.borrow_mut() += 1)),
        ..MenuCallbacks::default()
    };
    let mut menu = menu_with(vec![("repeat_last", "action")], callbacks);
    assert!(menu.pointer_down(289.0, 62.0, 2.0));
    assert!(menu.pointer_up(289.0, 62.0, 2.0));
    assert!(!menu.is_open());
    assert_eq!(*hidden.borrow(), 1);
}

#[test]
fn hover_tracks_rows_and_returns_when_leaving() {
    let mut menu = menu_with(vec![("smart_turn", "toggle")], MenuCallbacks::default());
    let (x, y) = center(&menu, "smart_turn");
    menu.pointer_move(x, y, 2.0);
    assert_eq!(menu.hover_id.as_deref(), Some("row:smart_turn"));
    let tween = menu.row_hover.get("smart_turn").expect("hover de fila");
    assert!(tween.active());
    menu.pointer_move(5.0, 5.0, 2.1);
    assert!(menu.hover_id.is_none());
}

#[test]
fn pointer_outside_a_closed_menu_is_ignored() {
    let mut menu = menu_with(vec![("smart_turn", "toggle")], MenuCallbacks::default());
    menu.close(3.0);
    menu.update(5.0);
    let (x, y) = center(&menu, "smart_turn");
    assert!(!menu.pointer_down(x, y, 5.0));
    assert!(!menu.pointer_move(x, y, 5.0));
    assert!(!menu.pointer_up(x, y, 5.0));
}
