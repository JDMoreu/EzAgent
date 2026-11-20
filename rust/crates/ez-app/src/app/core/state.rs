//! Estado compartido de [`super::VoiceApp`] (F6-01).
//!
//! Cada grupo equivale a un bloque de atributos de `VoiceApp` en `app.py` y
//! lleva su propio cerrojo o sus propios atómicos. Los nombres documentan el
//! atributo Python que representan para que F6-02…F6-07 lo encuentren.

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize};
use std::sync::Arc;
use std::thread::JoinHandle;

use ez_audio::capture::{InputDevice, MicCapture};
use ez_audio::turn::SmartTurn;
use ez_chat::contexts::ChatContexts;
use ez_core::config::VoiceConfig;
use ez_core::metrics::{LatencyStats, TurnTimings};
use ez_herdr::herdr::Herdr;
use ez_herdr::subagents::Registry;
use ez_herdr::tasks::TaskRegistry;
use ez_herdr::watcher::Watcher;
use ez_hud::win::ui_loop::UiLoop;
use ez_llm::messenger::Messenger;
use ez_swarm::stack::SwarmStack;
use parking_lot::{Condvar, Mutex, RwLock};
use serde_json::{Map, Value};

use crate::hotkeys::GlobalHotkey;
use crate::tray::TrayIcon;

use super::chat_model::AppChat;
use super::events::{AppEvent, MenuEvent, Queue};
use super::hooks::{AppHooks, ShutdownObserver};
use super::hud::AppHud;
use super::services::SpeakerApi;
use super::trace::TraceSlot;

/// `threading.Event`: indicador con espera interrumpible.
#[derive(Debug, Default)]
pub struct Flag {
    flag: Mutex<bool>,
    changed: Condvar,
}

impl Flag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self) {
        *self.flag.lock() = true;
        self.changed.notify_all();
    }

    pub fn clear(&self) {
        *self.flag.lock() = false;
    }

    pub fn is_set(&self) -> bool {
        *self.flag.lock()
    }

    /// `Event.wait(timeout)`: `true` si ya estaba puesto o se puso a tiempo.
    pub fn wait_timeout(&self, secs: f64) -> bool {
        if self.is_set() {
            return true;
        }
        let mut guard = self.flag.lock();
        let _ = self.changed.wait_for(
            &mut guard,
            std::time::Duration::from_secs_f64(secs.max(0.0)),
        );
        *guard
    }
}

/// `Messenger` compartido: el `ChatContexts` y la app lo consultan sin copiarlo.
pub type MessengerSlot = Arc<Mutex<Option<Messenger>>>;
/// Temporizador de fin de turno (Python guarda el objeto `SmartTurn`).
pub type TurnDetectorSlot = Mutex<Option<Arc<SmartTurn>>>;
/// Función de condensación ya construida (`Condenser.conclusion_start`).
pub type CondenserFn = Arc<dyn Fn(&[String]) -> Option<usize> + Send + Sync>;

/// `(pcm, rate, gen, timing)` de `_stt_pending`.
pub struct PendingStt {
    pub pcm: Vec<u8>,
    pub rate: u32,
    pub generation: i64,
    pub timing: Arc<TurnTimings>,
}

/// `_stt_lock` + `_stt_pending`: quién transcribe y qué espera su turno.
#[derive(Default)]
pub struct SttQueue {
    pub busy: bool,
    pub pending: Option<PendingStt>,
}

/// `(llm_provider, llm_model, llm_base_url, api_key, llm_api_style)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedLlm {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub api_style: String,
}

/// Lista real de modelos por proveedor (`_llm_models_cache`) y lo ya pedido
/// (`_llm_models_tried`), que comparten `_llm_models_lock` en Python.
#[derive(Default)]
pub struct LlmModels {
    pub cache: HashMap<String, Vec<String>>,
    pub tried: HashSet<(String, String, String)>,
}

/// Configuración y lo guardado para restaurar (`cfg`, `config_file`, `_saved_*`).
pub struct ConfigState {
    /// `self.cfg`; compartida con el adaptador del HUD.
    pub cfg: Arc<RwLock<VoiceConfig>>,
    /// `self.config_file`.
    pub config_file: std::path::PathBuf,
    /// `_saved_llm`.
    pub saved_llm: Mutex<SavedLlm>,
    /// `_saved_stt_model`.
    pub saved_stt_model: RwLock<String>,
    /// `_saved_stt_engine`.
    pub saved_stt_engine: RwLock<String>,
    /// `_llm_models_cache` + `_llm_models_tried` bajo `_llm_models_lock`.
    pub llm_models: Mutex<LlmModels>,
    /// `_devices_cache`.
    pub devices_cache: Mutex<Vec<InputDevice>>,
    /// `_menu_saved`.
    pub menu_saved: Mutex<Option<Map<String, Value>>>,
    /// `_hud_hidden`.
    pub hud_hidden: AtomicBool,
}

