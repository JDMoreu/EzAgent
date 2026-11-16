//! Pruebas del núcleo de `VoiceApp` (F6-01).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_core::metrics::TurnTimings;
use ez_hud::win::drop::DropConfig;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};

use super::hud::AppHud;
use super::hud_bridge::HudConfig;
use super::services::{AppParts, SpeakerApi};
use super::state::Inner;
use super::*;
use crate::app::core::hooks::{AppHooks, ShutdownObserver};
use crate::app::startup::check_hotkey_duplicates;

#[derive(Default)]
struct FakeSpeaker {
    calls: Mutex<Vec<String>>,
}

impl SpeakerApi for FakeSpeaker {
    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String> {
        self.calls.lock().push(format!("synthesize:{text}"));
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
        self.calls.lock().push("play".to_string());
        Ok(true)
    }

    fn speak(
        &self,
        _text: &str,
        _stop: &AtomicBool,
        _on_level: Option<&mut dyn FnMut(f64)>,
        _on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        self.calls.lock().push("speak".to_string());
        Ok(true)
    }

    fn close(&self) {
        self.calls.lock().push("close".to_string());
    }
}

#[derive(Default)]
struct RecordingHooks {
    calls: Mutex<Vec<String>>,
}

impl RecordingHooks {
    fn record(&self, call: &str) {
        self.calls.lock().push(call.to_string());
    }
}

impl AppHooks for RecordingHooks {
    fn start_call(&self) {
        self.record("start_call");
    }

    fn resume_pending_tasks(&self) {
        self.record("resume_pending_tasks");
    }

    fn activity_tick(&self) {
        self.record("activity_tick");
    }

    fn fetch_llm_models(
        &self,
        _provider: &str,
        _base_url: &str,
        _api_key: &str,
        _api_style: &str,
        _force: bool,
    ) {
        self.record("fetch_llm_models");
    }

    fn refresh_devices(&self) {
        self.record("refresh_devices");
    }

    fn create_tray(&self) {
        self.record("create_tray");
    }

    fn update_tray_title(&self) {
        self.record("update_tray_title");
    }
}

#[derive(Default)]
struct RecordingObserver {
    steps: Mutex<Vec<String>>,
}

impl ShutdownObserver for RecordingObserver {
    fn step(&self, step: &str) {
        self.steps.lock().push(step.to_string());
    }
}

