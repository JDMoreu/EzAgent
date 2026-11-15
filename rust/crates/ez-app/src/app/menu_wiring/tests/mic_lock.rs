//! Regresión de P-FIX-MICLOCK: el menú y el chat no esperan a la llamada.
//!
//! El Python lee los ajustes del micrófono en cada bloque (`audio.py`
//! `MicCapture.stream`), así que nada de la interfaz debe quedarse esperando a
//! que la captura termine. Estas pruebas usan la captura falsa de `ez-audio`
//! (`MicCapture::silent`), que se queda en `stream` sin abrir hardware.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_hud::win::drop::ConfigValue;
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::app::core::state::Flag;
use crate::app::core::{AppParts, SpeakerApi, VoiceApp};

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

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-miclock-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

/// App con la llamada activa sobre una captura que nunca sale de `stream`.
struct CallFixture {
    app: VoiceApp,
    mic: MicCapture,
    stop: Arc<Flag>,
    worker: Option<std::thread::JoinHandle<()>>,
    config_file: PathBuf,
    tasks_path: PathBuf,
}

impl CallFixture {
    fn new() -> Self {
        let mic = MicCapture::silent(VadSettings::default());
        let shared = Arc::new(Mutex::new(mic.clone()));
        let tasks_path = temp_path("tasks.json");
        let config_file = temp_path("voice.json");
        let mut cfg = VoiceConfig::new();
        cfg.set("smart_turn", Value::Bool(false));
        cfg.set("llm_base_url", json!(""));
        cfg.set("llm_api_style", json!("openai"));
        cfg.set("hud_hidden", Value::Bool(true));
        let mut parts = AppParts::new(Arc::new(SilentSpeaker), shared);
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(cfg, Some(tasks_path.clone()), parts);
        let stop = Arc::new(Flag::new());
        *app.inner.turn.call_stop.lock() = Some(Arc::clone(&stop));
        app.start_call();
        let worker = {
            let app = app.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || app.call_worker(stop))
        };
        // Espera a que la captura esté dentro de `stream`, como en la app real.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !mic.capturing() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(mic.capturing(), "la captura falsa entró en stream");
        Self {
            app,
            mic,
            stop,
            worker: Some(worker),
            config_file,
            tasks_path,
        }
    }
}

impl Drop for CallFixture {
    fn drop(&mut self) {
        self.stop.set();
        self.app.hang_up();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.tasks_path);
        let _ = std::fs::remove_file(&self.config_file);
    }
}

#[test]
fn hud_menu_settings_answers_while_the_call_is_capturing() {
    let fixture = CallFixture::new();
    let started = Instant::now();
    let settings = fixture.app.hud_menu_settings();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "el menú esperó a la captura: {:?}",
        started.elapsed()
    );
    assert!(settings.is_some(), "el menú devuelve su instantánea");
    assert!(
        fixture.mic.capturing(),
        "la llamada sigue capturando mientras se lee el menú"
    );
}

#[test]
fn live_min_rms_reaches_the_capture_in_progress() {
    let fixture = CallFixture::new();
    let started = Instant::now();
    fixture
        .app
        .hud_live_setting("min_rms", ConfigValue::Float(0.02));
    fixture.app.tick();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!((fixture.mic.min_rms() - 0.02).abs() < 1e-9);
    assert!(fixture.mic.capturing(), "la captura en curso sigue viva");
}

#[test]
fn tick_opens_the_panel_and_the_chat_during_a_call() {
    let fixture = CallFixture::new();
    fixture.app.on_hud_action("panel");
    fixture.app.on_hud_action("chat");
    let started = Instant::now();
    fixture.app.tick();
    // Detecta un bloqueo indefinido; 1 s fallaba con la suite en paralelo.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "el tick esperó a la captura: {:?}",
        started.elapsed()
    );
    assert!(
        fixture.app.hud().menu_panel().is_some(),
        "el panel se creó en el tick"
    );
    assert!(
        fixture.app.hud().chat_is_open(),
        "el chat se abrió en el tick"
    );
}
