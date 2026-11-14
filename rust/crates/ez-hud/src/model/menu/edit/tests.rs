use std::cell::RefCell;
use std::rc::Rc;

use crate::model::settings::{MenuSettings, SettingValue};

use super::super::geometry::{MenuCallbacks, MenuOptions};
use super::*;

fn menu(callbacks: MenuCallbacks) -> Menu {
    let mut menu = Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks)
        .expect("opciones válidas");
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            [
                ("wake_phrase", "text"),
                ("stt_dictionary", "text"),
                ("hotkey", "text"),
            ]
            .into_iter()
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

fn with_mods(names: &'static [&'static str], callbacks: MenuCallbacks) -> Menu {
    menu(MenuCallbacks {
        modifier_source: Some(Box::new(move || {
            names.iter().map(|name| (*name).to_string()).collect()
        })),
        ..callbacks
    })
}

fn with_ctrl(callbacks: MenuCallbacks) -> Menu {
    with_mods(&["ctrl"], callbacks)
}

#[test]
fn beginning_an_edit_loads_the_stored_text() {
    let states = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&states);
    let callbacks = MenuCallbacks {
        text_edit: Some(Box::new(move |active| recorded.borrow_mut().push(active))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    menu.settings
        .set("wake_phrase", &SettingValue::Text("Hola".into()))
        .expect("clave válida");
    menu.begin_text_edit("wake_phrase", Some(1.0));
    assert_eq!(menu.editing_key(), Some("wake_phrase"));
    assert_eq!(menu.text_buffer(), "Hola");
    assert_eq!(menu.edit_cursor, 4);
    assert_eq!(menu.edit_t0, 1.0);
    assert_eq!(states.borrow().as_slice(), &[true]);
}

#[test]
fn typing_and_editing_keys_move_the_cursor_by_characters() {
    let mut menu = menu(MenuCallbacks::default());
    menu.settings
        .set("wake_phrase", &SettingValue::Text("año".into()))
        .expect("clave válida");
    menu.begin_text_edit("wake_phrase", Some(1.0));
    assert_eq!(menu.edit_cursor, 3);
    menu.text_key(36, false); // Home
    assert_eq!(menu.edit_cursor, 0);
    menu.text_key(39, false); // Right
    menu.text_input("é");
    assert_eq!(menu.text_buffer(), "aéño");
    assert_eq!(menu.edit_cursor, 2);
    menu.text_key(8, false); // Backspace
    assert_eq!(menu.text_buffer(), "año");
    assert_eq!(menu.edit_cursor, 1);
    menu.text_key(46, false); // Delete
    assert_eq!(menu.text_buffer(), "ao");
    menu.text_key(35, false); // End
    menu.text_input("!");
    assert_eq!(menu.text_buffer(), "ao!");
    menu.text_key(46, false); // Delete at the end does nothing
    assert_eq!(menu.text_buffer(), "ao!");
}

#[test]
fn ctrl_a_then_typing_replaces_everything() {
    let mut menu = menu(MenuCallbacks::default());
    menu.settings
        .set("wake_phrase", &SettingValue::Text("Hola".into()))
        .expect("clave válida");
    menu.begin_text_edit("wake_phrase", Some(1.0));
    assert!(menu.text_key(65, true));
    assert!(menu.edit_select_all);
    menu.text_input("Ey");
    assert_eq!(menu.text_buffer(), "Ey");
    assert_eq!(menu.edit_cursor, 2);
    assert!(!menu.edit_select_all);
    assert!(menu.text_key(65, true));
    menu.text_key(8, false);
    assert_eq!(menu.text_buffer(), "");
}

#[test]
fn enter_commits_and_escape_discards() {
    let states = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&states);
    let callbacks = MenuCallbacks {
        text_edit: Some(Box::new(move |active| recorded.borrow_mut().push(active))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    menu.settings
        .set("wake_phrase", &SettingValue::Text("Hola".into()))
        .expect("clave válida");
    menu.begin_text_edit("wake_phrase", Some(1.0));
    menu.text_input(" mundo");
    menu.text_key(13, false);
    assert!(menu.editing_key().is_none());
    assert_eq!(
        menu.settings.get("wake_phrase"),
        Some(&SettingValue::Text("Hola mundo".into()))
    );
    menu.begin_text_edit("wake_phrase", Some(2.0));
    menu.text_input(" y adiós");
    menu.text_key(27, false);
    assert!(menu.editing_key().is_none());
    assert_eq!(
        menu.settings.get("wake_phrase"),
        Some(&SettingValue::Text("Hola mundo".into()))
    );
    assert_eq!(states.borrow().as_slice(), &[true, false, true, false]);
}

#[test]
fn paste_flattens_lines_except_the_dictionary() {
    let mut menu = menu(MenuCallbacks::default());
    menu.begin_text_edit("wake_phrase", Some(1.0));
    menu.edit_buffer.clear();
    menu.edit_cursor = 0;
    menu.paste_text("uno\r\ndos");
    assert_eq!(menu.text_buffer(), "uno  dos");
    menu.finish_text_edit(false);
    menu.begin_text_edit("stt_dictionary", Some(2.0));
    menu.paste_text("a\nb\nc");
    assert_eq!(menu.text_buffer(), "a; b; c");
    menu.finish_text_edit(false);
}

#[test]
fn a_hotkey_row_captures_the_combination() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let texts = Rc::new(RefCell::new(Vec::new()));
    let captures = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&events);
    let recorded_text = Rc::clone(&texts);
    let recorded_capture = Rc::clone(&captures);
    let callbacks = MenuCallbacks {
        live_change: Some(Box::new(move |key, value| {
            recorded.borrow_mut().push((key.to_string(), value.clone()));
        })),
        text_edit: Some(Box::new(move |active| {
            recorded_text.borrow_mut().push(active);
        })),
        hotkey_capture: Some(Box::new(move |active| {
            recorded_capture.borrow_mut().push(active);
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = with_ctrl(callbacks);
    menu.begin_text_edit("hotkey", Some(1.0));
    assert!(menu.capturing);
    assert_eq!(menu.editing_key(), Some("hotkey"));
    assert!(menu.text_key(69, false));
    assert!(!menu.capturing);
    assert!(menu.editing_key().is_none());
    assert_eq!(
        menu.settings.get("hotkey"),
        Some(&SettingValue::Text("ctrl+e".into()))
    );
    assert_eq!(
        events.borrow().as_slice(),
        &[("hotkey".to_string(), SettingValue::Text("ctrl+e".into()))]
    );
    assert_eq!(texts.borrow().as_slice(), &[true, false]);
    assert_eq!(captures.borrow().as_slice(), &[true, false]);
}

#[test]
fn a_conflicting_hotkey_is_rejected_with_the_other_label() {
    let mut menu = with_mods(&["ctrl", "alt"], MenuCallbacks::default());
    menu.now = 2.0;
    let before = menu.settings.get("hotkey").cloned();
    menu.begin_text_edit("hotkey", Some(2.0));
    assert!(menu.text_key(69, false));
    assert!(menu.capturing);
    assert_eq!(
        menu.capture_msg,
        Some((
            "Ya la usa «Atajo de colgar»".to_string(),
            2.0 + CAPTURE_MSG_SECS
        ))
    );
    assert_eq!(menu.settings.get("hotkey").cloned(), before);
}

#[test]
fn a_rejected_key_shows_the_reason() {
    let mut menu = menu(MenuCallbacks::default());
    menu.now = 3.0;
    menu.begin_text_edit("hotkey", Some(3.0));
    assert!(menu.text_key(69, false));
    assert_eq!(
        menu.capture_msg,
        Some((
            "Combínala con Ctrl, Alt o Win".to_string(),
            3.0 + CAPTURE_MSG_SECS
        ))
    );
}

#[test]
fn escape_cancels_the_capture() {
    let mut menu = menu(MenuCallbacks::default());
    menu.begin_text_edit("hotkey", Some(1.0));
    assert!(menu.capturing);
    assert!(menu.text_key(27, false));
    assert!(!menu.capturing);
    assert!(menu.editing_key().is_none());
    assert!(menu.capture_msg.is_none());
}

#[test]
fn editing_keys_are_ignored_without_an_active_row() {
    let mut menu = menu(MenuCallbacks::default());
    assert!(!menu.text_key(37, false));
    assert!(!menu.text_input("a"));
    assert!(!menu.paste_text("a"));
    assert!(menu.editing_key().is_none());
}
