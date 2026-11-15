//! Pasos de chat, menú y esperas de F7-01c.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ez_core::pyvalue::{py_eq, python_str};
use ez_hud::model::settings::{setting_value, SettingValue};
use serde_json::{json, Map, Value};

use crate::app::core::{AppEvent, VoiceApp};

use super::super::driver::{describe_first, flag, ready, Driver, Pending, Step};
use super::super::swarm_c;
use super::{describe, integer, object, predicate};

/// Terminales del estado de `fake-hc`.
fn hc_count() -> Result<i64, String> {
    let state = swarm_c::hc_state()?;
    Ok(state
        .get("terminals")
        .and_then(Value::as_array)
        .map_or(0, |terminals| terminals.len() as i64))
}

/// `wait_confirm_settled: true`.
pub struct WaitConfirmSettled;

impl Step for WaitConfirmSettled {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "wait_confirm_settled")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_confirm_settled")
    }

    fn apply(
        &self,
        driver: &Driver,
        _step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let app = driver.app.clone();
        Ok(predicate(
            20.0,
            move || {
                Ok(!crate::app::events::confirm_pending(&app)
                    && app.state() != "thinking"
                    && app.state() != "speaking")
            },
            "la confirmación pendiente",
        ))
    }
}

/// `wait_hc: 1`.
pub struct WaitHc;

impl Step for WaitHc {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_hc")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_hc")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let wanted = integer(step.get("wait_hc"));
        Ok(predicate(
            20.0,
            move || Ok(hc_count()? >= wanted),
            "las terminales de fake-hc",
        ))
    }
}

/// `hc_set: {name, state, conclusion}`.
pub struct HcSet;

impl Step for HcSet {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("hc_set").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe("hc_set", step, step.get("hc_set").unwrap_or(&Value::Null))
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let spec = object(step, "hc_set");
        let key = ["address", "name"]
            .iter()
            .map(|key| spec.get(*key).map(python_str).unwrap_or_default())
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        swarm_c::with_hc_lock(|| {
            let mut state = swarm_c::hc_state()?;
            let terminal = state
                .get_mut("terminals")
                .and_then(Value::as_array_mut)
                .and_then(|terminals| {
                    terminals.iter_mut().find(|item| {
                        python_str(item.get("address").unwrap_or(&Value::Null)) == key
                            || python_str(item.get("name").unwrap_or(&Value::Null)) == key
                    })
                })
                .ok_or_else(|| format!("no hay terminal {key:?} en el estado de fake-hc"))?;
            let new_state = spec
                .get("state")
                .map(python_str)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "idle".to_string());
            terminal["state"] = Value::String(new_state);
            let conclusion = spec.get("conclusion").map(python_str).unwrap_or_default();
            if !conclusion.is_empty() {
                terminal["conclusion"] = Value::String(conclusion.clone());
                let screen = python_str(terminal.get("screen").unwrap_or(&Value::Null));
                if !screen.contains(&format!("● {conclusion}")) {
                    terminal["screen"] = Value::String(format!("{screen}\n● {conclusion}"));
                }
            }
            swarm_c::hc_save(&state)
        })?;
        Ok(ready())
    }
}

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
            "chat_mute" => events.put(AppEvent::ChatMute(
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

/// `wait_drained: 40`.
pub struct WaitDrained;

impl Step for WaitDrained {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_drained")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_drained")
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let timeout = step
            .get("wait_drained")
            .and_then(Value::as_f64)
            .unwrap_or(20.0);
        let app = driver.app.clone();
        let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
        let mut since: Option<Instant> = None;
        Ok(Some(Pending::Check(Box::new(move || {
            let now = Instant::now();
            if quiet(&app) {
                let began = *since.get_or_insert(now);
                if now.duration_since(began).as_secs_f64() >= 0.3 {
                    return Ok(true);
                }
                return Ok(false);
            }
            since = None;
            if now > deadline {
                return Err(format!("la app no quedó quieta en {timeout:.0} s"));
            }
            Ok(false)
        }))))
    }
}

fn quiet(app: &VoiceApp) -> bool {
    let state = app.state();
    app.events().is_empty()
        && app.menu_events().is_empty()
        && app.inner.agents.announce_queue.lock().is_empty()
        && state != "thinking"
        && state != "speaking"
        && !busy_workers(app)
}

fn busy_workers(app: &VoiceApp) -> bool {
    const BUSY: [&str; 6] = [
        "ezagent-speech",
        "ezagent-answer",
        "ezagent-chat",
        "ezagent-tts",
        "ezagent-stt",
        "ezagent-swarm-launch",
    ];
    let workers = app.inner.runtime.workers.lock().iter().any(|handle| {
        !handle.is_finished()
            && handle
                .thread()
                .name()
                .is_some_and(|name| BUSY.iter().any(|prefix| name.starts_with(prefix)))
    });
    let launching = app
        .inner
        .agents
        .swarm
        .lock()
        .as_ref()
        .is_some_and(|stack| stack.supervisor.launching());
    workers || launching
}
