mod mic_lock;
mod more;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ez_audio::capture::{InputDevice, MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_hud::model::settings::SettingValue;
use parking_lot::Mutex;
use serde_json::{json, Value};

use super::menu::py_title;
use super::*;
use crate::app::core::{AppParts, SpeakerApi};

#[derive(Default)]
struct SpeakerLog {
    voices: Mutex<Vec<String>>,
    speaks: Mutex<Vec<String>>,
    reloads: Mutex<u32>,
}

struct TestSpeaker {
    log: Arc<SpeakerLog>,
}

impl SpeakerApi for TestSpeaker {
    fn synthesize(&self, _text: &str) -> Result<(Vec<f32>, u32), String> {
        Ok((Vec::new(), 24_000))
    }

    fn play(
        &self,
        _samples: &[f32],
        _sample_rate: u32,
        _stop: &AtomicBool,
        _on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(false)
    }

    fn speak(
        &self,
        text: &str,
        _stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        self.log.speaks.lock().push(text.to_string());
        if let Some(on_level) = on_level {
            on_level(0.5);
        }
        Ok(false)
    }

    fn set_voice(&self, voice: &str) -> Result<(), String> {
        self.log.voices.lock().push(voice.to_string());
        Ok(())
    }

    fn reload_model(&self) -> Result<(), String> {
        *self.log.reloads.lock() += 1;
        Ok(())
    }
}

#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
    voices: Vec<String>,
}

impl Recorder {
    fn with_voices(voices: &[&str]) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            voices: voices.iter().map(|voice| (*voice).to_string()).collect(),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl SettingsEffects for Recorder {
    fn set_state(&self, state: &str) {
        self.calls.lock().push(format!("state:{state}"));
    }

    fn set_mic_muted(&self, muted: bool, source: &str) {
        self.calls.lock().push(format!("mic:{muted}:{source}"));
    }
}

impl MenuEffects for Recorder {
    fn hang_up(&self) {
        self.calls.lock().push("hang_up".to_string());
    }

    fn kokoro_voices(&self) -> Vec<String> {
        self.voices.clone()
    }
}

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-f6-06b-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

struct Fixture {
    app: VoiceApp,
    log: Arc<SpeakerLog>,
    tasks_path: PathBuf,
    config_file: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let mut cfg = VoiceConfig::new();
        cfg.set("hud_hidden", Value::Bool(true));
        cfg.set("smart_turn", Value::Bool(false));
        cfg.set("llm_base_url", json!(""));
        cfg.set("llm_api_style", json!("openai"));
        let log = Arc::new(SpeakerLog::default());
        let speaker: Arc<dyn SpeakerApi> = Arc::new(TestSpeaker {
            log: Arc::clone(&log),
        });
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        let tasks_path = temp_path("tasks.json");
        let config_file = temp_path("voice.json");
        let mut parts = AppParts::new(speaker, mic);
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(cfg, Some(tasks_path.clone()), parts);
        *app.inner.config.devices_cache.lock() = vec![InputDevice {
            index: 0,
            name: "mic de prueba".to_string(),
            rate: 16_000,
            channels: 1,
        }];
        app.inner.audio.mic.lock().set_device_index(None);
        Self {
            app,
            log,
            tasks_path,
            config_file,
        }
    }

    fn values(&self, pairs: &[(&str, SettingValue)]) -> SettingsValues {
        let mut values = SettingsValues::new();
        for (key, value) in pairs {
            values.insert(*key, value.clone());
        }
        values
    }

    fn config_json(&self) -> Value {
        let text = std::fs::read_to_string(&self.config_file).expect("voice.json escrito");
        serde_json::from_str(&text).expect("json válido")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tasks_path);
        let _ = std::fs::remove_file(&self.config_file);
    }
}

#[test]
fn title_matches_python() {
    assert_eq!(py_title("opencode"), "Opencode");
    assert_eq!(py_title("mi proveedor"), "Mi Proveedor");
    assert_eq!(py_title("a1-b2"), "A1-B2");
}

#[test]
fn snapshot_uses_mic_llm_profile_and_providers() {
    let fixture = Fixture::new();
    let settings = fixture.app.hud_menu_settings().expect("menú disponible");
    assert_eq!(
        settings.value("input_device_index").expect("valor"),
        SettingValue::None
    );
    assert_eq!(
        settings.value("llm_profile").expect("valor"),
        SettingValue::Text("opencode".to_string())
    );
    assert_eq!(
        settings.value("speed").expect("valor"),
        SettingValue::Float(0.95)
    );
    assert!(settings.provider("opencode").is_some());
    assert!(fixture.app.inner.config.menu_saved.lock().is_some());
}

