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

use ez_audio::barge_in::PlaybackReference;
use ez_audio::capture::MicCapture;
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use ez_hud::win::drop::{DropUI, WindowFactory};
use ez_llm::messenger::AgentRuntimeApi;
use ez_swarm::stack::SwarmStack;
use ez_swarm::supervisor::{Announce, SwarmHost};
use parking_lot::Mutex;

use super::hooks::ShutdownObserver;
use super::trace::AppTrace;

/// Envoltorio del runtime que ve el mensajero (traza de herramientas).
pub type RuntimeWrap =
    Arc<dyn Fn(Arc<dyn AgentRuntimeApi>) -> Arc<dyn AgentRuntimeApi> + Send + Sync>;

/// Piezas con las que la app monta el enjambre (`_create_swarm`).
pub struct SwarmArgs {
    /// Carpeta del tablero, los asientos y el endpoint.
    pub base_dir: PathBuf,
    /// Adaptador de Herdr.
    pub host: Arc<dyn SwarmHost>,
    /// Aviso hablado.
    pub announce: Announce,
    /// Pregunta al usuario.
    pub ask: Option<Announce>,
    /// Nombre del comando de los agentes.
    pub command: String,
}

/// Fábrica del enjambre; el arnés la usa para inyectar el reloj de prueba.
pub type SwarmFactory = Arc<dyn Fn(SwarmArgs) -> Option<Arc<SwarmStack>> + Send + Sync>;

/// Ejecuta `ez retry <nombre> --auto` y devuelve el nombre del hijo nuevo.
///
/// `Ok` con el nombre nuevo (o el mismo si `ez` no lo dice); `Err` con el
/// motivo para la persona. La costura permite probar la aceptación sin `ez`.
pub type RetryRunner = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

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
    /// Envolvente de lo que suena, para el barge-in adaptativo de la captura.
    fn reference(&self) -> Option<Arc<PlaybackReference>> {
        None
    }
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

    fn reference(&self) -> Option<Arc<PlaybackReference>> {
        Some(ez_tts::speaker::Speaker::reference(self))
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
    /// Fábrica de ventanas del HUD; `None` usa la nativa de `ez-hud`.
    pub hud_factory: Option<Arc<dyn WindowFactory>>,
    /// `self.chat`; `None` usa el historial persistente de fábrica.
    pub chat: Option<Arc<ChatModel>>,
    /// `self.herdr`; `None` lo construye con la configuración.
    pub herdr: Option<Arc<Herdr>>,
    /// Raíz del registro de subagentes; `None` usa `EZ_SUBAGENTS_DIR` o
    /// `%LOCALAPPDATA%`. Las pruebas pasan siempre una carpeta temporal.
    pub subagents_dir: Option<PathBuf>,
    /// Reintento inyectado; `None` ejecuta `ez retry ... --auto` de verdad.
    pub retry_runner: Option<RetryRunner>,
    /// `_whisper_proc` que devolvió `ensure_whisper`.
    pub whisper_proc: Option<Child>,
    /// `_whisper_ready(cfg.whisper_url)` ya medido.
    pub whisper_ready: bool,
    /// `true` monta `_create_swarm` al arrancar (F6-01 en producción).
    pub build_swarm: bool,
    /// `true` monta `_create_agent_runtime` al arrancar.
    pub build_agent: bool,
    /// Fábrica del enjambre; `None` usa la de producción (F7-01c).
    pub swarm_factory: Option<SwarmFactory>,
    /// Servicios que el arnés sustituye por dobles que fallan (`degrade:`).
    pub degrade: Vec<String>,
    /// `false` en los dobles: sin hilos de fondo ni atajos reales.
    pub background: bool,
    /// `true` lanza `_warm_stt`; el arnés lo apaga para no gastar un guion.
    pub warm_stt: bool,
    /// `true` arranca la captura real al empezar la llamada (F7-01a la apaga
    /// para alimentar el micrófono desde el arnés).
    pub mic_capture: bool,
    /// `false` no deja pasar los acuses hablados de herramienta (costura del
    /// arnés equivalente a `llm.tool_ack = None` del Python).
    pub tool_acks: bool,
    /// Observador de la traza de reproducción (F7-01a).
    pub trace: Option<Arc<dyn AppTrace>>,
    /// Envoltorio del runtime para la traza de herramientas (F7-01a).
    pub runtime_wrap: Option<RuntimeWrap>,
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
            hud_factory: None,
            chat: None,
            herdr: None,
            subagents_dir: None,
            retry_runner: None,
            whisper_proc: None,
            whisper_ready: true,
            build_swarm: false,
            build_agent: false,
            swarm_factory: None,
            degrade: Vec::new(),
            background: false,
            warm_stt: true,
            mic_capture: true,
            tool_acks: true,
            trace: None,
            runtime_wrap: None,
            observer: None,
        }
    }
}
