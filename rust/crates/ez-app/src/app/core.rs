//! Núcleo de `VoiceApp`: estado, arranque y utilidades (F6-01).
//!
//! Porta `app.py` 1-383 (utilidades y `VoiceApp.__init__` 385-606, más
//! `_create_swarm`, `_pending_block`, `_create_agent_runtime`,
//! `_apply_turn_settings` y `_create_turn_detector`). El resto de `VoiceApp`
//! vive en `startup.rs` (F6-01) y en los archivos de F6-02…F6-07, que añaden
//! métodos sobre estos mismos campos.

mod callbacks;
mod chat_model;
mod events;
mod hooks;
mod hud;
mod hud_bridge;
mod init;
mod parts;
mod services;
pub mod state;
pub mod trace;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

use ez_agent::runtime::{
    AgentRuntime, RuntimeCallbacks, SwarmStack as AgentSwarmStack, TextFn, TextProducer,
};
use ez_audio::capture::{MicCapture, VadSettings};
use ez_audio::turn::SmartTurn;
use ez_chat::contexts::{ChatContexts, ContextHolder};
use ez_chat::session::ChatModel;
use ez_chat::store::{ChatStore, ModelMessage, Role};
use ez_core::config::{config_path, int_value, truthy, VoiceConfig};
use ez_core::metrics::{monotonic, LatencyStats, TurnTimings};
use ez_core::pyvalue::py_strip;
use ez_herdr::herdr::Herdr;
use ez_herdr::tasks::TaskRegistry;
use ez_herdr::watcher::{AnnounceCallback, EventCallback, TargetsCallback, Watcher};
use ez_hud::win::drop::create_ui;
use ez_hud::win::ui_loop::UiLoop;
use ez_llm::messenger::{AgentRuntimeApi, Messenger, MessengerCallbacks};
use ez_swarm::stack::SwarmStack;
use ez_swarm::supervisor::HerdrHost;
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use crate::tray::TrayIcon;

use self::callbacks::mic_capture;
pub use self::chat_model::AppChat;
pub use self::events::{AppEvent, MenuEvent, Queue};
pub use self::hooks::{AppHooks, NullHooks, NullObserver, ShutdownObserver};
pub use self::hud::AppHud;
pub use self::hud_bridge::{HudConfig, HudHost};
pub use self::services::{AppParts, SpeakerApi, SwarmArgs, SwarmFactory};
use self::state::{
    AgentsState, AudioState, ChatState, ConfigState, Flag, Inner, LlmModels, MessengerSlot,
    RuntimeState, SavedLlm, SessionState, SpeechState, TurnState, UiState,
};
pub use self::trace::{AppTrace, TraceSlot};

pub use ez_stt::server::{ensure_whisper, free_local_port, resource_root, whisper_ready};

/// Estados de la barra, igual que las constantes de `app.py`.
pub const IDLE: &str = "idle";
pub const LISTENING: &str = "listening";
pub const THINKING: &str = "thinking";
pub const SPEAKING: &str = "speaking";

/// Claves de atajo que se comparan para detectar duplicados.
pub const HOTKEY_KEYS: [&str; 6] = [
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
    "chat_hotkey",
];

/// Ajustes que el menú aplica y guarda al instante.
pub const INSTANT_SETTINGS: [&str; 14] = [
    "ui_layout",
    "theme",
    "theme_base",
    "theme_primary",
    "theme_secondary",
    "theme_accent",
    "theme_background",
    "ui_motion",
    "ui_scale",
    "show_advanced",
    "close_action",
    "voice_wake",
    "wake_phrase",
    "chat_voice",
];

/// Voces del menú con su etiqueta.
pub const MENU_VOICES: [(&str, &str); 3] = [
    ("em_alex", "Álex · masculina"),
    ("ef_dora", "Dora · femenina"),
    ("em_santa", "Santa · masculina grave"),
];

/// Cada cuánto se actualiza la actividad de la barra.
pub const ACTIVITY_TICK_MS: u64 = 1500;
/// Ventana para unir una frase nueva con la anterior.
pub const RESUME_SECS: f64 = 3.0;

