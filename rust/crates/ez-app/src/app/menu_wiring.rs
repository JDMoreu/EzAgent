//! Port of `src/ezagent/voice/app.py 2658-3396` (F6-06b).
//!
//! Menú embebido: instantánea para el menú, cambios en vivo, tema
//! personalizado, guardado y prueba de voz, y los ajustes que el menú aplica
//! sobre la app. `_apply_settings`, `_reload_mic` y los workers de modelos,
//! Kokoro y whisper viven en [`apply`] por tamaño.
//!
//! Los efectos que pertenecen a otras tareas entran por [`MenuEffects`], que
//! extiende la costura `SettingsEffects` de F6-06a con `_hang_up` (F6-03) y las
//! voces cargadas de Kokoro (F6-04). F6-07 cablea la implementación real.

mod apply;
mod menu;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use ez_core::config::{python_str, VoiceConfig};
use ez_core::pyvalue::{char_len, py_strip, repr_str};
use ez_hud::model::settings::{normalize_setting, SettingValue, SettingsValues};
use ez_hud::paint::flow::PRESETS;
use ez_hud::win::drop::ConfigValue;
use serde_json::{Map, Value};

use super::core::{
    cfg_float, cfg_text, cfg_truthy, log, AppEvent, MenuEvent, VoiceApp, INSTANT_SETTINGS, SPEAKING,
};
use super::settings_apply::SettingsEffects;

/// Operaciones de F6-03/F6-04/F6-07 que el menú pide con su nombre del Python.
///
/// `set_state`, `set_mic_muted` y `speak` ya existen en [`SettingsEffects`];
/// aquí solo faltan `_hang_up` y las voces de Kokoro cargadas. Por omisión no
/// hacen nada: el cableado real lo instala F6-07.
pub trait MenuEffects: SettingsEffects {
    /// `self._hang_up()` (F6-03).
    fn hang_up(&self) {}
    /// `kokoro.voices` (F6-04): voces válidas; vacío = no validar.
    fn kokoro_voices(&self) -> Vec<String> {
        Vec::new()
    }
}

impl MenuEffects for VoiceApp {}

impl VoiceApp {
    /// `_hud_live_setting`: el menú publica el control y `_tick` lo aplica.
    pub fn hud_live_setting(&self, key: &str, value: ConfigValue) {
        self.inner
            .ui
            .menu_events
            .put(MenuEvent::Live(key.to_string(), value));
    }

    /// `_hud_save_settings`: `None` en el HUD = guardado pendiente.
    pub fn hud_save_settings(&self, values: &SettingsValues) {
        self.inner
            .ui
            .menu_events
            .put(MenuEvent::Save(values.clone()));
    }

    /// `_hud_test_voice`.
    pub fn hud_test_voice(&self, values: &SettingsValues) {
        self.inner
            .ui
            .menu_events
            .put(MenuEvent::Test(values.clone()));
    }

    /// `_hud_hide`: el botón «–» oculta sin cerrar los ojos (A-02).
    pub fn hud_hide(&self) {
        self.inner.ui.menu_events.put(MenuEvent::Hide);
    }

    /// `_push_custom_colors`: los cuatro colores del tema personalizado.
    pub fn push_custom_colors(&self) {
        let cfg = self.config();
        self.inner.ui.hud.set_custom_colors(
            &cfg_text(&cfg, "theme_primary", "#1FAE9F"),
            &cfg_text(&cfg, "theme_secondary", "#8FE3C0"),
            &cfg_text(&cfg, "theme_accent", "#FF8A65"),
            &cfg_text(&cfg, "theme_background", "#0E6F7C"),
        );
    }

    /// `_apply_custom_theme`: un color o una paleta base pasan a «Personalizado».
    pub fn apply_custom_theme(&self, key: &str, value: &ConfigValue) {
        if key == "theme_preset" {
            let name = setting_from_config(value).python_str();
            let Some((_, colors)) = PRESETS.iter().find(|(preset, _)| *preset == name.as_str())
            else {
                return;
            };
            for (name, color) in [
                ("theme_background", colors[0]),
                ("theme_primary", colors[1]),
                ("theme_secondary", colors[2]),
                ("theme_accent", colors[3]),
            ] {
                self.apply_instant_setting(name, &ConfigValue::Text(color.into()), false, false);
            }
        } else {
            self.apply_instant_setting(key, value, false, false);
        }
        self.push_custom_colors();
        if self.config().get("theme").and_then(Value::as_str) != Some("custom") {
            self.apply_instant_setting("theme", &ConfigValue::Text("custom".into()), true, false);
        }
    }

