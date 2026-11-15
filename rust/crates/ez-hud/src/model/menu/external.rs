//! Port of `src/ezagent/voice/hud/menu.py 699-1342` (F5-07): configuración
//! externa del menú y utilidades de estado que comparten los demás módulos.

use crate::anim::EASE_OUT;
use crate::model::settings::{SettingValue, SettingsValues, TOGGLE_KEYS};
use crate::tokens::{palette, T_MICRO};

use super::geometry::{Menu, PaletteProvider, BUTTON_RELEASE_MS, TEST_FADE_MS, TOGGLE_MS};

impl Menu {
    /// Sustituye el modelo de vista (por ejemplo, al reabrir el menú).
    pub fn set_settings(
        &mut self,
        settings: crate::model::settings::MenuSettings,
        now: Option<f64>,
    ) {
        self.cancel_text_edit_quiet();
        let moment = now.unwrap_or(self.now);
        self.settings = settings;
        for key in TOGGLE_KEYS {
            let desired = if self.settings.get(key).is_some_and(|value| value.truthy()) {
                1.0
            } else {
                0.0
            };
            self.toggle_pos
                .get_mut(key)
                .expect("clave de interruptor")
                .to(desired, moment, TOGGLE_MS, crate::anim::EASE_SPRING);
            self.toggle_color
                .get_mut(key)
                .expect("clave de interruptor")
                .to(desired, moment, TOGGLE_MS, EASE_OUT);
        }
        self.clamp_list_scroll();
    }

    /// Cambia el tema; un nombre desconocido cae a «dark», como el Python.
    pub fn set_theme(&mut self, theme: &str) {
        self.theme = match theme {
            "auto" => "auto",
            "light" => "light",
            _ => "dark",
        };
    }

    /// ¿El botón de una acción destructiva espera la confirmación?
    pub fn is_armed(&self, key: &str, now: f64) -> bool {
        self.armed.get(key).is_some_and(|until| *until > now)
    }

    /// `provider(theme)` devuelve colores que sustituyen a los tokens.
    pub fn set_palette_overrides(&mut self, provider: Option<PaletteProvider>) {
        self.palette_overrides = provider;
    }

    /// Paleta del tema con las sobrescrituras de la aplicación, si las hay.
    pub fn palette(&self) -> crate::tokens::ColorMap {
        let base = palette(self.theme).expect("el tema del menú está validado");
        match &self.palette_overrides {
            None => base,
            Some(provider) => {
                let mut merged = base;
                merged.extend(provider(self.theme));
                merged
            }
        }
    }

    /// Escala visual; el Python lanza `ValueError` si no es positiva.
    pub fn set_scale(&mut self, scale: f64) -> Result<(), String> {
        if scale <= 0.0 {
            return Err("scale debe ser mayor que cero".into());
        }
        self.scale = scale;
        Ok(())
    }

    /// Activa o desactiva las animaciones por accesibilidad.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    /// Nivel RMS del micrófono para el medidor de entrada/umbral.
    pub fn set_input_level(&mut self, level: f64, muted: bool, active: bool) {
        // `float(level or 0.0)` y `max(0.0, value)` del Python.
        self.meter_target = level.max(0.0);
        self.meter_muted = muted;
        self.meter_active = active;
    }

    /// Estado del botón «Probar»; la aplicación lo controla al terminar.
    pub fn set_testing(&mut self, testing: bool, now: Option<f64>) {
        let moment = now.unwrap_or(self.now);
        let target = if testing { 1.0 } else { 0.0 };
        self.testing_t.to(target, moment, TEST_FADE_MS, EASE_OUT);
    }

    /// `on_live_change(clave, valor)` si la aplicación lo registró.
    pub fn notify_live_change(&self, key: &str, value: &SettingValue) {
        if let Some(callback) = &self.callbacks.live_change {
            callback(key, value);
        }
    }

    /// `on_text_edit(activo)` si la aplicación lo registró.
    pub fn notify_text_edit(&self, active: bool) {
        if let Some(callback) = &self.callbacks.text_edit {
            callback(active);
        }
    }

    /// `on_hotkey_capture(activo)` si la aplicación lo registró.
    pub fn notify_hotkey_capture(&self, active: bool) {
        if let Some(callback) = &self.callbacks.hotkey_capture {
            callback(active);
        }
    }

    /// `on_action(clave)` si la aplicación lo registró.
    pub fn notify_action(&self, key: &str) {
        if let Some(callback) = &self.callbacks.action {
            callback(key);
        }
    }

    /// `on_hide()` si la aplicación lo registró.
    pub fn notify_hide(&self) {
        if let Some(callback) = &self.callbacks.hide {
            callback();
        }
    }

