//! Pasos de chat, menú y esperas de F7-01c.

use std::path::PathBuf;

use ez_core::pyvalue::{py_eq, python_str};
use ez_hud::model::settings::{setting_value, SettingValue};
use serde_json::{json, Map, Value};

use crate::app::core::AppEvent;

use super::super::driver::{ready, Driver, Pending, Step};
use super::{describe, integer, object, predicate};

/// `hud: {action: chat_send, text: "…"}`.
pub struct Hud;

impl Step for Hud {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("hud").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe("hud", step, step.get("hud").unwrap_or(&Value::Null))
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let spec = object(step, "hud");
        let action = spec.get("action").map(python_str).unwrap_or_default();
        let events = driver.app.events();
        match action.as_str() {
            "chat_send" => events.put(AppEvent::ChatSend(
                spec.get("text").map(python_str).unwrap_or_default(),
            )),
            "chat_new" => events.put(AppEvent::ChatNew),
            "chat_stop" => events.put(AppEvent::ChatStop),
            "chat_switch" => events.put(AppEvent::ChatSwitch(integer(spec.get("session")))),
            "chat_delete" => events.put(AppEvent::ChatDelete(integer(spec.get("session")))),
            "chat_mic" => events.put(AppEvent::ChatMute(
                spec.get("muted").is_some_and(ez_core::config::truthy),
            )),
            "chat_menu" => events.put(AppEvent::HudAction("panel".to_string())),
            "chat_confirm" => events.put(AppEvent::ConfirmClick(
                spec.get("yes").is_some_and(ez_core::config::truthy),
                None,
            )),
            "chat_voice" => driver
                .app
                .set_chat_voice(spec.get("enabled").is_some_and(ez_core::config::truthy)),
            _ => return Err(format!("el HUD no expone on_{action}")),
        }
        driver
            .trace
            .record("hud.panel", json!({ "action": action }));
        Ok(ready())
    }
}

/// `menu_save: {clave: valor}`.
pub struct MenuSave;

impl Step for MenuSave {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("menu_save").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "menu_save",
            step,
            step.get("menu_save").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let values = object(step, "menu_save");
        let Some(mut settings) = driver.app.hud_menu_settings() else {
            return Err("el menú no está disponible".to_string());
        };
        for (key, value) in &values {
            let item = setting_from_json(value);
            settings
                .set(key, &item)
                .map_err(|error| format!("Ajuste {key} no válido: {error:?}"))?;
        }
        driver.app.hud_save_settings(&settings.snapshot());
        let mut keys: Vec<String> = values.keys().cloned().collect();
        keys.sort();
        driver.trace.record("menu.save", json!({ "keys": keys }));
        Ok(ready())
    }
}

pub(super) fn setting_from_json(value: &Value) -> SettingValue {
    match value {
        Value::Bool(flag) => setting_value(*flag),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => setting_value(integer),
            None => setting_value(number.as_f64().unwrap_or_default()),
        },
        Value::String(text) => setting_value(text.clone()),
        Value::Null => SettingValue::None,
        other => setting_value(scalar(other)),
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `wait_config: {clave: valor}`.
pub struct WaitConfig;

impl Step for WaitConfig {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("wait_config").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "wait_config",
            step,
            step.get("wait_config").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let want = object(step, "wait_config");
        let timeout = step.get("timeout").and_then(Value::as_f64).unwrap_or(20.0);
        let app = driver.app.clone();
        Ok(predicate(
            timeout,
            move || {
                let cfg = app.config();
                Ok(want
                    .iter()
                    .all(|(key, value)| cfg.get(key).is_some_and(|actual| py_eq(actual, value))))
            },
            "los ajustes esperados",
        ))
    }
}

/// `wait_voice: {clave: valor}`.
pub struct WaitVoice;

impl Step for WaitVoice {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("wait_voice").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "wait_voice",
            step,
            step.get("wait_voice").unwrap_or(&Value::Null),
        )
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let want = object(step, "wait_voice");
        let timeout = step.get("timeout").and_then(Value::as_f64).unwrap_or(20.0);
        Ok(predicate(
            timeout,
            move || {
                let Some(path) = voice_json_path() else {
                    return Ok(false);
                };
                let Ok(text) = std::fs::read_to_string(&path) else {
                    return Ok(false);
                };
                let Ok(data) = ez_core::pyloads::loads(&text) else {
                    return Ok(false);
                };
                Ok(want
                    .iter()
                    .all(|(key, value)| data.get(key).is_some_and(|actual| py_eq(actual, value))))
            },
            "voice.json con los valores esperados",
        ))
    }
}

fn voice_json_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join("EzAgent").join("voice.json"))
}