    /// `_apply_instant_setting`: apariencia y escucha, en vivo y opcionalmente
    /// persistidas en `voice.json`.
    pub fn apply_instant_setting(&self, key: &str, value: &ConfigValue, push: bool, persist: bool) {
        let raw = setting_from_config(value);
        let Some(value) = self.normalize_instant(key, &raw) else {
            return;
        };
        let json = setting_to_json(&value);
        {
            let mut cfg = self.inner.config.cfg.write();
            cfg.set(key, json.clone());
            if persist {
                // DECISION: se escribe en el `config_file` de la instancia, no
                // en `config_path()`, como F6-06a y F6-01.
                if let Err(error) = cfg.save_key(key, json, Some(&self.inner.config.config_file)) {
                    log(format!("No pude guardar {key}: {error}"));
                }
            }
        }
        self.apply_instant_visual(key, &value);
        if push && key.starts_with("theme_") && key != "theme_base" {
            self.push_custom_colors();
            if self.config().get("theme").and_then(Value::as_str) != Some("custom") {
                self.apply_instant_setting(
                    "theme",
                    &ConfigValue::Text("custom".into()),
                    true,
                    persist,
                );
            }
        }
        let shown = if key == "wake_phrase" {
            "(frase)".to_string()
        } else {
            value.python_str()
        };
        log(format!("Ajuste {key} = {shown}"));
    }

    fn normalize_instant(&self, key: &str, value: &SettingValue) -> Option<SettingValue> {
        match normalize_setting(key, value) {
            Ok(value) => Some(value),
            Err(error) => {
                log(format!("Ajuste {key} no válido: {error}"));
                None
            }
        }
    }

    fn apply_instant_visual(&self, key: &str, value: &SettingValue) {
        let hud = &self.inner.ui.hud;
        match key {
            "ui_layout" => hud.set_layout(&value.python_str()),
            "theme" => hud.set_theme(&value.python_str()),
            "theme_base" => hud.set_custom_base(&value.python_str()),
            "ui_motion" => hud.set_motion(&value.python_str()),
            "ui_scale" => {
                if let Ok(number) = setting_float(value) {
                    hud.set_bar_scale(number);
                }
            }
            "chat_voice" => self.inner.chat.chat.set_voice_replies(value.truthy()),
            _ => {}
        }
    }

    /// `_handle_menu_event`: aplica la intención que dejó el menú.
    pub fn handle_menu_event(&self, event: &MenuEvent) -> Result<(), String> {
        self.handle_menu_event_with(event, self)
    }

    /// Igual que [`Self::handle_menu_event`] con efectos intercambiables.
    pub fn handle_menu_event_with(
        &self,
        event: &MenuEvent,
        effects: &dyn MenuEffects,
    ) -> Result<(), String> {
        match event {
            MenuEvent::Live(key, value) => self.apply_live_setting(key, value),
            MenuEvent::Save(values) => self.save_from_menu_with(values, effects),
            MenuEvent::Test(values) => self.test_voice_from_menu_with(values, effects)?,
            // El botón «–» oculta con retirada, sin cerrar los ojos (A-02).
            MenuEvent::Hide => self.hide_hud(false),
        }
        Ok(())
    }

    /// `_apply_live_setting`: aplica un control del menú sin persistirlo.
    pub fn apply_live_setting(&self, key: &str, value: &ConfigValue) {
        if key == "theme_preset" {
            self.apply_custom_theme(key, value);
            return;
        }
        if INSTANT_SETTINGS.contains(&key) {
            self.apply_instant_setting(key, value, true, false);
            return;
        }
        if let Err(error) = self.apply_live_checked(key, value) {
            log(format!("Ajuste en vivo {key}: {error}"));
        }
    }

    /// `_restore_config`: todos los campos vuelven a la instantánea.
    pub fn restore_config(target: &mut VoiceConfig, snapshot: &VoiceConfig) {
        *target = snapshot.clone();
    }

    /// `_save_from_menu`: guarda y restaura la configuración si algo falla.
    pub fn save_from_menu(&self, values: &SettingsValues) {
        self.save_from_menu_with(values, self);
    }