#[test]
fn snapshot_restores_the_saved_copy() {
    let fixture = Fixture::new();
    let first = fixture.app.hud_menu_settings().expect("primer menú");
    assert!(!first.dirty());
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("speed", Value::from(1.2));
    let second = fixture.app.hud_menu_settings().expect("segundo menú");
    assert!(second.dirty());
}

#[test]
fn menu_callbacks_only_publish_intentions() {
    let fixture = Fixture::new();
    let values = fixture.values(&[("speed", SettingValue::Float(1.1))]);
    fixture
        .app
        .hud_live_setting("ui_scale", ConfigValue::Float(1.2));
    fixture.app.hud_save_settings(&values);
    fixture.app.hud_test_voice(&values);
    fixture.app.hud_hide();
    let events: Vec<MenuEvent> =
        std::iter::from_fn(|| fixture.app.menu_events().get_nowait()).collect();
    assert_eq!(
        events,
        vec![
            MenuEvent::Live("ui_scale".to_string(), ConfigValue::Float(1.2)),
            MenuEvent::Save(values.clone()),
            MenuEvent::Test(values),
            MenuEvent::Hide,
        ]
    );
}

#[test]
fn custom_theme_applies_a_preset_without_persisting() {
    let fixture = Fixture::new();
    fixture
        .app
        .apply_custom_theme("theme_preset", &ConfigValue::Text("marea".to_string()));
    let cfg = fixture.app.config();
    assert_eq!(cfg.get("theme").and_then(Value::as_str), Some("custom"));
    assert_eq!(
        cfg.get("theme_background").and_then(Value::as_str),
        Some("#0E6F7C")
    );
    assert_eq!(
        cfg.get("theme_primary").and_then(Value::as_str),
        Some("#1FAE9F")
    );
    assert_eq!(
        cfg.get("theme_secondary").and_then(Value::as_str),
        Some("#8FE3C0")
    );
    assert_eq!(
        cfg.get("theme_accent").and_then(Value::as_str),
        Some("#FF8A65")
    );
    assert!(!fixture.config_file.exists());
}

#[test]
fn invalid_theme_preset_is_ignored() {
    let fixture = Fixture::new();
    fixture
        .app
        .apply_custom_theme("theme_preset", &ConfigValue::Text("no-existe".to_string()));
    assert_ne!(
        fixture.app.config().get("theme").and_then(Value::as_str),
        Some("custom")
    );
}

#[test]
fn instant_setting_normalizes_applies_and_persists() {
    let fixture = Fixture::new();
    fixture.app.apply_instant_setting(
        "ui_scale",
        &ConfigValue::Text("1.2".to_string()),
        true,
        true,
    );
    let cfg = fixture.app.config();
    assert_eq!(cfg.get("ui_scale").and_then(Value::as_f64), Some(1.2));
    assert!((fixture.app.hud().bar_scale() - 1.2).abs() < 1e-9);
    assert_eq!(
        fixture
            .config_json()
            .get("ui_scale")
            .and_then(Value::as_f64),
        Some(1.2)
    );
    fixture
        .app
        .apply_instant_setting("chat_voice", &ConfigValue::Bool(true), true, true);
    assert!(fixture.app.inner.chat.chat.voice_replies());
}

#[test]
fn live_speed_changes_memory_only() {
    let fixture = Fixture::new();
    fixture
        .app
        .apply_live_setting("speed", &ConfigValue::Float(1.3));
    assert_eq!(
        fixture.app.config().get("speed").and_then(Value::as_f64),
        Some(1.3)
    );
    assert!(!fixture.config_file.exists());
}

#[test]
fn appearance_changes_are_applied_but_not_written_until_save() {
    let fixture = Fixture::new();
    fixture
        .app
        .apply_instant_setting("ui_scale", &ConfigValue::Float(1.4), false, false);
    assert_eq!(
        fixture.app.config().get("ui_scale").and_then(Value::as_f64),
        Some(1.4)
    );
    assert!((fixture.app.hud().bar_scale() - 1.4).abs() < 1e-9);
    assert!(!fixture.config_file.exists());
    // Quien lo aplica desde fuera del menú sigue escribiendo en el acto.
    fixture
        .app
        .apply_instant_setting("ui_scale", &ConfigValue::Float(1.2), true, true);
    assert_eq!(
        fixture
            .config_json()
            .get("ui_scale")
            .and_then(Value::as_f64),
        Some(1.2)
    );
}

#[test]
fn live_dictionary_changes_reach_the_transcription_configuration() {
    let fixture = Fixture::new();
    fixture.app.apply_live_setting(
        "stt_dictionary",
        &ConfigValue::Text("open code → OpenCode".to_string()),
    );
    assert_eq!(
        fixture
            .app
            .config()
            .get("stt_dictionary")
            .and_then(Value::as_str),
        Some("open code → OpenCode")
    );
}