    /// `on_close()` si la aplicación lo registró.
    pub fn notify_close(&self) {
        if let Some(callback) = &self.callbacks.close {
            callback();
        }
    }

    /// `on_copy(texto)`; `false` si no hay callback o lo rechaza.
    pub fn notify_copy(&self, text: &str) -> bool {
        self.callbacks
            .copy
            .as_ref()
            .is_some_and(|callback| callback(text))
    }

    /// `on_save(instantánea)`; `None` si no hay callback.
    pub fn notify_save(&self, values: &SettingsValues) -> Option<bool> {
        self.callbacks
            .save
            .as_ref()
            .and_then(|callback| callback(values))
    }

    /// `on_test(instantánea)` si la aplicación lo registró.
    pub fn notify_test(&self, values: &SettingsValues) {
        if let Some(callback) = &self.callbacks.test {
            callback(values);
        }
    }

    /// Descarta la edición sin avisar al llamador (cierre/reemplazo).
    pub(crate) fn cancel_text_edit_quiet(&mut self) {
        if self.editing_key.is_none() {
            return;
        }
        if self.capturing {
            self.finish_capture();
            return;
        }
        self.editing_key = None;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        self.edit_select_all = false;
        self.notify_text_edit(false);
    }

    /// Fin de la captura de un atajo: suelta el teclado y avisa a la aplicación.
    pub(crate) fn finish_capture(&mut self) {
        if !self.capturing {
            return;
        }
        self.capturing = false;
        self.editing_key = None;
        self.capture_mods.clear();
        self.capture_msg = None;
        self.notify_text_edit(false);
        self.notify_hotkey_capture(false);
    }

    /// `_finish_text_edit`: confirma o descarta el texto de la fila activa.
    pub(crate) fn apply_text_edit(&mut self, commit: bool) {
        let Some(key) = self.editing_key.clone() else {
            return;
        };
        if self.capturing {
            // Sin combinación completa no hay nada que confirmar.
            self.finish_capture();
            return;
        }
        let text = self.edit_buffer.clone();
        if commit {
            let old = self.settings.get(&key).cloned();
            match self.settings.set(&key, &SettingValue::Text(text)) {
                Err(error) => {
                    self.edit_error = Some((key, error.to_string()));
                    return;
                }
                Ok(new) => {
                    if old.as_ref() != Some(&new) {
                        self.notify_live_change(&key, &new);
                    }
                }
            }
        }
        self.editing_key = None;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        self.edit_select_all = false;
        self.edit_error = None;
        self.notify_text_edit(false);
    }

    /// `_finish_meter_drag`: emite el umbral final y suelta el arrastre.
    pub(crate) fn release_meter_drag(&mut self, now: f64, emit: bool) {
        if !self.meter_drag {
            return;
        }
        if emit && (self.meter_pending.is_some() || self.meter_changed) {
            self.meter_last_emit = now;
            let value = self
                .settings
                .get("min_rms")
                .cloned()
                .unwrap_or(SettingValue::None);
            self.notify_live_change("min_rms", &value);
        }
        self.meter_pending = None;
        self.meter_changed = false;
        self.meter_drag = false;
    }

    /// `_finish_slider_drag`: emite el valor final y suelta el arrastre.
    pub(crate) fn release_slider_drag(&mut self, now: f64, emit: bool) {
        let Some(hit) = self.slider_drag.clone() else {
            return;
        };
        if emit && (self.slider_pending.is_some() || self.slider_changed) {
            let value = self
                .settings
                .get(&hit.key)
                .cloned()
                .unwrap_or(SettingValue::None);
            self.slider_last_emit = now;
            self.notify_live_change(&hit.key, &value);
        }
        self.slider_pending = None;
        self.slider_changed = false;
        self.slider_drag = None;
        self.slider_color
            .get_mut(&hit.key)
            .expect("clave de slider")
            .to(0.0, now, T_MICRO as f64, EASE_OUT);
    }

    /// `_clear_press`: todos los botones vuelven a su estado de reposo.
    pub(crate) fn release_press(&mut self, now: f64) {
        for tween in self.press.values_mut() {
            tween.to(0.0, now, BUTTON_RELEASE_MS, EASE_OUT);
        }
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use std::sync::Arc;

    use crate::model::settings::{MenuSettings, SettingValue};
    use crate::tokens::{Color, ColorMap, Rect};

    use super::super::geometry::{Menu, MenuCallbacks, MenuHit, MenuHitKind, MenuOptions};

    fn menu(callbacks: MenuCallbacks) -> Menu {
        Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks).expect("menú")
    }