/// `log(message)`: `logger.info(redact(str(message)))`.
pub fn log(message: impl AsRef<str>) {
    tracing::info!("{}", ez_core::redact::redact(message.as_ref()));
}

/// `_hotkey_key`: normaliza un atajo para comparar duplicados.
pub fn hotkey_key(spec: &str) -> String {
    spec.split('+')
        .map(py_strip)
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("+")
}

/// Lectura de texto de la configuración con valor por omisión.
pub fn cfg_text(cfg: &VoiceConfig, key: &str, default: &str) -> String {
    cfg.get(key)
        .and_then(Value::as_str)
        .map_or_else(|| default.to_string(), str::to_string)
}

/// Lectura de número de la configuración (`float(value)` aproximado).
pub fn cfg_float(cfg: &VoiceConfig, key: &str, default: f64) -> f64 {
    match cfg.get(key) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(default),
        Some(Value::Bool(flag)) => {
            if *flag {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::String(text)) => py_strip(text).parse().ok().unwrap_or(default),
        _ => default,
    }
}

/// Lectura de entero de la configuración (`int(value)` con el `default` del Python).
pub fn cfg_int(cfg: &VoiceConfig, key: &str, default: i64) -> i64 {
    cfg.get(key)
        .map_or(default, |value| int_value(value, default))
}

/// Lectura booleana de la configuración (`bool(value)`).
pub fn cfg_truthy(cfg: &VoiceConfig, key: &str, default: bool) -> bool {
    cfg.get(key).map_or(default, truthy)
}

/// Muletillas de espera (`_FILLER_MARKERS`), sin tildes y en minúsculas.
const FILLER_MARKERS: [&str; 15] = [
    "un momento",
    "un segundo",
    "dame un",
    "dejame ver",
    "dejame revisar",
    "dejame pensar",
    "pensando",
    "lo reviso",
    "ya lo veo",
    "lo busco",
    "lo miro",
    "ya lo hago",
    "enseguida",
    "veamos",
    "a ver",
];

/// `_is_filler_sentence`: `true` si la frase es solo una muletilla de espera.
pub fn is_filler_sentence(sentence: &str) -> bool {
    let plain = ez_core::pyvalue::strip_accents(&sentence.to_lowercase());
    let cleaned: String = plain
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == 'ñ'
                || character == ' '
                || character.is_whitespace()
            {
                character
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() || words.len() > 6 {
        return false;
    }
    let text = format!(" {} ", words.join(" "));
    FILLER_MARKERS
        .iter()
        .any(|marker| text.contains(&format!(" {marker} ")))
}

/// `_advance_turn`: invalida el turno vigente y cancela el anterior.
pub fn advance_turn(app: &VoiceApp) -> i64 {
    let generation = app.inner.turn.turn_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let messenger = app.inner.agents.messenger.lock().clone();
    if let Some(messenger) = messenger {
        messenger.cancel_turn();
    }
    generation
}

/// `_timed_stream`: suma al turno solo el tiempo que se espera por cada trozo.
pub struct TimedStream<I> {
    iterator: I,
    timing: Option<Arc<TurnTimings>>,
}

impl<I: Iterator> Iterator for TimedStream<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let started = monotonic();
        let item = self.iterator.next();
        if let Some(timing) = &self.timing {
            timing.add_span("llm", monotonic() - started);
        }
        item
    }
}

/// Envuelve un flujo del LLM para medir el tiempo de espera (`_timed_stream`).
pub fn timed_stream<I: Iterator>(stream: I, timing: Option<Arc<TurnTimings>>) -> TimedStream<I> {
    TimedStream {
        iterator: stream,
        timing,
    }
}

/// Aplicación de voz: todas las copias comparten el mismo estado.
#[derive(Clone)]
pub struct VoiceApp {
    pub(crate) inner: Arc<Inner>,
}

