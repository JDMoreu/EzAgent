use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use parking_lot::Mutex;
use serde_json::{json, Value};

use super::*;
use crate::app::core::{AppHooks, AppParts, SpeakerApi};

struct SilentSpeaker;

impl SpeakerApi for SilentSpeaker {
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
        _text: &str,
        _stop: &AtomicBool,
        _on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

#[derive(Default)]
struct RecordingHooks {
    calls: Mutex<Vec<String>>,
}

impl AppHooks for RecordingHooks {
    fn start_call(&self) {
        self.calls.lock().push("start_call".to_string());
    }

    fn update_tray_title(&self) {
        self.calls.lock().push("update_tray_title".to_string());
    }
}

#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl Recorder {
    fn all(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl SettingsEffects for Recorder {
    fn speak(&self, text: &str, kind: &str) {
        self.calls.lock().push(format!("speak:{kind}:{text}"));
    }

    fn set_state(&self, state: &str) {
        self.calls.lock().push(format!("set_state:{state}"));
    }

    fn set_mic_muted(&self, muted: bool, source: &str) {
        self.calls.lock().push(format!("mic:{muted}:{source}"));
    }

    fn resume_listening_from_mic(&self) {
        self.calls.lock().push("resume_listening".to_string());
    }

    fn toggle_chat(&self) {
        self.calls.lock().push("toggle_chat".to_string());
    }

    fn handle_hotkey(&self, event: &str) {
        self.calls.lock().push(format!("hotkey:{event}"));
    }

    fn refresh_activity(&self) {
        self.calls.lock().push("refresh_activity".to_string());
    }
}

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-f6-06a-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

struct Fixture {
    app: VoiceApp,
    hooks: Arc<RecordingHooks>,
    tasks_path: PathBuf,
    config_file: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let mut cfg = VoiceConfig::new();
        // Oculto evita crear la ventana Win32 durante la prueba.
        cfg.set("hud_hidden", Value::Bool(true));
        cfg.set("smart_turn", Value::Bool(false));
        cfg.set("wake_phrase", json!("oye ez"));
        let speaker: Arc<dyn SpeakerApi> = Arc::new(SilentSpeaker);
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        let tasks_path = temp_path("tasks.json");
        let config_file = temp_path("voice.json");
        let mut parts = AppParts::new(speaker, mic);
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(cfg, Some(tasks_path.clone()), parts);
        let hooks = Arc::new(RecordingHooks::default());
        app.set_hooks(hooks.clone());
        Self {
            app,
            hooks,
            tasks_path,
            config_file,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tasks_path);
        let _ = std::fs::remove_file(&self.config_file);
    }
}

#[test]
fn normalize_es_la_de_fast() {
    assert_eq!(VoiceApp::normalize("Mutéate 🙂, ¡ya!"), "muteate ya");
}

#[test]
fn fast_mute_and_unmute_use_the_voice_source() {
    let fixture = Fixture::new();
    let recorder = Recorder::default();
    assert!(fixture.app.fast_command("mutéate"));
    assert!(fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert!(fixture.app.fast_command("desmutea"));
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));

    assert!(fixture.app.fast_command_with("mutéate", &recorder));
    let calls = recorder.all();
    assert!(calls.contains(&"mic:true:voz".to_string()), "{calls:?}");
    assert!(
        calls.contains(&"speak:answer:En pausa. Di «oye ez» cuando me necesites.".to_string()),
        "{calls:?}"
    );
}

