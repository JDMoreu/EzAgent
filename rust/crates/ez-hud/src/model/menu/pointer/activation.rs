//! Efectos de activación y ayudas de arrastre que `pointer` necesita y que
//! `menu.py` reparte más abajo (1888-2338): slider, medidor, guardado, prueba
//! de voz y edición de texto.
//!
//! // DECISION: F5-09 (sliders, wheel, edit, voice_test) es dueña de esas
//! // funciones en el Python. `pointer_up` las necesita ya, así que viven aquí
//! // como funciones privadas de `pointer`; F5-09 puede moverlas a su módulo y
//! // borrar esta copia sin tocar el resto del menú.

use ez_core::pyvalue::char_len;

use crate::model::settings::{
    db_to_rms, SettingValue, METER_DB_CEIL, METER_DB_FLOOR, SLIDERS, TEXT_KEYS,
};

use super::super::geometry::Menu;
use super::super::rects::{SLIDER_TRACK_X0, SLIDER_TRACK_X1};

/// Cadencia mínima de emisión mientras se arrastra un slider.
pub(super) const SLIDER_EMIT_MS: f64 = 50.0;
/// Confirmación visible tras guardar.
pub(super) const SAVE_CHECK_MS: f64 = 600.0;
/// Duración del destello del pie tras guardar.
pub(super) const SAVE_FLASH_MS: f64 = 800.0;
/// Duración del mensaje de error de guardado.
pub(super) const SAVE_ERROR_MS: f64 = 2000.0;

/// Filas que capturan una combinación de teclas en vez de texto libre.
pub(super) const HOTKEY_KEYS: [&str; 6] = [
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
    "chat_hotkey",
];

pub(super) fn start_test(menu: &mut Menu, now: f64) {
    menu.apply_text_edit(true);
    menu.set_testing(true, Some(now));
    menu.notify_test(&menu.settings.snapshot());
}

pub(super) fn save(menu: &mut Menu, now: f64) {
    menu.apply_text_edit(true);
    if menu.editing_key.is_some() {
        return;
    }
    if menu.saving_pending || !menu.settings.dirty() {
        return;
    }
    menu.save_backup = Some(menu.settings.saved_snapshot());
    let snapshot = menu.settings.snapshot();
    menu.save_snapshot = Some(snapshot.clone());
    // Un `on_save` ausente equivale al `True` del Python; un `None` real
    // significa «aceptado y pendiente» y espera a `on_applied`.
    let result = match &menu.callbacks.save {
        None => Some(true),
        Some(callback) => callback(&snapshot),
    };
    match result {
        Some(false) => save_failed(menu, now),
        Some(true) => save_succeeded(menu, now),
        None => {
            menu.saving_pending = true;
            menu.check_until = 0.0;
            menu.saved_flash_until = 0.0;
            menu.error_until = 0.0;
        }
    }
}

fn save_succeeded(menu: &mut Menu, now: f64) {
    menu.saving_pending = false;
    menu.settings.mark_saved(None);
    menu.error_until = 0.0;
    menu.check_until = now + SAVE_CHECK_MS / 1000.0;
    menu.saved_flash_until = now + SAVE_FLASH_MS / 1000.0;
}

fn save_failed(menu: &mut Menu, now: f64) {
    menu.saving_pending = false;
    if let Some(backup) = menu.save_backup.take() {
        menu.settings.restore_saved(&backup);
    }
    menu.save_snapshot = None;
    menu.check_until = 0.0;
    menu.saved_flash_until = 0.0;
    menu.error_until = now + SAVE_ERROR_MS / 1000.0;
}

pub(super) fn begin_text_edit(menu: &mut Menu, key: &str, now: Option<f64>) {
    if !TEXT_KEYS.contains(&key) {
        return;
    }
    if menu.editing_key.as_deref() == Some(key) {
        return;
    }
    if menu.editing_key.is_some() {
        menu.apply_text_edit(true);
        if menu.editing_key.is_some() {
            return;
        }
    }
    if HOTKEY_KEYS.contains(&key) {
        menu.editing_key = Some(key.to_string());
        menu.capturing = true;
        menu.capture_mods.clear();
        menu.capture_msg = None;
        menu.edit_buffer.clear();
        menu.edit_cursor = 0;
        menu.edit_select_all = false;
        menu.edit_t0 = now.unwrap_or(menu.now);
        menu.notify_text_edit(true);
        menu.notify_hotkey_capture(true);
        return;
    }
    let text = menu.secret_text(key);
    menu.editing_key = Some(key.to_string());
    menu.edit_error = None;
    menu.edit_buffer = text;
    menu.edit_cursor = char_len(&menu.edit_buffer);
    menu.edit_select_all = false;
    menu.edit_t0 = now.unwrap_or(menu.now);
    menu.notify_text_edit(true);
}

pub(super) fn set_slider_from_x(menu: &mut Menu, key: &str, x: f64, now: f64) {
    // DECISION: una clave sin especificación no mueve nada en vez de lanzar el
    // `KeyError` del Python; solo se llega aquí desde un hit de slider.
    let Some(spec) = SLIDERS.get(key) else {
        return;
    };
    let width = (SLIDER_TRACK_X1 + menu.dx) - SLIDER_TRACK_X0;
    let fraction = if width <= 0.0 {
        0.0
    } else {
        (x - SLIDER_TRACK_X0) / width
    };
    let fraction = fraction.clamp(0.0, 1.0);
    let value = spec.minimum + fraction * (spec.maximum - spec.minimum);
    let old = menu.settings.get(key).cloned();
    let Ok(new) = menu.settings.set(key, &SettingValue::Float(value)) else {
        return;
    };
    if old.as_ref() == Some(&new) {
        return;
    }
    menu.slider_changed = true;
    menu.slider_pending = Some((key.to_string(), new));
    emit_slider(menu, now, false);
}

pub(super) fn set_meter_from_x(menu: &mut Menu, x: f64, now: f64) {
    let width = (SLIDER_TRACK_X1 + menu.dx) - SLIDER_TRACK_X0;
    let fraction = if width <= 0.0 {
        0.0
    } else {
        (x - SLIDER_TRACK_X0) / width
    };
    let fraction = fraction.clamp(0.0, 1.0);
    // La posición se convierte en dB, la misma escala del medidor.
    let value = db_to_rms(METER_DB_FLOOR + fraction * (METER_DB_CEIL - METER_DB_FLOOR));
    let old = menu.settings.get("min_rms").cloned();
    let Ok(new) = menu.settings.set("min_rms", &SettingValue::Float(value)) else {
        return;
    };
    if old.as_ref() == Some(&new) {
        return;
    }
    menu.meter_changed = true;
    menu.meter_pending = Some(("min_rms".to_string(), new));
    if now - menu.meter_last_emit >= SLIDER_EMIT_MS / 1000.0 {
        emit_meter(menu, now);
    }
}

pub(super) fn emit_slider(menu: &mut Menu, now: f64, force: bool) {
    if menu.slider_pending.is_none() {
        return;
    }
    if !force && now - menu.slider_last_emit < SLIDER_EMIT_MS / 1000.0 {
        return;
    }
    let Some((key, value)) = menu.slider_pending.take() else {
        return;
    };
    menu.slider_last_emit = now;
    menu.notify_live_change(&key, &value);
}

pub(super) fn emit_meter(menu: &mut Menu, now: f64) {
    let Some((key, value)) = menu.meter_pending.take() else {
        return;
    };
    menu.meter_last_emit = now;
    menu.notify_live_change(&key, &value);
}
