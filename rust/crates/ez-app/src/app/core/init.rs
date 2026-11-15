//! Construcción del estado compartido y arranque de `__init__` (F6-01).

use super::callbacks::{build_watcher, saved_llm, schedule_tick, MessengerHolder};
use std::path::Path;

use super::*;

impl VoiceApp {
    /// Arranque con las piezas ya construidas (dobles de prueba incluidos).
    pub fn with_services(
        cfg: VoiceConfig,
        tasks_path: Option<PathBuf>,
        mut parts: AppParts,
    ) -> Self {
        let prepared = prepare_start(&cfg, &mut parts);
        let herdr = parts
            .herdr
            .take()
            .unwrap_or_else(|| Arc::new(Herdr::new(cfg.clone())));
        let watched = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let hooks: Arc<RwLock<Arc<dyn AppHooks>>> = Arc::new(RwLock::new(Arc::new(NullHooks)));
        let watcher = Arc::new(build_watcher(
            &herdr,
            &prepared.cfg,
            &watched,
            Arc::clone(&hooks),
        ));
        let tasks = Arc::new(TaskRegistry::new(
            tasks_path.unwrap_or_else(|| config_path().with_file_name("tasks.json")),
        ));
        let latency_path = config_path()
            .parent()
            .map(|parent| parent.join("logs").join("latency.jsonl"));
        let mic_info = {
            let mic = parts.mic.lock();
            (mic.device_name.clone(), mic.device_index, mic.rate)
        };
        let inner = Inner {
            config: config_state(&cfg, Arc::clone(&prepared.cfg), prepared.config_file),
            session: session_state(),
            turn: turn_state(),
            audio: audio_state(Arc::clone(&parts.mic)),
            speech: speech_state(Arc::clone(&parts.speaker)),
            chat: chat_state(prepared.chat, prepared.contexts),
            agents: agents_state(tasks, herdr, watcher, prepared.messenger, watched),
            ui: ui_state(
                prepared.ui,
                prepared.hud,
                prepared.events,
                prepared.hotkeys,
                prepared.menu,
            ),
            runtime: runtime_state(&mut parts, latency_path, hooks),
        };
        log(format!(
            "Micrófono: {} (índice {}, {} Hz)",
            mic_info.0,
            mic_info
                .1
                .map_or_else(|| "None".to_string(), |index| index.to_string()),
            mic_info.2
        ));
        let app = Self {
            inner: Arc::new(inner),
        };
        app.finish_start(parts);
        app
    }

    /// Lo que `__init__` hace después de construir los servicios.
    fn finish_start(&self, parts: AppParts) {
        self.apply_start_hud(parts.whisper_ready);
        self.apply_start_audio();
        self.apply_start_agents(parts.build_swarm, parts.build_agent);
        self.install_chat();
        self.install_hotkeys(parts.background);
        self.start_warm_workers(parts.background);
        self.schedule_start_timers();
        self.inner.runtime.observer.step("iniciado");
    }

    /// Barra, tema y aviso de whisper ausente.
    fn apply_start_hud(&self, whisper_ready: bool) {
        let cfg = self.config();
        if !whisper_ready {
            self.inner.ui.hud.set_state("error");
            self.inner.ui.events.put(AppEvent::Error(
                "No encuentro whisper. Revisa la carpeta de whisper en la configuración."
                    .to_string(),
                None,
            ));
        }
        self.inner.ui.hud.set_custom_colors(
            &cfg_text(&cfg, "theme_primary", "#1FAE9F"),
            &cfg_text(&cfg, "theme_secondary", "#8FE3C0"),
            &cfg_text(&cfg, "theme_accent", "#FF8A65"),
            &cfg_text(&cfg, "theme_background", "#0E6F7C"),
        );
        self.inner
            .ui
            .hud
            .set_custom_base(&cfg_text(&cfg, "theme_base", "dark"));
        self.inner
            .ui
            .hud
            .set_theme(&cfg_text(&cfg, "theme", "auto"));
        self.inner.ui.hud.set_in_call(false);
        if !self.inner.config.hud_hidden.load(Ordering::SeqCst) {
            self.inner.ui.hud.show(IDLE);
        }
    }

    /// Smart Turn y el resumen del VAD.
    fn apply_start_audio(&self) {
        let cfg = self.config();
        let detector = self.create_turn_detector(&cfg);
        let turn = if detector.is_some() {
            format!(
                "fin de turno a {}s si la frase está completa",
                cfg_float(&cfg, "turn_quick_secs", 0.35)
            )
        } else {
            "sin Smart Turn".to_string()
        };
        log(format!(
            "VAD: silencio={}s, {turn}, min_rms={}, pre_roll={}s, barge_in={}",
            cfg_float(&cfg, "silence_secs", 1.3),
            cfg_float(&cfg, "min_rms", 0.0006),
            cfg_float(&cfg, "pre_roll_secs", 0.6),
            cfg_truthy(&cfg, "barge_in", true),
        ));
        *self.inner.audio.turn_detector.lock() = detector;
    }

