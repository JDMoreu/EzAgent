use parking_lot::Mutex;
use std::sync::Arc;

use crate::model::settings::{MenuSettings, SettingValue, SettingsValues};

use super::super::geometry::{MenuCallbacks, MenuOptions};
use super::*;

fn menu(callbacks: MenuCallbacks) -> Menu {
    let mut menu = Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks)
        .expect("opciones válidas");
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            [("wake_phrase", "text"), ("speed", "slider")]
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

fn speed(menu: &mut Menu, value: f64) {
    menu.settings
        .set("speed", &SettingValue::Float(value))
        .expect("clave válida");
}

#[test]
fn start_test_commits_the_edit_and_reports_the_snapshot() {
    let snapshots = Arc::new(Mutex::new(Vec::<SettingsValues>::new()));
    let recorded = Arc::clone(&snapshots);
    let callbacks = MenuCallbacks {
        test: Some(Box::new(move |values| recorded.lock().push(values.clone()))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    menu.begin_text_edit("wake_phrase", Some(1.0));
    menu.text_input("!");
    menu.start_test(2.0);
    assert!(menu.editing_key.is_none());
    assert_eq!(
        menu.settings.get("wake_phrase"),
        Some(&SettingValue::Text("EzAgent, escúchame!".into()))
    );
    assert!(menu.testing_t.active());
    let snapshots = snapshots.lock();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].get("wake_phrase"),
        Some(&SettingValue::Text("EzAgent, escúchame!".into()))
    );
}

#[test]
fn a_successful_save_marks_the_settings_clean() {
    let snapshots = Arc::new(Mutex::new(Vec::<SettingsValues>::new()));
    let recorded = Arc::clone(&snapshots);
    let callbacks = MenuCallbacks {
        save: Some(Box::new(move |values| {
            recorded.lock().push(values.clone());
            Some(true)
        })),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    speed(&mut menu, 1.1);
    assert!(menu.settings.dirty());
    menu.save(2.0);
    assert!(!menu.settings.dirty());
    assert!(!menu.saving_pending);
    assert_eq!(menu.check_until, 2.0 + SAVE_CHECK_MS / 1000.0);
    assert_eq!(menu.saved_flash_until, 2.0 + SAVE_FLASH_MS / 1000.0);
    assert_eq!(menu.error_until, 0.0);
    assert!(menu.save_backup.is_some());
    assert_eq!(
        snapshots.lock()[0].get("speed"),
        Some(&SettingValue::Float(1.1))
    );
}

#[test]
fn a_failed_save_restores_the_previous_copy_and_shows_the_error() {
    let callbacks = MenuCallbacks {
        save: Some(Box::new(|_values| Some(false))),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    speed(&mut menu, 1.1);
    menu.save(2.0);
    // El valor editado sigue a la vista; solo se restaura la copia guardada,
    // así que el menú vuelve a estar dirty (spec 10.2).
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.1)));
    assert!(menu.settings.dirty());
    assert_eq!(menu.error_until, 2.0 + SAVE_ERROR_MS / 1000.0);
    assert_eq!(menu.check_until, 0.0);
    assert_eq!(menu.saved_flash_until, 0.0);
    assert!(menu.save_backup.is_none());
    assert!(menu.save_snapshot.is_none());
}

#[test]
fn a_pending_save_waits_for_on_applied() {
    let callbacks = MenuCallbacks {
        save: Some(Box::new(|_values| None)),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    speed(&mut menu, 1.1);
    menu.save(2.0);
    assert!(menu.saving_pending);
    assert_eq!(menu.check_until, 0.0);
    assert_eq!(menu.saved_flash_until, 0.0);
    menu.now = 3.0;
    menu.on_applied(true);
    assert!(!menu.saving_pending);
    assert!(!menu.settings.dirty());
    assert_eq!(menu.check_until, 3.0 + SAVE_CHECK_MS / 1000.0);
    assert_eq!(menu.saved_flash_until, 3.0 + SAVE_FLASH_MS / 1000.0);
    assert!(menu.save_backup.is_none());
    assert!(menu.save_snapshot.is_none());
}

#[test]
fn a_failed_async_save_keeps_the_edited_values_and_dirty() {
    let callbacks = MenuCallbacks {
        save: Some(Box::new(|_values| None)),
        ..MenuCallbacks::default()
    };
    let mut menu = menu(callbacks);
    speed(&mut menu, 1.1);
    menu.save(2.0);
    menu.now = 3.0;
    menu.on_applied(false);
    assert_eq!(menu.settings.get("speed"), Some(&SettingValue::Float(1.1)));
    assert!(menu.settings.dirty());
    assert_eq!(menu.error_until, 3.0 + SAVE_ERROR_MS / 1000.0);
    assert_eq!(menu.check_until, 0.0);
    assert!(menu.save_backup.is_none());
    assert!(menu.save_snapshot.is_none());
}

#[test]
fn save_without_a_callback_succeeds() {
    let mut menu = menu(MenuCallbacks::default());
    speed(&mut menu, 1.1);
    menu.save(2.0);
    assert!(!menu.settings.dirty());
    assert_eq!(menu.check_until, 2.0 + SAVE_CHECK_MS / 1000.0);
}

#[test]
fn save_does_nothing_when_clean_or_while_pending() {
    let mut clean = menu(MenuCallbacks::default());
    clean.save(2.0);
    assert_eq!(clean.check_until, 0.0);
    assert!(clean.save_backup.is_none());
    let mut pending = menu(MenuCallbacks::default());
    speed(&mut pending, 1.1);
    pending.saving_pending = true;
    pending.save(3.0);
    assert!(pending.settings.dirty());
    assert_eq!(pending.check_until, 0.0);
    assert!(pending.save_backup.is_none());
}

#[test]
fn an_invalid_text_edit_blocks_the_save() {
    let mut menu = menu(MenuCallbacks::default());
    menu.begin_text_edit("llm_profile_name", Some(1.0));
    menu.edit_buffer = "x".repeat(81);
    menu.save(2.0);
    assert!(menu.editing_key.is_some());
    assert!(menu.edit_error.is_some());
    assert!(menu.save_backup.is_none());
}
