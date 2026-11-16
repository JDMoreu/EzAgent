//! Adaptadores y callbacks del mensajero y del núcleo agéntico (F6-01).

use ez_core::pyvalue::text_or_empty;

use super::*;

/// `Messenger` como `ContextHolder` del chat (F3-14).
pub(super) struct MessengerHolder(pub(super) Messenger);

impl ContextHolder for MessengerHolder {
    fn reset_history(&self) {
        Messenger::reset_history(&self.0);
    }

    fn export_context(&self) -> (Vec<ModelMessage>, String, String) {
        let (messages, summary, last_user) = Messenger::export_context(&self.0);
        let converted = messages.iter().filter_map(model_message).collect();
        (converted, summary, last_user)
    }

    fn import_context(&self, messages: Vec<ModelMessage>, summary: &str, last_user: &str) {
        let values = messages
            .into_iter()
            .map(|message| json!({"role": message.role.as_str(), "content": message.content}))
            .collect();
        Messenger::import_context(&self.0, values, summary, last_user);
    }
}

/// DECISION: el mensajero guarda mensajes con partes y herramientas; el chat
/// solo representa rol y texto, así que la conversión descarta el resto.
pub(super) fn model_message(value: &Value) -> Option<ModelMessage> {
    let role = match value.get("role")?.as_str()? {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => return None,
    };
    let content = match value.get("content")? {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };
    Some(ModelMessage { role, content })
}

/// `Herdr` visto por el núcleo agéntico (`ez-herdr` no depende de `ez-agent`).
///
/// `Herdr` hereda de `AgentDriver` en Python; aquí el catálogo se construye
/// sobre la copia de configuración que `Herdr` ya guarda y la sesión usa el
/// mismo `Herdr` como `SessionHost`.
pub(super) struct AgentHerdr(pub(super) Arc<Herdr>);

impl ez_agent::runtime::HerdrApi for AgentHerdr {
    fn catalog_text(&self) -> String {
        ez_herdr::driver::AgentDriver::new(Arc::clone(&self.0), self.0.config()).catalog_text()
    }

    fn context_text(&self) -> String {
        self.0.context_text()
    }

    fn list_agents(&self) -> Vec<Value> {
        self.0
            .list_agents(20.0)
            .map(|agents| agents.into_iter().map(Value::Object).collect())
            .unwrap_or_default()
    }
}

/// `ez_swarm::SwarmStack` visto por el runtime (`active()` sin error).
pub(super) struct SwarmActive(pub(super) Arc<SwarmStack>);

impl AgentSwarmStack for SwarmActive {
    fn active(&self) -> bool {
        self.0.active().unwrap_or(false)
    }
}

use ez_audio::barge_in::BargeInMode;

pub(super) fn mic_capture(cfg: &VoiceConfig) -> MicCapture {
    let index = cfg
        .get("input_device_index")
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok());
    // El nombre es el que sobrevive a los índices: los de PortAudio y los de
    // cpal apuntan a micrófonos distintos con el mismo `voice.json`.
    let name = text_or_empty(cfg.get("input_device_name"));
    let name = if name.is_empty() { None } else { Some(name) };
    MicCapture::new_named(
        index,
        name.as_deref(),
        VadSettings {
            min_rms: cfg_float(cfg, "min_rms", 0.0006),
            silence_secs: cfg_float(cfg, "silence_secs", 1.3),
            pre_roll_secs: cfg_float(cfg, "pre_roll_secs", 0.6),
            min_speech_secs: cfg_float(cfg, "min_speech_secs", 0.2),
            max_utterance_secs: cfg_float(cfg, "max_utterance_secs", 30.0),
            noise_multiplier: cfg_float(cfg, "noise_multiplier", 1.3),
            turn_quick_secs: cfg_float(cfg, "turn_quick_secs", 0.35),
            barge_in_rms: cfg_float(cfg, "barge_in_rms", 0.008),
            barge_in_secs: cfg_float(cfg, "barge_in_secs", 0.25),
            barge_in_mode: barge_in_mode(cfg),
        },
    )
}

/// `barge_in_mode` de `voice.json`: «fixed» es el Python; lo demás, adaptativo.
pub(crate) fn barge_in_mode(cfg: &VoiceConfig) -> BargeInMode {
    match cfg_text(cfg, "barge_in_mode", "adaptive").as_str() {
        "fixed" => BargeInMode::Fixed,
        _ => BargeInMode::Adaptive,
    }
}

pub(super) fn saved_llm(cfg: &VoiceConfig) -> SavedLlm {
    SavedLlm {
        provider: cfg_text(cfg, "llm_provider", ""),
        model: cfg_text(cfg, "llm_model", ""),
        base_url: cfg_text(cfg, "llm_base_url", ""),
        api_key: cfg_text(cfg, "api_key", ""),
        api_style: cfg_text(cfg, "llm_api_style", ""),
    }
}

pub(super) fn build_watcher(
    herdr: &Arc<Herdr>,
    cfg: &Arc<RwLock<VoiceConfig>>,
    watched: &Arc<Mutex<std::collections::HashSet<String>>>,
    hooks: Arc<RwLock<Arc<dyn AppHooks>>>,
) -> Watcher {
    let on_event: EventCallback = Arc::new({
        let hooks = Arc::clone(&hooks);
        move |address: &str, agent: &str, state: &str, reason: &str| {
            hooks.read().watcher_event(address, agent, state, reason);
        }
    });
    let should_announce: AnnounceCallback = Arc::new({
        let hooks = Arc::clone(&hooks);
        move |address: &str| hooks.read().should_announce(address)
    });
    let has_targets: TargetsCallback = Arc::new({
        let watched = Arc::clone(watched);
        let cfg = Arc::clone(cfg);
        move || !watched.lock().is_empty() || cfg_truthy(&cfg.read(), "announce_all", false)
    });
    Watcher::new(
        herdr.hc_path(),
        on_event,
        should_announce,
        None,
        Some(has_targets),
    )
}

