//! Adaptadores de `DropUI` a la configuración y a las colas de la app (F6-01).
//!
//! En Python `DropUI` lee y escribe `VoiceConfig` directamente y publica sus
//! intenciones con lambdas a `events`/`menu_events`. Aquí son [`DropConfig`] y
//! [`DropHost`], los dos únicos puntos por donde el HUD toca la app.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
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

/// Un guardado pendiente: la clave y su último valor en esa ruta.
struct SaveJob {
    path: PathBuf,
    key: String,
    value: Value,
}

/// Destino de un guardado (el archivo `voice.json` en producción).
type Sink = Arc<dyn Fn(&Path, &str, Value) + Send + Sync>;

/// Cola del escritor: una escritura a disco nunca ocurre en el hilo que llama.
pub struct SaveQueue {
    jobs: Sender<SaveJob>,
}

impl SaveQueue {
    /// Arranca el hilo `ezagent-config-save` con este destino.
    pub fn start(sink: Sink) -> Self {
        let (jobs, queue) = unbounded();
        let spawned = thread::Builder::new()
            .name("ezagent-config-save".to_string())
            .spawn(move || drain(queue, sink));
        if let Err(error) = spawned {
            tracing::error!("No pude lanzar el hilo que guarda la configuración: {error}");
        }
        Self { jobs }
    }

    /// Encola `key = value`; las escrituras de una misma clave se funden en la
    /// última, sin perder el orden de llegada. No bloquea.
    pub fn push(&self, path: &Path, key: &str, value: Value) {
        let job = SaveJob {
            path: path.to_path_buf(),
            key: key.to_string(),
            value,
        };
        if self.jobs.send(job).is_err() {
            tracing::debug!("No pude encolar el guardado de {key}");
        }
    }
}

/// Atiende la cola por lotes: el último valor por clave, en orden de llegada.
fn drain(queue: Receiver<SaveJob>, sink: Sink) {
    while let Ok(first) = queue.recv() {
        let mut batch = vec![first];
        batch.extend(queue.try_iter());
        let mut pending: Vec<SaveJob> = Vec::new();
        for job in batch {
            match pending
                .iter_mut()
                .find(|old| old.path == job.path && old.key == job.key)
            {
                Some(old) => old.value = job.value,
                None => pending.push(job),
            }
        }
        for job in pending {
            sink(&job.path, &job.key, job.value);
        }
    }
}

/// `cfg.save_key(key, value)`: lee el archivo, cambia la clave y reescribe.
fn write_key(path: &Path, key: &str, value: Value) {
    let config = VoiceConfig::new();
    if let Err(error) = config.save_key(key, value, Some(path)) {
        // El Python envuelve estas escrituras en `except Exception: pass`.
        tracing::debug!("No pude guardar {key}: {error}");
    }
}

fn queue() -> &'static SaveQueue {
    static QUEUE: OnceLock<SaveQueue> = OnceLock::new();
    QUEUE.get_or_init(|| SaveQueue::start(Arc::new(write_key)))
}

/// `cfg`/`cfg.save_key` vistos por el HUD.
pub struct HudConfig {
    pub cfg: Arc<RwLock<VoiceConfig>>,
    pub config_file: PathBuf,
}

impl HudConfig {
    /// El valor en memoria (cerrojo corto) y el guardado a la cola.
    fn store(&self, key: &str, value: ConfigValue) {
        let json = config_to_json(&value);
        {
            let mut cfg = self.cfg.write();
            cfg.set(key, json.clone());
        }
        queue().push(&self.config_file, key, json);
    }
}

impl DropConfig for HudConfig {
    fn value(&self, key: &str) -> Option<ConfigValue> {
        let cfg = self.cfg.read();
        cfg.get(key).and_then(json_to_config)
    }

    fn save(&self, key: &str, value: ConfigValue) {
        self.store(key, value);
    }

    fn save_soon(&self, key: &str, value: ConfigValue) {
        self.store(key, value);
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

    #[test]
    fn a_slow_write_never_blocks_the_caller() {
        let (calls, received) = crossbeam_channel::unbounded();
        let slow = calls.clone();
        let queue = SaveQueue::start(Arc::new(move |path: &Path, key: &str, value: Value| {
            thread::sleep(std::time::Duration::from_secs(2));
            let _ = slow.send((path.to_path_buf(), key.to_string(), value));
        }));
        let started = std::time::Instant::now();
        queue.push(Path::new("voice.json"), "drop_x", Value::from(12));
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(300),
            "el guardado bloqueó {elapsed:?}"
        );
        let (_, key, value) = received
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("la escritura llegó");
        assert_eq!(key, "drop_x");
        assert_eq!(value, Value::from(12));
    }

    #[test]
    fn the_writer_keeps_the_last_value_per_key_in_order() {
        let (calls, received) = crossbeam_channel::unbounded();
        let sink = calls.clone();
        let queue = SaveQueue::start(Arc::new(move |_: &Path, key: &str, value: Value| {
            let _ = sink.send((key.to_string(), value));
        }));
        let path = Path::new("voice.json");
        for value in 0..100 {
            queue.push(path, "drop_x", Value::from(value));
        }
        queue.push(path, "drop_dock", Value::String("left".to_string()));
        let first = received
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("drop_x");
        let second = received
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("drop_dock");
        assert_eq!(first, ("drop_x".to_string(), Value::from(99)));
        assert_eq!(
            second,
            ("drop_dock".to_string(), Value::String("left".to_string()))
        );
        assert!(received
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
    }

    #[test]
    fn saving_the_hud_config_persists_other_keys() {
        let file = std::env::temp_dir().join(format!(
            "ezagent-hud-{}-{:?}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::write(&file, "{\n  \"ajena\": 7\n}\n").expect("archivo previo");
        let hud = HudConfig {
            cfg: config(),
            config_file: file.clone(),
        };
        hud.save("ui_scale", ConfigValue::Float(1.25));
        assert_eq!(hud.value("ui_scale"), Some(ConfigValue::Float(1.25)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let written = loop {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            if text.contains("ui_scale") || std::time::Instant::now() > deadline {
                break text;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(written.contains("\"ui_scale\": 1.25"), "{written}");
        assert!(written.contains("\"ajena\": 7"), "{written}");
        let _ = std::fs::remove_file(&file);
    }
}