#[test]
fn live_voice_changes_the_speaker() {
    let fixture = Fixture::new();
    fixture
        .app
        .apply_live_setting("voice", &ConfigValue::Text("ef_dora".to_string()));
    assert_eq!(
        fixture.app.config().get("voice").and_then(Value::as_str),
        Some("ef_dora")
    );
    assert_eq!(fixture.log.voices.lock().as_slice(), ["ef_dora"]);
}

#[test]
fn live_unknown_profile_is_ignored() {
    let fixture = Fixture::new();
    let before = fixture.app.config();
    fixture.app.apply_live_setting(
        "llm_profile",
        &ConfigValue::Text("perfil-inexistente".to_string()),
    );
    assert_eq!(
        fixture.app.config().get("llm_model"),
        before.get("llm_model")
    );
}

#[test]
fn save_from_menu_applies_and_writes_voice_json() {
    let fixture = Fixture::new();
    let values = fixture.values(&[
        ("speed", SettingValue::Float(1.1)),
        ("stt_model", SettingValue::Text("tiny".to_string())),
        ("confirm_actions", SettingValue::Bool(true)),
    ]);
    fixture.app.save_from_menu(&values);
    let cfg = fixture.app.config();
    assert_eq!(cfg.get("speed").and_then(Value::as_f64), Some(1.1));
    assert_eq!(cfg.get("stt_model").and_then(Value::as_str), Some("tiny"));
    assert_eq!(
        cfg.get("confirm_actions").and_then(Value::as_bool),
        Some(true)
    );
    let saved = fixture.config_json();
    assert_eq!(saved.get("speed").and_then(Value::as_f64), Some(1.1));
    assert_eq!(saved.get("stt_model").and_then(Value::as_str), Some("tiny"));
    let menu_saved = fixture.app.inner.config.menu_saved.lock().clone();
    assert_eq!(
        menu_saved
            .and_then(|map| map.get("speed").cloned())
            .and_then(|value| value.as_f64()),
        Some(1.1)
    );
}

#[test]
fn save_failure_restores_the_configuration() {
    let fixture = Fixture::new();
    let before = fixture.app.config();
    let values = fixture.values(&[("llm_profile_name", SettingValue::Text("x".repeat(81)))]);
    let error = fixture
        .app
        .apply_settings(&values)
        .expect_err("nombre demasiado largo");
    assert_eq!(
        error,
        "El nombre de la configuración admite hasta 80 caracteres."
    );
    fixture.app.save_from_menu(&values);
    assert_eq!(fixture.app.config().to_value(), before.to_value());
    assert!(fixture.app.inner.config.menu_saved.lock().is_none());
    assert!(!fixture.config_file.exists());
}

#[test]
fn apply_settings_rejects_an_llm_without_endpoint_or_model() {
    let fixture = Fixture::new();
    let no_endpoint = fixture.values(&[("llm_model", SettingValue::Text("otro".to_string()))]);
    assert_eq!(
        fixture
            .app
            .apply_settings(&no_endpoint)
            .expect_err("sin URL"),
        "El proveedor LLM necesita un endpoint (URL)."
    );
    let no_model = fixture.values(&[
        (
            "llm_base_url",
            SettingValue::Text("http://127.0.0.1:9/v1".to_string()),
        ),
        ("llm_model", SettingValue::Text(String::new())),
    ]);
    assert_eq!(
        fixture
            .app
            .apply_settings(&no_model)
            .expect_err("sin modelo"),
        "El proveedor LLM necesita un modelo."
    );
}

#[test]
fn apply_settings_validates_hotkeys_before_mutating() {
    let fixture = Fixture::new();
    let invalid = fixture.values(&[("hotkey", SettingValue::Text("ctrl+alt+qq".to_string()))]);
    assert_eq!(
        fixture.app.apply_settings(&invalid).expect_err("tecla"),
        "Atajo hotkey: Tecla no reconocida en hotkey: 'qq'"
    );
    let duplicate = fixture.values(&[
        ("hotkey", SettingValue::Text("ctrl+alt+e".to_string())),
        (
            "cancel_hotkey",
            SettingValue::Text("CTRL+ALT+E".to_string()),
        ),
    ]);
    assert_eq!(
        fixture
            .app
            .apply_settings(&duplicate)
            .expect_err("duplicado"),
        "Los atajos hotkey y cancel_hotkey son el mismo."
    );
}

#[test]
fn restore_config_copies_every_field() {
    let fixture = Fixture::new();
    let snapshot = fixture.app.config();
    let mut target = fixture.app.config();
    target.set("speed", Value::from(2.0));
    target.set("llm_model", Value::String("otro".to_string()));
    VoiceApp::restore_config(&mut target, &snapshot);
    assert_eq!(target.to_value(), snapshot.to_value());
}