    #[test]
    fn scale_and_theme_follow_the_python_validation() {
        let mut menu = menu(MenuCallbacks::default());
        assert_eq!(
            menu.set_scale(0.0).expect_err("cero no es válido"),
            "scale debe ser mayor que cero"
        );
        menu.set_scale(2.0).expect("escala válida");
        assert_eq!(menu.scale, 2.0);
        menu.set_theme("light");
        assert_eq!(menu.theme, "light");
        menu.set_theme("neón");
        assert_eq!(menu.theme, "dark");
    }

    #[test]
    fn palette_overrides_replace_token_colors() {
        let mut menu = menu(MenuCallbacks::default());
        menu.set_palette_overrides(Some(Box::new(|_theme: &str| {
            ColorMap::from([("TEXT", Color::Rgb([1, 2, 3]))])
        })));
        let palette = menu.palette();
        assert_eq!(palette.get("TEXT"), Some(&Color::Rgb([1, 2, 3])));
        assert!(palette.contains_key("MINT"));
    }

    #[test]
    fn armed_actions_expire() {
        let mut menu = menu(MenuCallbacks::default());
        menu.armed.insert("new_conversation".into(), 5.0);
        assert!(menu.is_armed("new_conversation", 4.9));
        assert!(!menu.is_armed("new_conversation", 5.0));
        assert!(!menu.is_armed("repeat_last", 0.0));
    }

    #[test]
    fn input_level_and_testing_state_are_kept() {
        let mut menu = menu(MenuCallbacks::default());
        menu.set_input_level(-1.0, true, true);
        assert_eq!(menu.meter_target, 0.0);
        assert!(menu.meter_muted && menu.meter_active);
        menu.set_testing(true, Some(2.0));
        assert!(menu.testing_t.active());
        assert!(menu.is_animating(2.05));
        menu.set_testing(false, Some(3.0));
        assert!(!menu.is_animating(3.2));
    }

    #[test]
    fn text_edit_commit_updates_settings_and_releases_the_keyboard() {
        let states = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&states);
        let callbacks = MenuCallbacks {
            text_edit: Some(Box::new(move |active| recorded.lock().push(active))),
            ..MenuCallbacks::default()
        };
        let mut menu = menu(callbacks);
        menu.editing_key = Some("wake_phrase".into());
        menu.edit_buffer = "Hola EzAgent".into();
        menu.apply_text_edit(true);
        assert!(menu.editing_key.is_none());
        assert_eq!(
            menu.settings.get("wake_phrase"),
            Some(&SettingValue::Text("Hola EzAgent".into()))
        );
        assert_eq!(states.lock().as_slice(), &[false]);
    }

    #[test]
    fn cancel_text_edit_keeps_the_old_value() {
        let mut menu = menu(MenuCallbacks::default());
        menu.settings
            .set("wake_phrase", &SettingValue::Text("antes".into()))
            .expect("clave válida");
        menu.editing_key = Some("wake_phrase".into());
        menu.edit_buffer = "después".into();
        menu.cancel_text_edit_quiet();
        assert_eq!(
            menu.settings.get("wake_phrase"),
            Some(&SettingValue::Text("antes".into()))
        );
        assert!(menu.editing_key.is_none());
    }

    #[test]
    fn slider_release_emits_the_last_value_once() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&events);
        let callbacks = MenuCallbacks {
            live_change: Some(Box::new(move |key, value| {
                recorded.lock().push((key.to_string(), value.clone()));
            })),
            ..MenuCallbacks::default()
        };
        let mut menu = menu(callbacks);
        menu.settings
            .set("speed", &SettingValue::Float(1.25))
            .expect("clave válida");
        menu.slider_drag = Some(MenuHit::new(
            MenuHitKind::Slider,
            "speed",
            Rect::new(0.0, 0.0, 1.0, 1.0),
        ));
        menu.slider_changed = true;
        menu.release_slider_drag(1.0, true);
        assert_eq!(
            events.lock().as_slice(),
            &[("speed".to_string(), SettingValue::Float(1.25))]
        );
        assert!(menu.slider_drag.is_none());
    }

    #[test]
    fn save_and_copy_callbacks_keep_their_return_values() {
        let callbacks = MenuCallbacks {
            save: Some(Box::new(|_values| Some(false))),
            copy: Some(Box::new(|text| text == "clave")),
            ..MenuCallbacks::default()
        };
        let menu = menu(callbacks);
        assert_eq!(menu.notify_save(&menu.settings.snapshot()), Some(false));
        assert!(menu.notify_copy("clave"));
        assert!(!menu.notify_copy("otra"));
    }
}