fn test_config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", Value::Bool(false));
    cfg.set("chat_voice", Value::Bool(false));
    cfg
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let unique = format!(
        "ezagent-f6-01-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

struct Fixture {
    app: VoiceApp,
    speaker: Arc<FakeSpeaker>,
    observer: Arc<RecordingObserver>,
    hooks: Arc<RecordingHooks>,
    tasks_path: std::path::PathBuf,
    config_file: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let speaker = Arc::new(FakeSpeaker::default());
        let output: Arc<dyn SpeakerApi> = speaker.clone();
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        let tasks_path = temp_path("tasks.json");
        let config_file = temp_path("voice.json");
        let observer = Arc::new(RecordingObserver::default());
        let hooks = Arc::new(RecordingHooks::default());
        let mut parts = AppParts::new(output, mic);
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        parts.observer = Some(observer.clone());
        let app = VoiceApp::with_services(test_config(), Some(tasks_path.clone()), parts);
        app.set_hooks(hooks.clone());
        Self {
            app,
            speaker,
            observer,
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

fn hud_of(inner: &Inner) -> &Arc<AppHud> {
    &inner.ui.hud
}

#[test]
fn starts_with_doubles_and_declares_every_group() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    assert_eq!(app.state(), IDLE);
    assert!(!app.inner.session.in_call.load(Ordering::SeqCst));
    assert!(!app.inner.session.muted.load(Ordering::SeqCst));
    assert!(app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert_eq!(app.inner.turn.turn_gen.load(Ordering::SeqCst), 0);
    assert_eq!(app.inner.ui.hotkeys.lock().len(), 8);
    assert!(app.inner.agents.swarm.lock().is_none());
    assert!(app.inner.agents.agent.lock().is_none());
    assert!(app.inner.agents.messenger.lock().is_some());
    assert!(app.inner.runtime.workers.lock().is_empty());
    assert!(app.tray().is_none());
    assert!(!hud_of(&app.inner).theme_name().is_empty());
    assert_eq!(
        fixture.observer.steps.lock().as_slice(),
        &["iniciado".to_string()]
    );
    assert_eq!(
        app.inner.config.saved_llm.lock().provider.as_str(),
        "opencode"
    );
    assert_eq!(app.inner.config.devices_cache.lock().len(), 0);
}

#[test]
fn shutdown_follows_the_python_order_and_runs_once() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.shutdown();
    let steps = fixture.observer.steps.lock().clone();
    assert_eq!(
        steps,
        vec![
            "iniciado",
            "agente-cancelar",
            "llamada-parar",
            "vigilante",
            "enjambre",
            "autonomo",
            "atajos",
            "hilos",
            "hilo-llamada",
            "mensajero",
            "stt",
            "whisper",
            "hud",
            "agente",
            "altavoz",
            "microfono",
            "detector",
            "bandeja",
            "terminado",
        ]
    );
    assert!(fixture
        .speaker
        .calls
        .lock()
        .iter()
        .any(|call| call == "close"));
    app.shutdown();
    assert_eq!(
        fixture.observer.steps.lock().last().map(String::as_str),
        Some("terminado")
    );
    assert_eq!(fixture.observer.steps.lock().len(), steps.len());
}

#[test]
fn warm_helpers_use_the_double_speaker() {
    let fixture = Fixture::new();
    fixture.app.warm_tts();
    assert_eq!(
        fixture.speaker.calls.lock().as_slice(),
        &["synthesize:Hola.".to_string()]
    );
}

#[test]
fn hotkey_key_normalizes_like_python() {
    assert_eq!(hotkey_key(" Ctrl + Alt + Space "), "ctrl+alt+space");
    assert_eq!(hotkey_key("a++b"), "a+b");
    assert_eq!(hotkey_key(""), "");
    assert_eq!(hotkey_key("CTRL+ALT+E"), "ctrl+alt+e");
}

#[test]
fn duplicate_hotkeys_report_the_python_text() {
    let mut values = Map::new();
    values.insert("hotkey".to_string(), json!("ctrl+alt+e"));
    values.insert("cancel_hotkey".to_string(), json!("ctrl + alt + E"));
    assert_eq!(
        check_hotkey_duplicates(&values),
        Err("Los atajos hotkey y cancel_hotkey son el mismo.".to_string())
    );
    values.insert("cancel_hotkey".to_string(), json!("ctrl+alt+c"));
    assert_eq!(check_hotkey_duplicates(&values), Ok(()));
    values.insert("repeat_hotkey".to_string(), json!(""));
    assert_eq!(check_hotkey_duplicates(&values), Ok(()));
}

#[test]
fn builds_primary_aliases_and_the_three_fixed_hotkeys() {
    let fixture = Fixture::new();
    let hotkeys = fixture.app.build_hotkeys();
    assert_eq!(hotkeys.len(), 8);
    assert_eq!(hotkeys[0].spec, "ctrl+alt+space");
    assert_eq!(hotkeys[0].event, "toggle");
    assert_eq!(hotkeys[0].hk_id, 47_201);
    assert_eq!(
        hotkeys
            .iter()
            .map(|hotkey| hotkey.spec.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ctrl+alt+space",
            "ctrl+alt+e",
            "ctrl+alt+c",
            "ctrl+alt+r",
            "ctrl+alt+y",
            "ctrl+alt+m",
            "ctrl+alt+o",
            "ctrl+alt+t",
        ]
    );
    assert_eq!(hotkeys[7].event, "chat");
    assert_eq!(hotkeys[7].hk_id, 47_222);
}

#[test]
fn alias_skips_a_key_already_used_by_the_primary_hotkey() {
    let fixture = Fixture::new();
    {
        let mut cfg = fixture.app.inner.config.cfg.write();
        cfg.set("hotkey", json!("ctrl+alt+e"));
        cfg.set("cancel_hotkey", json!(""));
    }
    let hotkeys = fixture.app.build_hotkeys();
    let specs: Vec<&str> = hotkeys.iter().map(|hotkey| hotkey.spec.as_str()).collect();
    assert_eq!(
        specs,
        vec![
            "ctrl+alt+e",
            "ctrl+alt+c",
            "ctrl+alt+r",
            "ctrl+alt+y",
            "ctrl+alt+m",
            "ctrl+alt+o",
            "ctrl+alt+t",
        ]
    );
}

#[test]
fn advance_turn_increments_the_generation() {
    let fixture = Fixture::new();
    assert_eq!(advance_turn(&fixture.app), 1);
    assert_eq!(advance_turn(&fixture.app), 2);
    assert_eq!(fixture.app.inner.turn.turn_gen.load(Ordering::SeqCst), 2);
}

#[test]
fn timed_stream_records_the_llm_span() {
    let timing = Arc::new(TurnTimings::new(0.0, 0.0));
    let items: Vec<i32> =
        timed_stream(vec![1, 2, 3].into_iter(), Some(Arc::clone(&timing))).collect();
    assert_eq!(items, vec![1, 2, 3]);
    let spans = timing.spans.lock().expect("spans");
    assert!(spans.get("llm").is_some_and(|value| *value >= 0.0));
}

#[test]
fn start_listening_on_launch_shows_the_hud_and_calls_the_hook() {
    let fixture = Fixture::new();
    fixture.app.start_listening_on_launch();
    assert!(!fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert_eq!(
        fixture.hooks.calls.lock().as_slice(),
        &["start_call".to_string()]
    );
    let saved = std::fs::read_to_string(&fixture.config_file).expect("voice.json de prueba");
    assert!(saved.contains("hud_hidden"));
}

#[test]
fn finish_start_reports_missing_whisper() {
    let mut cfg = test_config();
    cfg.set("hud_hidden", Value::Bool(true));
    let speaker = Arc::new(FakeSpeaker::default());
    let output: Arc<dyn SpeakerApi> = speaker;
    let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
    let mut parts = AppParts::new(output, mic);
    parts.chat = Some(Arc::new(ChatModel::new(None, false)));
    parts.whisper_ready = false;
    let app = VoiceApp::with_services(cfg, Some(temp_path("tasks.json")), parts);
    let events: Vec<AppEvent> = std::iter::from_fn(|| app.events().get_nowait()).collect();
    assert_eq!(
        events,
        vec![AppEvent::Error(
            "No encuentro whisper. Revisa la carpeta de whisper en la configuración.".to_string(),
            None,
        )]
    );
}

#[test]
fn the_app_installs_the_menu_provider_and_the_real_factory() {
    // `hud_hidden = true` deja la barra sin ventana: `toggle_menu` llega hasta
    // crear el menú (proveedor de ajustes y fábrica real) y se detiene ahí.
    let speaker = Arc::new(FakeSpeaker::default());
    let output: Arc<dyn SpeakerApi> = speaker;
    let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
    let tasks_path = temp_path("tasks.json");
    let mut parts = AppParts::new(output, mic);
    parts.chat = Some(Arc::new(ChatModel::new(None, false)));
    let app = VoiceApp::with_services(test_config(), Some(tasks_path.clone()), parts);
    app.toggle_menu();
    assert!(
        app.hud().menu_panel().is_some(),
        "la app instaló el proveedor y la fábrica real creó el menú"
    );
    let _ = std::fs::remove_file(&tasks_path);
}

#[test]
fn hud_config_reads_writes_and_remembers() {
    let cfg = Arc::new(parking_lot::RwLock::new(VoiceConfig::new()));
    let adapter = HudConfig {
        cfg,
        config_file: temp_path("hud.json"),
    };
    assert_eq!(
        super::hud_bridge::json_to_config(&json!(1.5)),
        Some(ez_hud::win::drop::ConfigValue::Float(1.5))
    );
    adapter.remember("ui_scale", ez_hud::win::drop::ConfigValue::Float(1.5));
    assert!(matches!(
        adapter.value("ui_scale"),
        Some(ez_hud::win::drop::ConfigValue::Float(value)) if value == 1.5
    ));
}

#[test]
fn filler_sentences_match_python() {
    assert!(is_filler_sentence("Un momento, lo reviso."));
    assert!(is_filler_sentence("¿Pensando?"));
    assert!(is_filler_sentence("Pensando…"));
    assert!(is_filler_sentence("Va, déjame ver."));
    assert!(is_filler_sentence("A ver."));
    assert!(is_filler_sentence("Dejame ver si esto funciona"));
    assert!(is_filler_sentence("Ya lo veo"));
    assert!(is_filler_sentence("ENSEGUIDA"));
    assert!(!is_filler_sentence("Busco en la web."));
    assert!(!is_filler_sentence("Sí, está abierto."));
    assert!(!is_filler_sentence(
        "Un momento: el agente t51 terminó y dejó tres archivos nuevos."
    ));
    assert!(!is_filler_sentence("Esto es una prueba completa"));
    assert!(!is_filler_sentence("una dos tres cuatro cinco seis siete"));
    assert!(!is_filler_sentence(""));
    // Los avisos concretos del código nunca se confunden con muletillas.
    for (name, arguments) in [
        ("web_search", "{}"),
        ("fetch_page", "{}"),
        ("run_skill_script", "{}"),
        ("delegate_to_agent", "{\"action\":\"send\"}"),
        (
            "delegate_to_agent",
            "{\"action\":\"run\",\"agent\":\"codex\"}",
        ),
    ] {
        let ack = ez_llm::messenger::tool_ack(name, arguments).expect("la herramienta tiene aviso");
        assert!(!is_filler_sentence(&ack), "{name}: {ack}");
    }
}

#[test]
fn apply_turn_settings_follows_the_config() {
    let fixture = Fixture::new();
    let detector = Arc::new(ez_audio::turn::SmartTurn::with_model(
        0.5,
        Arc::new(ez_audio::turn::UnavailableTurnModel),
    ));
    *fixture.app.inner.audio.turn_detector.lock() = Some(Arc::clone(&detector));
    fixture
        .app
        .inner
        .audio
        .mic
        .lock()
        .set_turn_detector(Some(Arc::clone(&detector)));
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("smart_turn", json!(false));
    fixture.app.apply_turn_settings();
    assert!(fixture.app.inner.audio.mic.lock().turn_detector().is_none());
    {
        let mut cfg = fixture.app.inner.config.cfg.write();
        cfg.set("smart_turn", json!(true));
        cfg.set("turn_threshold", json!(0.7));
    }
    fixture.app.apply_turn_settings();
    assert!((detector.threshold() - 0.7).abs() < 1e-9);
    assert!(fixture.app.inner.audio.mic.lock().turn_detector().is_some());
    assert_eq!(fixture.app.check_hotkey_duplicates(&Map::new()), Ok(()));
}

#[test]
fn pending_block_is_empty_without_targets() {
    let fixture = Fixture::new();
    assert_eq!(fixture.app.pending_block(), "");
}

#[test]
fn start_asks_the_hooks_and_skips_threads_in_doubles() {
    let fixture = Fixture::new();
    fixture.app.start();
    assert_eq!(
        fixture.hooks.calls.lock().clone(),
        vec![
            "resume_pending_tasks".to_string(),
            "fetch_llm_models".to_string(),
            "create_tray".to_string(),
            "update_tray_title".to_string(),
            "refresh_devices".to_string(),
        ]
    );
    assert!(fixture.app.inner.runtime.workers.lock().is_empty());
    fixture.app.shutdown();
}

#[test]
fn vad_line_matches_the_python_text() {
    let mut cfg = VoiceConfig::default();
    cfg.set("silence_secs", json!(1.3));
    cfg.set("min_rms", json!(0.014125));
    cfg.set("pre_roll_secs", json!(0.6));
    cfg.set("barge_in", json!(true));
    assert_eq!(
        super::init::vad_line(&cfg, true),
        "VAD: silencio=1.3s, fin de turno a 0.35s si la frase está completa, \
         min_rms=0.014125, pre_roll=0.6s, barge_in=True"
    );
    // Sin Smart Turn y con la interrupción apagada, como el Python.
    cfg.set("barge_in", json!(false));
    assert_eq!(
        super::init::vad_line(&cfg, false),
        "VAD: silencio=1.3s, sin Smart Turn, min_rms=0.014125, pre_roll=0.6s, \
         barge_in=False"
    );
}