pub(super) fn schedule_tick(app: &VoiceApp) {
    let next = app.clone();
    let app = app.clone();
    app.inner
        .ui
        .ui
        .schedule(Duration::from_millis(30), move || {
            next.hooks().tick();
            schedule_tick(&next);
        });
}

/// Callbacks sin valor y con un `bool`, con nombre corto.
pub(super) type VoidFn = Arc<dyn Fn() + Send + Sync>;
pub(super) type BoolFn = Arc<dyn Fn(bool) + Send + Sync>;
pub(super) type MuteFn = Arc<dyn Fn(i64) + Send + Sync>;
pub(super) type SpecFn = Arc<dyn Fn(&Value) + Send + Sync>;
pub(super) type RememberFn = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

pub(super) fn upgrade<F: FnOnce(&VoiceApp)>(weak: &Weak<Inner>, action: F) {
    if let Some(inner) = weak.upgrade() {
        action(&VoiceApp { inner });
    }
}

pub(super) fn upgrade_value<F: FnOnce(&VoiceApp) -> T, T: Default>(
    weak: &Weak<Inner>,
    action: F,
) -> T {
    weak.upgrade()
        .map(|inner| action(&VoiceApp { inner }))
        .unwrap_or_default()
}

pub(super) fn text_hook<F>(weak: &Weak<Inner>, action: F) -> TextFn
where
    F: Fn(&Arc<dyn AppHooks>, &str) + Send + Sync + 'static,
{
    let weak = weak.clone();
    Arc::new(move |text: &str| {
        upgrade(&weak, |app| action(&app.hooks(), text));
    })
}

pub(super) fn void_hook<F>(weak: &Weak<Inner>, action: F) -> VoidFn
where
    F: Fn(&Arc<dyn AppHooks>) + Send + Sync + 'static,
{
    let weak = weak.clone();
    Arc::new(move || {
        upgrade(&weak, |app| action(&app.hooks()));
    })
}

pub(super) fn bool_hook<F>(weak: &Weak<Inner>, action: F) -> BoolFn
where
    F: Fn(&Arc<dyn AppHooks>, bool) + Send + Sync + 'static,
{
    let weak = weak.clone();
    Arc::new(move |flag: bool| {
        upgrade(&weak, |app| action(&app.hooks(), flag));
    })
}

pub(super) fn mute_hook(weak: &Weak<Inner>) -> MuteFn {
    let weak = weak.clone();
    Arc::new(move |minutes: i64| {
        upgrade(&weak, |app| app.hooks().mute(minutes));
    })
}

pub(super) fn spec_hook<F>(weak: &Weak<Inner>, action: F) -> SpecFn
where
    F: Fn(&Arc<dyn AppHooks>, &Value) + Send + Sync + 'static,
{
    let weak = weak.clone();
    Arc::new(move |value: &Value| {
        upgrade(&weak, |app| action(&app.hooks(), value));
    })
}

pub(super) fn remember_hook(weak: &Weak<Inner>) -> RememberFn {
    let weak = weak.clone();
    Arc::new(move |address: &str, agent: &str, task: &str| {
        upgrade(&weak, |app| {
            app.hooks().remember_task(address, agent, task);
        });
    })
}

pub(super) fn text_producer<F>(weak: &Weak<Inner>, action: F) -> TextProducer
where
    F: Fn(&VoiceApp) -> String + Send + Sync + 'static,
{
    let weak = weak.clone();
    Arc::new(move || upgrade_value(&weak, &action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ez_agent::runtime::HerdrApi;
    use ez_core::config::VoiceConfig;
    use ez_herdr::herdr::Herdr;
    use serde_json::json;

    #[test]
    fn barge_in_mode_defaults_to_adaptive_and_honours_fixed() {
        let mut cfg = VoiceConfig::new();
        assert_eq!(barge_in_mode(&cfg), BargeInMode::Adaptive);
        cfg.set("barge_in_mode", json!("fixed"));
        assert_eq!(barge_in_mode(&cfg), BargeInMode::Fixed);
        cfg.set("barge_in_mode", json!("desconocido"));
        assert_eq!(barge_in_mode(&cfg), BargeInMode::Adaptive);
    }

    #[test]
    fn agent_herdr_ports_the_catalog_and_context_texts() {
        let mut cfg = VoiceConfig::new();
        // Ruta absoluta inexistente: ninguna prueba lanza `hc` de verdad.
        cfg.set("hc_path", json!(r"C:\no-existe\hc.exe"));
        cfg.set("agents", json!({"claude": ["claude"]}));
        cfg.set(
            "agent_profiles",
            json!({"claude": {"label": "Claude Code", "model_args": ["--model"]}}),
        );
        let api = AgentHerdr(Arc::new(Herdr::new(cfg)));
        let catalog = api.catalog_text();
        assert!(catalog.starts_with("# Agentes disponibles (datos del catálogo)"));
        assert!(catalog.contains("- claude (Claude Code)"));
        assert!(catalog.contains("se puede fijar: modelo"));
        assert_eq!(api.context_text(), "");
    }
}
