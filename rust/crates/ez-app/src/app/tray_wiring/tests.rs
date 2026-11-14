//! Pruebas de la bandeja, el menú, los archivos y el reinicio (F6-02).

use super::*;
use std::sync::atomic::AtomicBool;

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use parking_lot::Mutex;
use serde_json::json;

use crate::app::core::{AppHooks, AppParts, SpeakerApi};

#[derive(Default)]
struct FakeSpeaker;

impl SpeakerApi for FakeSpeaker {
    fn synthesize(&self, _text: &str) -> Result<(Vec<f32>, u32), String> {
        Ok((vec![0.0], 24_000))
    }

    fn play(
        &self,
        _samples: &[f32],
        _sample_rate: u32,
        _stop: &AtomicBool,
        _on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }

    fn speak(
        &self,
        _text: &str,
        _stop: &AtomicBool,
        _on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
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
}

struct Fixture {
    app: VoiceApp,
    hooks: Arc<RecordingHooks>,
    root: PathBuf,
    config_file: PathBuf,
    tasks_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let unique = format!(
            "ezagent-f6-02-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("carpeta temporal");
        let config_file = root.join("voice.json");
        let tasks_path = root.join("tasks.json");
        let mut cfg = VoiceConfig::new();
        cfg.set("hud_hidden", json!(true));
        cfg.set("smart_turn", json!(false));
        cfg.set("chat_voice", json!(false));
        let speaker: Arc<dyn SpeakerApi> = Arc::new(FakeSpeaker);
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        let mut parts = AppParts::new(speaker, mic);
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(cfg, Some(tasks_path.clone()), parts);
        let hooks = Arc::new(RecordingHooks::default());
        app.set_hooks(hooks.clone());
        Self {
            app,
            hooks,
            root,
            config_file,
            tasks_path,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.app.shutdown();
        let _ = fs::remove_file(&self.tasks_path);
        let _ = fs::remove_file(&self.config_file);
        let _ = fs::remove_file(self.root.join("permissions.toml"));
        let _ = fs::remove_dir(&self.root);
    }
}

fn config_value(fixture: &Fixture, key: &str) -> Option<Value> {
    let text = fs::read_to_string(&fixture.config_file).ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| value.get(key).cloned())
}

#[test]
fn tray_title_follows_the_python_order() {
    assert_eq!(tray_title(false, false, IDLE), "EzAgent · en espera");
    assert_eq!(tray_title(false, true, LISTENING), "EzAgent · en espera");
    assert_eq!(
        tray_title(true, true, LISTENING),
        "EzAgent · micrófono silenciado"
    );
    assert_eq!(tray_title(true, false, LISTENING), "EzAgent · escuchando");
    assert_eq!(
        tray_title(true, false, "thinking"),
        "EzAgent · en conversación"
    );
}

#[test]
fn create_tray_installs_the_icon_and_shutdown_removes_it() {
    let fixture = Fixture::new();
    assert!(fixture.app.tray().is_none());
    let tray = fixture.app.create_tray();
    assert!(tray.is_some());
    assert!(fixture.app.tray().is_some());
    fixture.app.update_tray_title();
    fixture.app.shutdown();
    assert!(fixture.app.tray().is_none());
}

#[test]
fn tray_action_config_unhides_and_saves_the_bar() {
    let fixture = Fixture::new();
    assert!(fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    fixture.app.tray_action_ui("config");
    assert!(!fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert_eq!(config_value(&fixture, "hud_hidden"), Some(json!(false)));
}

#[test]
fn tray_action_shape_toggles_the_layout() {
    let fixture = Fixture::new();
    let before = fixture.app.hud().layout().to_string();
    fixture.app.tray_action_ui("shape");
    let after = fixture.app.hud().layout().to_string();
    assert_ne!(before, after);
    fixture.app.tray_action_ui("shape");
    assert_eq!(fixture.app.hud().layout(), before);
}

#[test]
fn tray_action_quit_stops_the_ui_loop() {
    let fixture = Fixture::new();
    fixture.app.tray_action_ui("quit");
    assert!(fixture.app.inner.ui.ui.run().is_ok());
}

#[test]
fn menu_repeat_last_queues_the_hotkey_and_unknown_actions_are_ignored() {
    let fixture = Fixture::new();
    fixture.app.menu_action_ui("repeat_last");
    assert_eq!(
        fixture.app.hotkey_events().get_nowait().as_deref(),
        Some("repeat")
    );
    fixture.app.menu_action_ui("cualquier_cosa");
    assert!(fixture.app.hotkey_events().is_empty());
}

#[test]
fn new_conversation_clears_the_turn_and_queues_the_answer() {
    let fixture = Fixture::new();
    *fixture.app.inner.session.last_summary.write() = "resumen viejo".to_string();
    *fixture.app.inner.speech.speaking_kind.write() = Some("answer".to_string());
    let generation = fixture.app.inner.turn.turn_gen.load(Ordering::SeqCst);
    fixture.app.new_conversation();
    assert_eq!(fixture.app.last_summary(), "");
    assert!(fixture.app.inner.speech.speaking_kind.read().is_none());
    assert_eq!(
        fixture.app.inner.turn.turn_gen.load(Ordering::SeqCst),
        generation + 1
    );
    let events: Vec<AppEvent> = std::iter::from_fn(|| fixture.app.events().get_nowait()).collect();
    assert_eq!(
        events,
        vec![AppEvent::Say("Listo, empezamos de cero.".to_string())]
    );
}

#[test]
fn resume_listening_unmutes_and_starts_or_resumes_the_call() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .session
        .muted
        .store(true, Ordering::SeqCst);
    fixture
        .app
        .inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    fixture.app.resume_listening_from_mic();
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert!(!fixture
        .app
        .inner
        .session
        .ignore_utterance
        .load(Ordering::SeqCst));
    assert!(fixture.app.inner.audio.reset_capture.is_set());
    assert_eq!(
        fixture.hooks.calls.lock().as_slice(),
        &["start_call".to_string()]
    );

    fixture
        .app
        .inner
        .session
        .in_call
        .store(true, Ordering::SeqCst);
    fixture.app.inner.audio.reset_capture.clear();
    fixture.app.resume_listening_from_mic();
    assert_eq!(fixture.app.state(), LISTENING);
    assert!(!fixture.app.inner.audio.reset_capture.is_set());
    assert!(!fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
}

#[test]
fn app_dir_uses_the_config_file_without_an_agent_runtime() {
    let fixture = Fixture::new();
    assert_eq!(fixture.app.app_dir(), fixture.root);
}

#[test]
fn missing_permissions_file_is_not_created() {
    let fixture = Fixture::new();
    fixture.app.open_permissions();
    assert!(!fixture.root.join("permissions.toml").exists());
}

#[test]
fn system_md_is_created_from_the_default_template_once() {
    let fixture = Fixture::new();
    let path = fixture.root.join("system.md");
    ensure_system_md(&path).expect("crear system.md");
    let text = fs::read_to_string(&path).expect("leer system.md");
    assert!(text.starts_with("# Identidad"));
    assert!(text.contains("Eres EzAgent"));
    fs::write(&path, "personalizado").expect("sobrescribir");
    ensure_system_md(&path).expect("no debe sobrescribir");
    assert_eq!(fs::read_to_string(&path).expect("leer"), "personalizado");
    let _ = fs::remove_file(&path);
}

#[test]
fn voice_json_is_created_empty() {
    let fixture = Fixture::new();
    let path = fixture.root.join("nuevo.json");
    ensure_voice_json(&path).expect("crear voice.json");
    let bytes = fs::read(&path).expect("leer voice.json");
    let expected = if cfg!(windows) {
        b"{}\r\n".as_slice()
    } else {
        b"{}\n".as_slice()
    };
    assert_eq!(bytes, expected);
    let _ = fs::remove_file(&path);
}

#[test]
fn restart_target_is_the_running_executable_and_its_folder() {
    let (program, _args, cwd) = restart_target().expect("ejecutable actual");
    assert!(program.is_file());
    assert_eq!(cwd, program.parent().expect("carpeta").to_path_buf());
}
