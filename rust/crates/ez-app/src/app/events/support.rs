//! Dobles compartidos por las pruebas de F6-03: ningún test abre micrófono.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::app::core::VoiceApp;
use crate::app::core::{AppParts, SpeakerApi};
use crate::app::states::tests::support::redirect_logs;

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

/// App con dobles, chat sin disco y micrófono que nunca abre un dispositivo.
pub(crate) struct Fixture {
    pub app: VoiceApp,
    pub mic: Arc<Mutex<MicCapture>>,
    config_file: PathBuf,
    tasks_path: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        redirect_logs();
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        // Ningún test debe intentar abrir el dispositivo por omisión.
        mic.lock().set_device_index(None);
        let tasks_path = temp_path("tasks.json");
        let config_file = temp_path("voice.json");
        let mut parts = AppParts::new(Arc::new(FakeSpeaker), Arc::clone(&mic));
        parts.chat = Some(Arc::new(ChatModel::new(None, false)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(test_config(), Some(tasks_path.clone()), parts);
        Self {
            app,
            mic,
            config_file,
            tasks_path,
        }
    }

    /// Contenido de `voice.json` (lo escrito por `save_key`).
    pub(crate) fn saved_config(&self) -> String {
        std::fs::read_to_string(&self.config_file).unwrap_or_default()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tasks_path);
        let _ = std::fs::remove_file(&self.config_file);
    }
}

fn test_config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg
}

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-f6-03-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}
