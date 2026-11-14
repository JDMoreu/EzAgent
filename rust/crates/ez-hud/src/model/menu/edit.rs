//! Port of `src/ezagent/voice/hud/menu.py 2034-2253` (F5-09).
//!
//! Edición del texto de una fila: cursor, selección, pegado y captura de
//! atajos. El portapapeles llega como dato inyectable y no se toca Win32.

use ez_core::pyvalue::{char_len, py_splitlines};

use crate::model::settings::{SettingValue, LABELS, TEXT_KEYS};
use crate::paint::hotkey_field::{self, CaptureKind, Mods};

use super::geometry::Menu;

/// Tiempo que se ve el motivo de un atajo rechazado.
pub const CAPTURE_MSG_SECS: f64 = 2.6;

/// Corta `text` en el índice de carácter `index`, como `text[:index]` y
/// `text[index:]` de Python.
fn split_at_char(text: &str, index: usize) -> (&str, &str) {
    match text.char_indices().nth(index) {
        Some((byte, _)) => text.split_at(byte),
        None => (text, ""),
    }
}

impl Menu {
    /// Clave de la fila que tiene el teclado, o `None`.
    pub fn editing_key(&self) -> Option<&str> {
        self.editing_key.as_deref()
    }

    /// `text_buffer`: texto que se está editando.
    pub fn text_buffer(&self) -> &str {
        &self.edit_buffer
    }

    /// Activa la edición de una fila de texto y pide el teclado.
    pub fn begin_text_edit(&mut self, key: &str, now: Option<f64>) {
        if !TEXT_KEYS.contains(&key) {
            return;
        }
        if self.editing_key.as_deref() == Some(key) {
            return;
        }
        if self.editing_key.is_some() {
            self.apply_text_edit(true);
            if self.editing_key.is_some() {
                return;
            }
        }
        if hotkey_field::HOTKEY_KEYS.contains(&key) {
            self.begin_capture(key, now);
            return;
        }
        let text = self.secret_text(key);
        self.editing_key = Some(key.to_string());
        self.edit_error = None;
        self.edit_buffer = text;
        self.edit_cursor = char_len(&self.edit_buffer);
        self.edit_select_all = false;
        self.edit_t0 = now.unwrap_or(self.now);
        self.notify_text_edit(true);
    }

    /// Confirma o descarta el texto y devuelve el teclado.
    pub fn finish_text_edit(&mut self, commit: bool) {
        self.apply_text_edit(commit);
    }

    /// `_begin_capture`: la siguiente combinación sustituye al atajo.
    fn begin_capture(&mut self, key: &str, now: Option<f64>) {
        self.editing_key = Some(key.to_string());
        self.capturing = true;
        self.capture_mods.clear();
        self.capture_msg = None;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        self.edit_select_all = false;
        self.edit_t0 = now.unwrap_or(self.now);
        self.notify_text_edit(true);
        self.notify_hotkey_capture(true);
    }

    /// `_capture_press`: espera, cancela, rechaza o cierra el atajo.
    fn capture_press(&mut self, vk: i32) -> bool {
        let key = self.editing_key.clone().unwrap_or_default();
        let held = self.modifiers();
        let names: Vec<&str> = held.iter().map(String::as_str).collect();
        let mods = Mods::from_names(&names);
        self.capture_mods = held;
        let event = hotkey_field::press(vk, mods);
        match event.kind {
            CaptureKind::Cancel => self.finish_capture(),
            CaptureKind::Reject => {
                self.capture_msg = Some((event.reason, self.now + CAPTURE_MSG_SECS));
            }
            CaptureKind::Done => {
                let others: Vec<(&str, String)> = hotkey_field::HOTKEY_KEYS
                    .iter()
                    .map(|name| {
                        let text = self
                            .settings
                            .get(name)
                            .map(|value| {
                                if value.truthy() {
                                    value.python_str()
                                } else {
                                    String::new()
                                }
                            })
                            .unwrap_or_default();
                        (*name, text)
                    })
                    .collect();
                let current: Vec<(&str, &str)> = others
                    .iter()
                    .map(|(name, text)| (*name, text.as_str()))
                    .collect();
                let labels: Vec<(&str, &str)> =
                    LABELS.iter().map(|(name, label)| (*name, *label)).collect();
                if let Some(taken) = hotkey_field::conflict(&event.spec, &current, &key, &labels) {
                    self.capture_msg =
                        Some((format!("Ya la usa «{taken}»"), self.now + CAPTURE_MSG_SECS));
                    return true;
                }
                let old = self.settings.get(&key).cloned();
                if let Ok(new) = self.settings.set(&key, &SettingValue::Text(event.spec)) {
                    if old.as_ref() != Some(&new) {
                        self.notify_live_change(&key, &new);
                    }
                }
                self.finish_capture();
            }
            CaptureKind::Pending => {}
        }
        true
    }

