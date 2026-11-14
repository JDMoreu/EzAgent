//! Servicios externos del arranque y sus costuras de prueba (F6-01).
//!
//! `VoiceApp.__init__` construye el altavoz, el micrófono y el HUD. El altavoz
//! carga Kokoro y no se puede abrir en una prueba unitaria; por eso entra por
//! [`AppParts`] detrás de [`SpeakerApi`]. El micrófono y el HUD se crean sin
//! abrir hardware ni ventanas, así que se usan tal cual.

use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ez_audio::capture::MicCapture;
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use ez_hud::win::drop::DropUI;
use parking_lot::Mutex;

use super::hooks::ShutdownObserver;

/// Lo que la app necesita del altavoz (`ez_tts::Speaker` en producción).
// DECISION: Kokoro carga un modelo real y no se puede abrir en una prueba;
// este rasgo es la costura de los dobles (`AppParts::new`).
pub trait SpeakerApi: Send + Sync {
    /// `Speaker.synthesize`.
    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String>;
    /// `Speaker.play`.
    fn play(
        &self,
        samples: &[f32],
        sample_rate: u32,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String>;
    /// `Speaker.speak`.
    fn speak(
        &self,
        text: &str,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String>;
    /// `Speaker.set_config`.
    fn set_config(&self, _cfg: VoiceConfig) {}
    /// `Speaker.set_voice`.
    fn set_voice(&self, _voice: &str) -> Result<(), String> {
        Ok(())
    }
    /// `Speaker.reload_model`.
    fn reload_model(&self) -> Result<(), String> {
        Ok(())
    }
    /// `Speaker.close`.
    fn close(&self) {}
}

impl SpeakerApi for ez_tts::speaker::Speaker {
    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String> {
        ez_tts::speaker::Speaker::synthesize(self, text).map_err(|error| error.0)
    }

    fn play(
        &self,
        samples: &[f32],
        sample_rate: u32,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        ez_tts::speaker::Speaker::play(self, samples, sample_rate, stop, on_level, on_start)
            .map_err(|error| error.0)
    }

    fn speak(
        &self,
        text: &str,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        ez_tts::speaker::Speaker::speak(self, text, stop, on_level, on_start)
            .map_err(|error| error.0)
    }

    fn set_config(&self, cfg: VoiceConfig) {
        ez_tts::speaker::Speaker::set_config(self, cfg);
    }

    fn set_voice(&self, voice: &str) -> Result<(), String> {
        ez_tts::speaker::Speaker::set_voice(self, voice).map_err(|error| error.0)
    }

    fn reload_model(&self) -> Result<(), String> {
        ez_tts::speaker::Speaker::reload_model(self).map_err(|error| error.0)
    }

    fn close(&self) {
        ez_tts::speaker::Speaker::close(self);
    }
}

/// Piezas ya construidas con las que nace la aplicación.
pub struct AppParts {
    /// `config_file` del constructor (por omisión, `config_path()`).
    pub config_file: Option<PathBuf>,
    /// `self.speaker`.
    pub speaker: Arc<dyn SpeakerApi>,
    /// `self.mic`.
    pub mic: Arc<Mutex<MicCapture>>,
    /// `self.hud` ya creado; `None` lo construye `with_services`.
    pub hud: Option<Arc<DropUI>>,
    /// `self.chat`; `None` usa el historial persistente de fábrica.
    pub chat: Option<Arc<ChatModel>>,
    /// `self.herdr`; `None` lo construye con la configuración.
    pub herdr: Option<Arc<Herdr>>,
    /// `_whisper_proc` que devolvió `ensure_whisper`.
    pub whisper_proc: Option<Child>,
    /// `_whisper_ready(cfg.whisper_url)` ya medido.
    pub whisper_ready: bool,
    /// `true` monta `_create_swarm` al arrancar (F6-01 en producción).
    pub build_swarm: bool,
    /// `true` monta `_create_agent_runtime` al arrancar.
    pub build_agent: bool,
    /// `false` en los dobles: sin hilos de fondo ni atajos reales.
    pub background: bool,
    /// Observador del orden de apagado.
    pub observer: Option<Arc<dyn ShutdownObserver>>,
}

impl AppParts {
    /// Piezas mínimas de una prueba: sin HUD (se construye sin ventana) ni
    /// servicios pesados.
    pub fn new(speaker: Arc<dyn SpeakerApi>, mic: Arc<Mutex<MicCapture>>) -> Self {
        Self {
            config_file: None,
            speaker,
            mic,
            hud: None,
            chat: None,
            herdr: None,
            whisper_proc: None,
            whisper_ready: true,
            build_swarm: false,
            build_agent: false,
            background: false,
            observer: None,
        }
    }
}