/// Estado visible y de sesión (`state`, `in_call`, `muted`…).
pub struct SessionState {
    /// `self.state`: `idle`, `listening`, `thinking`, `speaking`.
    pub state: RwLock<String>,
    /// `self.in_call`.
    pub in_call: AtomicBool,
    /// `self.muted`.
    pub muted: AtomicBool,
    /// `_user_speaking`.
    pub user_speaking: AtomicBool,
    /// `_ignore_utterance`.
    pub ignore_utterance: AtomicBool,
    /// `self.last_summary`.
    pub last_summary: RwLock<String>,
    /// `self.mute_until`.
    pub mute_until: Mutex<f64>,
    /// `_shutdown_done`.
    pub shutdown_done: AtomicBool,
    /// `_restarting`.
    pub restarting: AtomicBool,
}

/// Estado del turno y de la transcripción (`_turn_*`, `_stt_*`, `_call_*`).
pub struct TurnState {
    /// `_last_turn_audio`: `(pcm, rate, monotonic)`.
    pub last_turn_audio: Mutex<Option<(Vec<u8>, u32, f64)>>,
    /// `_resume_pcm`: `(pcm, rate)` de la frase que se une a la siguiente.
    pub resume_pcm: Mutex<Option<(Vec<u8>, u32)>>,
    /// `_last_voice_user_id`.
    pub last_voice_user_id: Mutex<Option<i64>>,
    /// `_merge_user_id`.
    pub merge_user_id: Mutex<Option<i64>>,
    /// `_close_after_answer`.
    pub close_after_answer: AtomicBool,
    /// `_close_mode`: `""`, `pause` o `hangup`.
    pub close_mode: RwLock<String>,
    /// `_wake_lock`.
    pub wake_lock: Mutex<()>,
    /// `_post_tts_until`.
    pub post_tts_until: Mutex<f64>,
    /// `_post_tts_ignore`.
    pub post_tts_ignore: AtomicBool,
    /// `_call_stop` (se reasigna al empezar la llamada).
    pub call_stop: Mutex<Option<Arc<Flag>>>,
    /// `_call_thread`.
    pub call_thread: Mutex<Option<JoinHandle<()>>>,
    /// `_turn_gen`.
    pub turn_gen: AtomicI64,
    /// `_turn_started`.
    pub turn_started: Mutex<f64>,
    /// `_turn_timing`.
    pub turn_timing: Mutex<Option<Arc<TurnTimings>>>,
    /// `_stt_lock` + `_stt_pending`.
    pub stt: Mutex<SttQueue>,
    /// `_confirm_token`.
    pub confirm_token: RwLock<String>,
}

/// Micrófono y su detector de fin de turno (`mic`, `_reset_capture`).
pub struct AudioState {
    /// `self.mic`.
    pub mic: Arc<Mutex<MicCapture>>,
    /// `_reset_capture`.
    pub reset_capture: Arc<Flag>,
    /// `self.turn_detector`.
    pub turn_detector: TurnDetectorSlot,
}

impl AudioState {
    /// Clon del micrófono vigente. `MicCapture` comparte su estado y su
    /// captura no retiene el cerrojo, así que tomarlo aquí nunca espera a la
    /// llamada en curso.
    pub fn mic(&self) -> MicCapture {
        self.mic.lock().clone()
    }

    /// `self.mic.device_index` (espejo del dispositivo en uso).
    pub fn device_index(&self) -> Option<usize> {
        self.mic().device_index()
    }
}

/// Voz y avisos hablados (`speaker`, `_tts_*`, `_busy_*`, `_speaking_kind`).
pub struct SpeechState {
    /// `self.speaker`.
    pub speaker: Arc<dyn SpeakerApi>,
    /// `_tts_stop` bajo `_tts_lock`.
    pub tts_stop: Mutex<Option<Arc<Flag>>>,
    /// `_busy_speaking`.
    pub busy_speaking: Arc<Flag>,
    /// `_busy_stop`.
    pub busy_stop: Mutex<Option<Arc<Flag>>>,
    /// `_busy_barged`.
    pub busy_barged: AtomicBool,
    /// `_speaking_kind`: `answer`, `notice`…
    pub speaking_kind: RwLock<Option<String>>,
    /// Tipo de la última frase hablada (`announce`, `answer`, `ack`…).
    pub last_outbound_kind: RwLock<Option<String>>,
    /// `monotonic()` cuando terminó la última frase hablada.
    pub last_outbound_done_at: RwLock<f64>,
    /// `true` si la última frase hablada terminaba en «?».
    pub last_outbound_question: RwLock<bool>,
    /// Índice para rotar las respuestas de acuse sin repetir la anterior.
    pub ack_index: AtomicUsize,
}

/// Chat escrito y contexto por conversación (`chat`, `contexts`, `_condenser*`).
pub struct ChatState {
    /// `self.chat`.
    pub chat: Arc<AppChat>,
    /// `self.contexts`.
    pub contexts: Mutex<ChatContexts>,
    /// `_condenser`, ya reducido a su `conclusion_start`.
    // DECISION: `Condenser<'a>` presta el cliente y no se puede guardar; se
    // guarda su llamada y F6-05 la construye desde el cliente del mensajero.
    pub condenser: Mutex<Option<CondenserFn>>,
    /// `_condenser_client`: identidad del cliente vigente, para reconstruir.
    pub condenser_client: Mutex<Option<usize>>,
}