    /// Pega texto de una sola línea en la fila activa.
    pub fn paste_text(&mut self, text: &str) -> bool {
        if self.editing_key.is_none() {
            return false;
        }
        if self.capturing {
            return true;
        }
        let clean = if self.editing_key.as_deref() == Some("stt_dictionary") {
            py_splitlines(text).join("; ")
        } else {
            text.replace(['\r', '\n'], " ")
        };
        self.insert_text(&clean);
        true
    }

    /// Introduce un carácter imprimible en la fila activa.
    pub fn text_input(&mut self, char: &str) -> bool {
        if self.editing_key.is_none() {
            return false;
        }
        if self.capturing || char.is_empty() {
            return true;
        }
        for ch in char.chars() {
            if ch == '\r' || ch == '\n' {
                self.apply_text_edit(true);
                return true;
            }
            // Backspace y Supr ya llegan por `text_key`; WM_CHAR también los
            // emite, así que aquí se ignoran para no borrar dos caracteres.
            if ch as u32 >= 32 {
                self.insert_text(&ch.to_string());
            }
        }
        true
    }

    /// Teclas de edición (Windows VK). Devuelve `true` si se consumieron.
    pub fn text_key(&mut self, vk: i32, ctrl: bool) -> bool {
        if self.editing_key.is_none() {
            return false;
        }
        if self.capturing {
            return self.capture_press(vk);
        }
        match vk {
            27 => self.apply_text_edit(false), // Esc
            13 => self.apply_text_edit(true),  // Enter
            8 => self.backspace(),             // Backspace
            46 => self.delete(),               // Delete
            37 => {
                // Left
                self.edit_select_all = false;
                self.edit_cursor = self.edit_cursor.saturating_sub(1);
            }
            39 => {
                // Right
                self.edit_select_all = false;
                self.edit_cursor = (self.edit_cursor + 1).min(char_len(&self.edit_buffer));
            }
            36 => {
                // Home
                self.edit_select_all = false;
                self.edit_cursor = 0;
            }
            35 => {
                // End
                self.edit_select_all = false;
                self.edit_cursor = char_len(&self.edit_buffer);
            }
            65 if ctrl => {
                // Ctrl+A
                self.edit_select_all = true;
                self.edit_cursor = char_len(&self.edit_buffer);
            }
            _ => return false,
        }
        true
    }

    /// `_insert_text`: inserta en el cursor, sustituyendo la selección.
    fn insert_text(&mut self, text: &str) {
        self.edit_error = None;
        if text.is_empty() {
            return;
        }
        if self.edit_select_all {
            self.edit_buffer.clear();
            self.edit_select_all = false;
        }
        let cursor = self.edit_cursor.min(char_len(&self.edit_buffer));
        let (head, tail) = split_at_char(&self.edit_buffer, cursor);
        self.edit_buffer = format!("{head}{text}{tail}");
        self.edit_cursor = cursor + char_len(text);
    }

    /// `_backspace`.
    fn backspace(&mut self) {
        if self.edit_select_all {
            self.edit_buffer.clear();
            self.edit_cursor = 0;
            self.edit_select_all = false;
            return;
        }
        let cursor = self.edit_cursor.min(char_len(&self.edit_buffer));
        if cursor == 0 {
            return;
        }
        let (head, rest) = split_at_char(&self.edit_buffer, cursor - 1);
        let (_removed, tail) = split_at_char(rest, 1);
        self.edit_buffer = format!("{head}{tail}");
        self.edit_cursor = cursor - 1;
    }

    /// `_delete`.
    fn delete(&mut self) {
        if self.edit_select_all {
            self.edit_buffer.clear();
            self.edit_cursor = 0;
            self.edit_select_all = false;
            return;
        }
        let length = char_len(&self.edit_buffer);
        let cursor = self.edit_cursor.min(length);
        if cursor >= length {
            return;
        }
        let (head, rest) = split_at_char(&self.edit_buffer, cursor);
        let (_removed, tail) = split_at_char(rest, 1);
        self.edit_buffer = format!("{head}{tail}");
    }
}

#[cfg(test)]
mod tests;