    /// Igual que [`Self::save_from_menu`] con efectos intercambiables.
    pub fn save_from_menu_with(&self, values: &SettingsValues, effects: &dyn MenuEffects) {
        let before = self.config();
        let mut ok = false;
        match self.apply_settings_with(values, effects) {
            Ok(_) => {
                *self.inner.config.menu_saved.lock() = Some(settings_to_map(values));
                ok = true;
            }
            Err(error) => {
                self.set_config(before);
                log(format!("No pude guardar los ajustes: {error}"));
            }
        }
        if ok {
            self.inner.ui.hud.refresh_menu_settings();
        }
        self.inner.ui.hud.set_save_result(ok);
    }

    /// `_test_voice_from_menu`: prueba la voz elegida sin tocar la persistida.
    pub fn test_voice_from_menu(&self, values: &SettingsValues) -> Result<(), String> {
        self.test_voice_from_menu_with(values, self)
    }

    /// Igual que [`Self::test_voice_from_menu`] con efectos intercambiables.
    pub fn test_voice_from_menu_with(
        &self,
        values: &SettingsValues,
        effects: &dyn MenuEffects,
    ) -> Result<(), String> {
        let cfg = self.config();
        let voice = match values.get("voice") {
            Some(value) if value.truthy() => value.python_str(),
            _ => cfg_text(&cfg, "voice", ""),
        };
        let speed = match values.get("speed") {
            Some(value) if value.truthy() => setting_float(value)?,
            _ => cfg_float(&cfg, "speed", 0.95),
        };
        let old_muted = self
            .inner
            .session
            .muted
            .load(std::sync::atomic::Ordering::SeqCst);
        let old_barge = cfg_truthy(&cfg, "barge_in", true);
        self.inner
            .config
            .cfg
            .write()
            .set("barge_in", Value::Bool(false));
        if !old_muted {
            effects.set_mic_muted(true, "");
        }
        self.inner.ui.hud.set_testing(true);
        effects.set_state(SPEAKING);
        let app = self.clone();
        self.start_worker("ezagent-voice-test", move || {
            app.run_voice_test(voice, speed, old_muted, old_barge);
        });
        Ok(())
    }

    fn run_voice_test(&self, voice: String, speed: f64, old_muted: bool, old_barge: bool) {
        let cfg = self.config();
        let old_voice = cfg_text(&cfg, "voice", "");
        let old_speed = cfg_float(&cfg, "speed", 0.95);
        let result = self.speak_test_phrase(&voice, speed);
        if let Err(error) = result {
            log(format!("Prueba de voz: {error}"));
        }
        let mut cfg = self.inner.config.cfg.write();
        cfg.set("voice", Value::String(old_voice.clone()));
        cfg.set("speed", Value::from(old_speed));
        drop(cfg);
        let _ = self.inner.speech.speaker.set_voice(&old_voice);
        let encoded = if old_barge { 1.0 } else { 0.0 };
        // DECISION: `AppEvent::TestDone` guarda el barge-in como número (F6-01);
        // el manejo de F6-07 lo vuelve a leer con `bool(...)`.
        self.inner
            .ui
            .events
            .put(AppEvent::TestDone(old_muted, encoded));
    }

    fn speak_test_phrase(&self, voice: &str, speed: f64) -> Result<(), String> {
        {
            let mut cfg = self.inner.config.cfg.write();
            cfg.set("voice", Value::String(voice.to_string()));
            cfg.set("speed", Value::from(speed));
        }
        let speaker = Arc::clone(&self.inner.speech.speaker);
        speaker.set_config(self.config());
        speaker.set_voice(voice)?;
        let stop = std::sync::atomic::AtomicBool::new(false);
        let hud = Arc::clone(&self.inner.ui.hud);
        let mut on_level = |level: f64| hud.draw(level, Some(SPEAKING));
        speaker.speak(
            "Hola, soy tu asistente de voz.",
            &stop,
            Some(&mut on_level),
            None,
        )?;
        Ok(())
    }
}

fn json_to_setting(value: &Value) -> SettingValue {
    match value {
        Value::Null => SettingValue::None,
        Value::Bool(flag) => SettingValue::Bool(*flag),
        Value::String(text) => SettingValue::Text(text.clone()),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => SettingValue::Int(integer),
            None => SettingValue::Float(number.as_f64().unwrap_or_default()),
        },
        other => SettingValue::Text(python_str(other)),
    }
}