/// Agentes, vigilancia y enjambre (`tasks`, `herdr`, `watcher`, `swarm`…).
pub struct AgentsState {
    /// `self.tasks`.
    pub tasks: Arc<TaskRegistry>,
    /// `self.herdr`.
    pub herdr: Arc<Herdr>,
    /// Registro de los subagentes que lanzó la app (NV-3, padre `voz`).
    pub registry: Arc<Registry>,
    /// Subagente cuya oferta de reintento espera un «sí»/«dale».
    pub retry_offer: Mutex<Option<String>>,
    /// Detiene el vigilante interno de `voz` al cerrar.
    pub subagents_watch_stop: Arc<Flag>,
    /// Estados ya contados en esta sesión (el `notified` del disco se escribe
    /// en un hilo aparte para no esperar el candado del registro en la UI).
    pub announced: Mutex<HashMap<String, ez_herdr::subagents::TaskState>>,
    /// `self.watcher`.
    pub watcher: Arc<Watcher>,
    /// `self.swarm`.
    pub swarm: Mutex<Option<Arc<SwarmStack>>>,
    /// `self.agent`.
    pub agent: Mutex<Option<ez_agent::runtime::AgentRuntime>>,
    /// `self.messenger`.
    pub messenger: MessengerSlot,
    /// `self.watched`.
    pub watched: Arc<Mutex<HashSet<String>>>,
    /// `_deliver` (creado en diferido en Python).
    pub deliver: Mutex<HashSet<String>>,
    /// `_sent`: texto exacto pegado a cada agente (ancla para recortar).
    pub sent: Mutex<HashMap<String, String>>,
    /// `_prelaunch` (`_prelaunch_lock` sobra con el mutex).
    pub prelaunch: Mutex<HashMap<String, Map<String, Value>>>,
    /// `_announce_queue`.
    pub announce_queue: Mutex<Vec<(String, bool)>>,
    /// Bucle del modo autónomo (AUTO-A3); `None` si no hay núcleo agéntico.
    pub autonomous: Mutex<Option<Arc<crate::app::autonomous::AutonomousLoop>>>,
}

/// Interfaz, colas y accesorios del hilo principal (`root`, `hud`, `events`…).
pub struct UiState {
    /// `self.root` / `root.mainloop` / `root.after`.
    pub ui: UiLoop,
    /// `self.hud`.
    pub hud: Arc<AppHud>,
    /// `self.events`.
    pub events: Arc<Queue<AppEvent>>,
    /// `self.hotkey_events`.
    pub hotkey_events: Arc<Queue<String>>,
    /// `self.menu_events`.
    pub menu_events: Arc<Queue<MenuEvent>>,
    /// `self.tray`.
    pub tray: Mutex<Option<Arc<TrayIcon>>>,
    /// `self.hotkeys`.
    pub hotkeys: Mutex<Vec<GlobalHotkey>>,
}

/// Procesos, hilos y ganchos de ciclo de vida (`_workers`, `_whisper_proc`…).
pub struct RuntimeState {
    /// `_workers`.
    pub workers: Mutex<Vec<JoinHandle<()>>>,
    /// `_latency`.
    pub latency: Mutex<LatencyStats>,
    /// `_whisper_proc`.
    pub whisper_proc: Mutex<Option<Child>>,
    /// Ganchos de las tareas F6-02…F6-07 (compartidos con el vigilante).
    pub hooks: Arc<RwLock<Arc<dyn AppHooks>>>,
    /// Observador del arnés de reproducción (F7-01a).
    pub trace: TraceSlot,
    /// Envoltorio del runtime para la traza de herramientas (F7-01a).
    pub runtime_wrap: Option<super::services::RuntimeWrap>,
    /// Fábrica del enjambre (F7-01c); `None` usa la de producción.
    pub swarm_factory: Option<super::services::SwarmFactory>,
    /// Servicios que el arnés sustituye por dobles que fallan (`degrade:`).
    pub degrade: Vec<String>,
    /// Observador del orden de apagado (pruebas).
    pub observer: Arc<dyn ShutdownObserver>,
    /// `false` en los dobles: sin hilos de fondo ni registro real de atajos.
    pub background: bool,
    /// `false` cuando el arnés alimenta la captura desde fuera (F7-01a).
    pub mic_capture: bool,
    /// `false` filtra los acuses hablados de herramienta (costura del arnés).
    pub tool_acks: bool,
    /// Ejecuta el reintento de un subagente; `None` usa `ez retry` real.
    pub retry_runner: Option<super::services::RetryRunner>,
}

/// Estado completo compartido por todas las copias de [`super::VoiceApp`].
pub struct Inner {
    pub config: ConfigState,
    pub session: SessionState,
    pub turn: TurnState,
    pub audio: AudioState,
    pub speech: SpeechState,
    pub chat: ChatState,
    pub agents: AgentsState,
    pub ui: UiState,
    pub runtime: RuntimeState,
}