    /// Enjambre, runtime, mensajero y contexto reconstruido.
    // DECISION: el chat del HUD se conecta en `chat_wiring` (F6-08); aquí solo
    // se deja el mensajero y el contexto listos.
    fn apply_start_agents(&self, build_swarm: bool, build_agent: bool) {
        let cfg = self.config();
        if build_swarm {
            *self.inner.agents.swarm.lock() = self.create_swarm();
        }
        let agent = if build_agent {
            self.create_agent_runtime(&cfg)
        } else {
            None
        };
        let runtime = agent
            .as_ref()
            .map(|agent| Arc::new(agent.clone()) as Arc<dyn AgentRuntimeApi>);
        *self.inner.agents.agent.lock() = agent;
        let messenger = self.create_messenger(&cfg, runtime);
        *self.inner.agents.messenger.lock() = Some(messenger);
        if !self.inner.chat.chat.snapshot().1.is_empty() {
            self.inner.chat.contexts.lock().rebuild(None);
        }
    }

    /// Registra (o solo construye, en pruebas) los atajos.
    fn install_hotkeys(&self, background: bool) {
        let hotkeys = self.build_hotkeys();
        if background {
            for hotkey in &hotkeys {
                hotkey.start();
            }
        }
        *self.inner.ui.hotkeys.lock() = hotkeys;
    }

    /// Kokoro y whisper se precalientan en sus hilos (`ezagent-warm-*`).
    fn start_warm_workers(&self, background: bool) {
        if !background {
            return;
        }
        let app = self.clone();
        self.start_worker("ezagent-warm-tts", move || app.warm_tts());
        let app = self.clone();
        self.start_worker("ezagent-warm-stt", move || app.warm_stt());
        let herdr = Arc::clone(&self.inner.agents.herdr);
        self.start_worker("ez-herdr-install", move || {
            herdr.ensure_installed();
        });
    }

    /// `root.after(120, _report_hotkeys)` y `root.after(30, _tick)`.
    fn schedule_start_timers(&self) {
        let app = self.clone();
        self.inner
            .ui
            .ui
            .schedule(Duration::from_millis(120), move || app.report_hotkeys());
        schedule_tick(self);
    }
}

/// Piezas creadas antes del estado: colas, barra, chat y contexto.
struct Prepared {
    cfg: Arc<RwLock<VoiceConfig>>,
    config_file: PathBuf,
    ui: UiLoop,
    events: Arc<Queue<AppEvent>>,
    hotkeys: Arc<Queue<String>>,
    menu: Arc<Queue<MenuEvent>>,
    hud: Arc<DropUI>,
    chat: Arc<ChatModel>,
    messenger: MessengerSlot,
    contexts: ChatContexts,
}

fn prepare_start(cfg: &VoiceConfig, parts: &mut AppParts) -> Prepared {
    let config_file = parts.config_file.take().unwrap_or_else(config_path);
    let cfg_cell = Arc::new(RwLock::new(cfg.clone()));
    let ui = UiLoop::new();
    let events = Queue::new(Some(ui.clone()));
    let hotkeys = Queue::new(Some(ui.clone()));
    let menu = Queue::new(Some(ui.clone()));
    let hud = build_hud(parts, &cfg_cell, &config_file, &events, &menu);
    let chat = build_chat(parts, cfg);
    let messenger: MessengerSlot = Arc::new(Mutex::new(None));
    let contexts = build_contexts(&chat, &messenger);
    Prepared {
        cfg: cfg_cell,
        config_file,
        ui,
        events,
        hotkeys,
        menu,
        hud,
        chat,
        messenger,
        contexts,
    }
}

fn build_hud(
    parts: &mut AppParts,
    cfg: &Arc<RwLock<VoiceConfig>>,
    config_file: &Path,
    events: &Arc<Queue<AppEvent>>,
    menu_events: &Arc<Queue<MenuEvent>>,
) -> Arc<DropUI> {
    if let Some(hud) = parts.hud.take() {
        return hud;
    }
    let host = Arc::new(HudHost {
        events: Arc::clone(events),
        menu_events: Arc::clone(menu_events),
    });
    let hud_config = Arc::new(HudConfig {
        cfg: Arc::clone(cfg),
        config_file: config_file.to_path_buf(),
    });
    // F6-07 conecta el rasterizador real de iconos (F2-11).
    let icons = Arc::new(crate::app::live::LiveIcons) as Arc<dyn ez_hud::paint::peek::IconPainter>;
    create_ui(host, Some(hud_config), Some(icons))
}

fn build_chat(parts: &mut AppParts, cfg: &VoiceConfig) -> Arc<ChatModel> {
    parts.chat.take().unwrap_or_else(|| {
        let store = Arc::new(ChatStore::new(None));
        Arc::new(ChatModel::new(
            Some(store),
            cfg_truthy(cfg, "chat_voice", false),
        ))
    })
}

fn build_contexts(chat: &Arc<ChatModel>, slot: &MessengerSlot) -> ChatContexts {
    let slot = Arc::clone(slot);
    ChatContexts::new(
        Arc::clone(chat),
        Box::new(move || {
            slot.lock()
                .clone()
                .map(|messenger| Arc::new(MessengerHolder(messenger)) as Arc<dyn ContextHolder>)
        }),
    )
}