#[test]
fn fast_close_respects_close_action_and_sets_mode() {
    let fixture = Fixture::new();
    assert!(fixture.app.fast_command("cierra la escucha"));
    assert!(fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
    assert_eq!(*fixture.app.inner.turn.close_mode.read(), "pause");

    fixture.app.set_config(with_close_action("HANGUP"));
    fixture
        .app
        .inner
        .turn
        .close_after_answer
        .store(false, Ordering::SeqCst);
    assert!(fixture.app.fast_command("pausa"));
    assert!(fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
    assert_eq!(*fixture.app.inner.turn.close_mode.read(), "hangup");
}

fn with_close_action(value: &str) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", Value::Bool(false));
    cfg.set("close_action", json!(value));
    cfg
}

#[test]
fn fast_repeat_speaks_the_summary_or_its_fallback() {
    let fixture = Fixture::new();
    *fixture.app.inner.session.last_summary.write() = "Resumen anterior.".to_string();
    let recorder = Recorder::default();
    assert!(fixture.app.fast_command_with("repítelo", &recorder));
    assert_eq!(
        recorder.all(),
        vec!["speak:answer:Resumen anterior.".to_string()]
    );

    *fixture.app.inner.session.last_summary.write() = String::new();
    let recorder = Recorder::default();
    assert!(fixture.app.fast_command_with("otra vez", &recorder));
    assert_eq!(
        recorder.all(),
        vec!["speak:answer:No hay nada que repetir todavía.".to_string()]
    );
}

#[test]
fn fast_ignores_texts_that_are_not_commands() {
    let fixture = Fixture::new();
    let recorder = Recorder::default();
    for text in ["silencio", "haz algo", ""] {
        assert!(!fixture.app.fast_command_with(text, &recorder), "{text}");
    }
    assert!(recorder.all().is_empty());
}

#[test]
fn fast_tasks_lists_tasks_and_reveals_activity() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .agents
        .tasks
        .add("t1", "codex", "revisar los tests", None)
        .expect("tarea temporal");
    let recorder = Recorder::default();
    assert!(fixture
        .app
        .fast_command_with("qué tareas pendientes", &recorder));
    let calls = recorder.all();
    assert!(calls.contains(&"refresh_activity".to_string()), "{calls:?}");
    assert!(
        calls
            .iter()
            .any(|call| call
                == "speak:answer:Tengo 1 tareas pendientes: Codex: revisar los tests."),
        "{calls:?}"
    );
}

#[test]
fn hud_action_publishes_the_intent_without_touching_ui() {
    let fixture = Fixture::new();
    let _ = fixture.app.events().get_nowait();
    fixture.app.on_hud_action("panel");
    assert_eq!(
        fixture.app.events().get_nowait(),
        Some(AppEvent::HudAction("panel".to_string()))
    );
}

#[test]
fn hud_action_mute_toggles_the_microphone() {
    let fixture = Fixture::new();
    fixture.app.hud_action_ui("mute");
    assert!(fixture.app.inner.session.muted.load(Ordering::SeqCst));
    fixture.app.hud_action_ui("mute");
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));
}

#[test]
fn hud_action_listen_unmutes_and_starts_a_call() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .session
        .muted
        .store(true, Ordering::SeqCst);
    fixture.app.hud_action_ui("listen");
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert!(fixture
        .hooks
        .calls
        .lock()
        .iter()
        .any(|call| call == "start_call"));
}

#[test]
fn hud_action_hide_hides_the_bar_and_persists_the_flag() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .config
        .hud_hidden
        .store(false, Ordering::SeqCst);
    fixture.app.hud_action_ui("hide");
    assert!(fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.config_file).expect("voice.json"))
            .expect("json");
    assert_eq!(saved.get("hud_hidden"), Some(&Value::Bool(true)));
}

#[test]
fn hide_hud_updates_the_in_memory_config() {
    let fixture = Fixture::new();
    fixture.app.hide_hud(false);
    assert_eq!(
        fixture.app.config().get("hud_hidden"),
        Some(&Value::Bool(true))
    );
}

#[test]
fn toggle_menu_without_provider_is_a_no_op() {
    let fixture = Fixture::new();
    fixture.app.toggle_menu();
    assert!(!fixture.app.hud().menu_open());
}

#[test]
fn refresh_devices_fills_the_cache() {
    let fixture = Fixture::new();
    let expected = ez_audio::capture::list_input_devices().len();
    fixture.app.refresh_devices();
    assert_eq!(
        fixture.app.inner.config.devices_cache.lock().len(),
        expected
    );
}