fn setting_to_json(value: &SettingValue) -> Value {
    match value {
        SettingValue::None => Value::Null,
        SettingValue::Bool(flag) => Value::Bool(*flag),
        SettingValue::Int(number) => Value::from(*number),
        SettingValue::Float(number) => Value::from(*number),
        SettingValue::Text(text) => Value::String(text.clone()),
    }
}

fn setting_from_config(value: &ConfigValue) -> SettingValue {
    match value {
        ConfigValue::Bool(flag) => SettingValue::Bool(*flag),
        ConfigValue::Int(number) => SettingValue::Int(*number),
        ConfigValue::Float(number) => SettingValue::Float(*number),
        ConfigValue::Text(text) => SettingValue::Text(text.clone()),
    }
}

fn settings_to_map(values: &SettingsValues) -> Map<String, Value> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), setting_to_json(value)))
        .collect()
}

fn map_to_settings(values: &Map<String, Value>) -> SettingsValues {
    values
        .iter()
        .map(|(key, value)| (key.clone(), json_to_setting(value)))
        .collect()
}

/// `str(value or "")`.
fn setting_text_or_empty(value: Option<&SettingValue>) -> String {
    match value {
        Some(value) if value.truthy() => value.python_str(),
        _ => String::new(),
    }
}

/// `str(values.get(key, getattr(cfg, key)))`.
fn setting_python_str_or(values: &SettingsValues, key: &str, cfg: &VoiceConfig) -> String {
    match values.get(key) {
        Some(value) => value.python_str(),
        None => cfg
            .get(key)
            .map(python_str)
            .unwrap_or_else(|| "None".to_string()),
    }
}

/// `str(values.get(key, getattr(cfg, key)) or "")`.
fn setting_text_or_cfg(values: &SettingsValues, key: &str, cfg: &VoiceConfig) -> String {
    match values.get(key) {
        Some(value) => setting_text_or_empty(Some(value)),
        None => cfg
            .get(key)
            .map(setting_text_or_empty_value)
            .unwrap_or_default(),
    }
}

fn setting_text_or_empty_value(value: &Value) -> String {
    setting_text_or_empty(Some(&json_to_setting(value)))
}

/// `float(value)` con el texto de error de CPython.
fn setting_float(value: &SettingValue) -> Result<f64, String> {
    match value {
        SettingValue::None => {
            Err("float() argument must be a string or a real number, not 'NoneType'".to_string())
        }
        SettingValue::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
        SettingValue::Int(number) => Ok(*number as f64),
        SettingValue::Float(number) => Ok(*number),
        // DECISION: el menú entrega números; no se portan los guiones bajos
        // numéricos de `float("1_000")`.
        SettingValue::Text(text) => py_strip(text)
            .parse::<f64>()
            .map_err(|_| format!("could not convert string to float: {}", repr_str(text))),
    }
}

/// `int(value)` con el texto de error de CPython.
fn setting_int(value: &SettingValue) -> Result<i64, String> {
    match value {
        SettingValue::None => Err(
            "int() argument must be a string, a bytes-like object or a real number, not 'NoneType'"
                .to_string(),
        ),
        SettingValue::Bool(flag) => Ok(i64::from(*flag)),
        SettingValue::Int(number) => Ok(*number),
        SettingValue::Float(number) => Ok(number.trunc() as i64),
        SettingValue::Text(text) => ez_core::config::python_int(text)
            .ok_or_else(|| format!("invalid literal for int() with base 10: {}", repr_str(text))),
    }
}

/// `int(round(float(value)))`, con el redondeo a par de Python.
fn setting_rounded_int(value: &SettingValue) -> Result<i64, String> {
    Ok(setting_float(value)?.round_ties_even() as i64)
}

/// `str(values.get("llm_profile_name", "") or "").strip().len() > 80`.
fn profile_name_too_long(values: &SettingsValues) -> bool {
    let raw = setting_text_or_empty(values.get("llm_profile_name"));
    char_len(py_strip(&raw)) > 80
}

/// Schedula `_start_call` como `root.after(250, ...)`.
fn schedule_start_call(app: &VoiceApp) {
    let next = app.clone();
    app.inner
        .ui
        .ui
        .schedule(Duration::from_millis(250), move || {
            next.hooks().start_call()
        });
}