fn config_state(
    cfg: &VoiceConfig,
    cfg_cell: Arc<RwLock<VoiceConfig>>,
    config_file: PathBuf,
) -> ConfigState {
    ConfigState {
        cfg: cfg_cell,
        config_file,
        saved_llm: Mutex::new(saved_llm(cfg)),
        saved_stt_model: RwLock::new(cfg_text(cfg, "stt_model", "")),
        saved_stt_engine: RwLock::new(cfg_text(cfg, "stt_engine", "")),
        llm_models: Mutex::new(LlmModels::default()),
        devices_cache: Mutex::new(Vec::new()),
        menu_saved: Mutex::new(None),
        hud_hidden: std::sync::atomic::AtomicBool::new(cfg_truthy(cfg, "hud_hidden", false)),
    }
}

fn session_state() -> SessionState {
    SessionState {
        state: RwLock::new(IDLE.to_string()),
        in_call: std::sync::atomic::AtomicBool::new(false),
        muted: std::sync::atomic::AtomicBool::new(false),
        user_speaking: std::sync::atomic::AtomicBool::new(false),
        ignore_utterance: std::sync::atomic::AtomicBool::new(false),
        last_summary: RwLock::new(String::new()),
        mute_until: Mutex::new(0.0),
        shutdown_done: std::sync::atomic::AtomicBool::new(false),
        restarting: std::sync::atomic::AtomicBool::new(false),
    }
}

fn turn_state() -> TurnState {
    TurnState {
        last_turn_audio: Mutex::new(None),
        resume_pcm: Mutex::new(None),
        last_voice_user_id: Mutex::new(None),
        merge_user_id: Mutex::new(None),
        close_after_answer: std::sync::atomic::AtomicBool::new(false),
        close_mode: RwLock::new(String::new()),
        wake_lock: Mutex::new(()),
        post_tts_until: Mutex::new(0.0),
        post_tts_ignore: std::sync::atomic::AtomicBool::new(false),
        call_stop: Mutex::new(None),
        call_thread: Mutex::new(None),
        turn_gen: std::sync::atomic::AtomicI64::new(0),
        turn_started: Mutex::new(0.0),
        turn_timing: Mutex::new(None),
        stt: Mutex::new(state::SttQueue::default()),
        confirm_token: RwLock::new(String::new()),
    }
}

fn audio_state(mic: Arc<Mutex<MicCapture>>) -> AudioState {
    AudioState {
        mic,
        reset_capture: Arc::new(Flag::new()),
        turn_detector: Mutex::new(None),
    }
}

fn speech_state(speaker: Arc<dyn SpeakerApi>) -> SpeechState {
    SpeechState {
        speaker,
        tts_stop: Mutex::new(None),
        busy_speaking: Arc::new(Flag::new()),
        busy_stop: Mutex::new(None),
        busy_barged: std::sync::atomic::AtomicBool::new(false),
        speaking_kind: RwLock::new(None),
    }
}

fn chat_state(chat: Arc<ChatModel>, contexts: ChatContexts) -> ChatState {
    ChatState {
        chat,
        contexts: Mutex::new(contexts),
        condenser: Mutex::new(None),
        condenser_client: Mutex::new(None),
    }
}

fn agents_state(
    tasks: Arc<TaskRegistry>,
    herdr: Arc<Herdr>,
    watcher: Arc<Watcher>,
    messenger: MessengerSlot,
    watched: Arc<Mutex<std::collections::HashSet<String>>>,
) -> AgentsState {
    AgentsState {
        tasks,
        herdr,
        watcher,
        swarm: Mutex::new(None),
        agent: Mutex::new(None),
        messenger,
        watched,
        deliver: Mutex::new(std::collections::HashSet::new()),
        sent: Mutex::new(std::collections::HashMap::new()),
        prelaunch: Mutex::new(std::collections::HashMap::new()),
        announce_queue: Mutex::new(Vec::new()),
    }
}

fn ui_state(
    ui: UiLoop,
    hud: Arc<DropUI>,
    events: Arc<Queue<AppEvent>>,
    hotkey_events: Arc<Queue<String>>,
    menu_events: Arc<Queue<MenuEvent>>,
) -> UiState {
    UiState {
        ui,
        hud,
        events,
        hotkey_events,
        menu_events,
        tray: Mutex::new(None),
        hotkeys: Mutex::new(Vec::new()),
    }
}

fn runtime_state(
    parts: &mut AppParts,
    latency_path: Option<PathBuf>,
    hooks: Arc<RwLock<Arc<dyn AppHooks>>>,
) -> RuntimeState {
    RuntimeState {
        workers: Mutex::new(Vec::new()),
        latency: Mutex::new(LatencyStats::new(50, latency_path)),
        whisper_proc: Mutex::new(parts.whisper_proc.take()),
        hooks,
        observer: parts
            .observer
            .take()
            .unwrap_or_else(|| Arc::new(NullObserver)),
        background: parts.background,
    }
}