impl VoiceApp {
    /// `VoiceApp.__init__`: construye los servicios reales y arranca la app.
    // DECISION: sin manejadores de SIGINT/SIGTERM (Tk no existe y no hay crate
    // de señales); el bucle se cierra con `UiLoop::quit`.
    pub fn new(
        cfg: VoiceConfig,
        tasks_path: Option<PathBuf>,
        config_file: Option<PathBuf>,
    ) -> Result<Self, String> {
        let mut cfg = cfg;
        let whisper_proc = ensure_whisper(&mut cfg);
        let ready = whisper_ready(&cfg_text(&cfg, "whisper_url", ""));
        log("Cargando Kokoro…");
        let speaker: Arc<dyn SpeakerApi> = match ez_tts::speaker::Speaker::new(cfg.clone()) {
            Ok(speaker) => Arc::new(speaker),
            Err(error) => return Err(format!("No pude cargar Kokoro: {}", error.0)),
        };
        let mic = Arc::new(Mutex::new(mic_capture(&cfg)));
        let mut parts = AppParts::new(speaker, mic);
        parts.config_file = config_file;
        parts.whisper_proc = whisper_proc;
        parts.whisper_ready = ready;
        parts.build_swarm = true;
        parts.build_agent = true;
        parts.background = true;
        Ok(Self::with_services(cfg, tasks_path, parts))
    }

    /// `self.cfg` vigente.
    pub fn config(&self) -> VoiceConfig {
        self.inner.config.cfg.read().clone()
    }

    /// Sustituye la configuración en memoria (el equivalente a mutar `cfg`).
    pub fn set_config(&self, cfg: VoiceConfig) {
        *self.inner.config.cfg.write() = cfg;
    }

    /// `self.state`.
    pub fn state(&self) -> String {
        self.inner.session.state.read().clone()
    }

    /// `self.last_summary`.
    pub fn last_summary(&self) -> String {
        self.inner.session.last_summary.read().clone()
    }

    /// `self.events`.
    pub fn events(&self) -> &Arc<Queue<AppEvent>> {
        &self.inner.ui.events
    }

    /// `self.hotkey_events`.
    pub fn hotkey_events(&self) -> &Arc<Queue<String>> {
        &self.inner.ui.hotkey_events
    }

    /// `self.menu_events`.
    pub fn menu_events(&self) -> &Arc<Queue<MenuEvent>> {
        &self.inner.ui.menu_events
    }

    /// `self.hud`.
    pub fn hud(&self) -> &Arc<AppHud> {
        &self.inner.ui.hud
    }

    /// `self.tray` (lo instala F6-02).
    pub fn tray(&self) -> Option<Arc<TrayIcon>> {
        self.inner.ui.tray.lock().clone()
    }

    /// Ganchos vigentes (F6-02…F6-07).
    pub fn hooks(&self) -> Arc<dyn AppHooks> {
        self.inner.runtime.hooks.read().clone()
    }

    /// Anota la escritura real de `voice.json` para el arnés (F7-01c).
    pub(crate) fn trace_config_write(&self) {
        let name = self
            .inner
            .config
            .config_file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self::trace::notify(&self.inner.runtime.trace, |trace| {
            trace.config_file_write(&name);
        });
    }

    /// Instala los ganchos de una tarea posterior.
    pub fn set_hooks(&self, hooks: Arc<dyn AppHooks>) {
        *self.inner.runtime.hooks.write() = hooks;
    }

    /// Lanza un hilo con el nombre de Python (`_start_worker`).
    pub fn start_worker<F>(&self, name: &str, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        match std::thread::Builder::new()
            .name(name.to_string())
            .spawn(job)
        {
            Ok(handle) => self.inner.runtime.workers.lock().push(handle),
            Err(error) => log(format!("No pude crear el hilo {name}: {error}")),
        }
    }

    /// `VoiceApp._check_hotkey_duplicates` (staticmethod en Python).
    pub fn check_hotkey_duplicates(
        &self,
        values: &serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        super::startup::check_hotkey_duplicates(values)
    }
}

/// `_whisper_proc` de la app (para F6-07 y las pruebas).
pub fn whisper_process(app: &VoiceApp) -> Option<Child> {
    app.inner.runtime.whisper_proc.lock().take()
}
