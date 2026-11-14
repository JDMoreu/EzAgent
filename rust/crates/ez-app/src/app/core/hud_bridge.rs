//! Adaptadores de `DropUI` a la configuración y a las colas de la app (F6-01).
//!
//! En Python `DropUI` lee y escribe `VoiceConfig` directamente y publica sus
//! intenciones con lambdas a `events`/`menu_events`. Aquí son [`DropConfig`] y
//! [`DropHost`], los dos únicos puntos por donde el HUD toca la app.

use std::path::PathBuf;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_hud::model::settings::SettingsValues;
use ez_hud::win::drop::{ConfigValue, DropConfig, DropHost};
use parking_lot::RwLock;
use serde_json::Value;

use super::events::{AppEvent, MenuEvent, Queue};
use super::log;

/// `ConfigValue` del HUD → JSON de `VoiceConfig`.
pub fn config_to_json(value: &ConfigValue) -> Value {
    match value {
        ConfigValue::Bool(flag) => Value::Bool(*flag),
        ConfigValue::Int(number) => Value::from(*number),
        ConfigValue::Float(number) => Value::from(*number),
        ConfigValue::Text(text) => Value::String(text.clone()),
    }
}

/// JSON de `VoiceConfig` → `ConfigValue`; los objetos y listas no son ajustes.
pub fn json_to_config(value: &Value) -> Option<ConfigValue> {
    match value {
        Value::Bool(flag) => Some(ConfigValue::Bool(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => Some(ConfigValue::Int(integer)),
            None => number.as_f64().map(ConfigValue::Float),
        },
        Value::String(text) => Some(ConfigValue::Text(text.clone())),
        _ => None,
    }
}

/// `cfg`/`cfg.save_key` vistos por el HUD.
pub struct HudConfig {
    pub cfg: Arc<RwLock<VoiceConfig>>,
    pub config_file: PathBuf,
}

impl DropConfig for HudConfig {
    fn value(&self, key: &str) -> Option<ConfigValue> {
        let cfg = self.cfg.read();
        cfg.get(key).and_then(json_to_config)
    }

    fn save(&self, key: &str, value: ConfigValue) {
        let json = config_to_json(&value);
        let mut cfg = self.cfg.write();
        cfg.set(key, json.clone());
        if let Err(error) = cfg.save_key(key, json, Some(&self.config_file)) {
            // El Python envuelve estas escrituras en `except Exception: pass`.
            tracing::debug!("No pude guardar {key}: {error}");
        }
    }

    fn remember(&self, key: &str, value: ConfigValue) {
        self.cfg.write().set(key, config_to_json(&value));
    }
}

/// Callbacks de `DropUI` (`on_action`, `on_chat_*`, `on_activity_*`…).
pub struct HudHost {
    pub events: Arc<Queue<AppEvent>>,
    pub menu_events: Arc<Queue<MenuEvent>>,
}

impl DropHost for HudHost {
    fn action(&self, action: &str) {
        // `_on_hud_action`: log y la intención al hilo principal.
        log(format!("HUD: {action}"));
        self.events.put(AppEvent::HudAction(action.to_string()));
    }

    fn live_setting(&self, key: &str, value: ConfigValue) {
        self.menu_events
            .put(MenuEvent::Live(key.to_string(), value));
    }

    fn save_settings(&self, values: &SettingsValues) -> Option<bool> {
        // `_hud_save_settings`: el guardado real ocurre en `_tick`; `None` = pendiente.
        self.menu_events.put(MenuEvent::Save(values.clone()));
        None
    }

    fn test_voice(&self, values: &SettingsValues) {
        self.menu_events.put(MenuEvent::Test(values.clone()));
    }

    fn menu_action(&self, action: &str) {
        self.events.put(AppEvent::MenuAction(action.to_string()));
    }

    fn hotkey_capture(&self, active: bool) {
        self.events.put(AppEvent::HotkeyCapture(active));
    }

    fn activity_open(&self, key: &str) {
        self.events.put(AppEvent::ActivityOpen(key.to_string()));
    }

    fn activity_dismiss(&self, key: &str) {
        self.events.put(AppEvent::ActivityDismiss(key.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ez_core::config::config_path;
    use serde_json::json;

    fn config() -> Arc<RwLock<VoiceConfig>> {
        Arc::new(RwLock::new(VoiceConfig::new()))
    }

    #[test]
    fn converts_scalars_and_ignores_collections() {
        assert_eq!(json_to_config(&json!(true)), Some(ConfigValue::Bool(true)));
        assert_eq!(json_to_config(&json!(7)), Some(ConfigValue::Int(7)));
        assert_eq!(json_to_config(&json!(0.5)), Some(ConfigValue::Float(0.5)));
        assert_eq!(
            json_to_config(&json!("hola")),
            Some(ConfigValue::Text("hola".to_string()))
        );
        assert_eq!(json_to_config(&json!([1, 2])), None);
        assert_eq!(config_to_json(&ConfigValue::Int(-2)), json!(-2));
    }

    #[test]
    fn reads_in_memory_config_and_remembers_values() {
        let hud = HudConfig {
            cfg: config(),
            config_file: config_path(),
        };
        assert_eq!(
            hud.value("hotkey"),
            Some(ConfigValue::Text("ctrl+alt+space".to_string()))
        );
        assert_eq!(hud.value("ui_scale"), Some(ConfigValue::Float(1.0)));
        assert_eq!(hud.value("no_such_key"), None);
        hud.remember("ui_scale", ConfigValue::Float(1.25));
        assert_eq!(hud.value("ui_scale"), Some(ConfigValue::Float(1.25)));
    }

    #[test]
    fn host_publishes_every_callback() {
        let events = Queue::new(None);
        let menu = Queue::new(None);
        let host = HudHost {
            events: Arc::clone(&events),
            menu_events: Arc::clone(&menu),
        };
        host.action("mute");
        host.menu_action("panel");
        host.hotkey_capture(true);
        host.activity_open("a1");
        host.activity_dismiss("a1");
        host.live_setting("ui_scale", ConfigValue::Float(1.1));
        assert_eq!(host.save_settings(&SettingsValues::new()), None);
        host.test_voice(&SettingsValues::new());
        let collected: Vec<AppEvent> = std::iter::from_fn(|| events.get_nowait()).collect();
        assert_eq!(
            collected,
            vec![
                AppEvent::HudAction("mute".to_string()),
                AppEvent::MenuAction("panel".to_string()),
                AppEvent::HotkeyCapture(true),
                AppEvent::ActivityOpen("a1".to_string()),
                AppEvent::ActivityDismiss("a1".to_string()),
            ]
        );
        let menu_events: Vec<MenuEvent> = std::iter::from_fn(|| menu.get_nowait()).collect();
        assert_eq!(
            menu_events,
            vec![
                MenuEvent::Live("ui_scale".to_string(), ConfigValue::Float(1.1)),
                MenuEvent::Save(SettingsValues::new()),
                MenuEvent::Test(SettingsValues::new()),
            ]
        );
    }
}
