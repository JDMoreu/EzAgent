//! Servicios que `__init__` crea bajo demanda (F6-01).

use super::callbacks::{
    bool_hook, mute_hook, remember_hook, spec_hook, text_hook, text_producer, void_hook,
    AgentHerdr, SwarmActive,
};
use super::*;

impl VoiceApp {
    /// `_create_turn_detector`: Smart Turn en segundo plano.
    pub fn create_turn_detector(&self, cfg: &VoiceConfig) -> Option<Arc<SmartTurn>> {
        if !cfg_truthy(cfg, "smart_turn", true) {
            return None;
        }
        if self
            .inner
            .runtime
            .degrade
            .iter()
            .any(|service| service == "smart_turn")
        {
            log("No encuentro el modelo de Smart Turn.");
            super::trace::notify(&self.inner.runtime.trace, |trace| {
                trace.degraded("smart_turn");
            });
            return None;
        }
        let path = match ez_audio::turn::resolve_model_path() {
            Ok(path) => path,
            Err(error) => {
                log(format!("Smart Turn no disponible: {error}"));
                return None;
            }
        };
        let detector = Arc::new(SmartTurn::new(
            cfg_float(cfg, "turn_threshold", 0.5),
            &path.to_string_lossy(),
        ));
        let mic = self.inner.audio.mic();
        mic.set_turn_detector(Some(Arc::clone(&detector)));
        mic.set_turn_quick_secs(cfg_float(cfg, "turn_quick_secs", 0.35));
        Some(detector)
    }

    /// `_apply_turn_settings`: enciende, apaga o ajusta Smart Turn en vivo.
    pub fn apply_turn_settings(&self) {
        let cfg = self.config();
        if !cfg_truthy(&cfg, "smart_turn", true) {
            self.inner.audio.mic().set_turn_detector(None);
            return;
        }
        let current = self.inner.audio.turn_detector.lock().clone();
        match current {
            Some(detector) => {
                detector.set_threshold(cfg_float(&cfg, "turn_threshold", 0.5));
                self.inner.audio.mic().set_turn_detector(Some(detector));
            }
            None => {
                let detector = self.create_turn_detector(&cfg);
                *self.inner.audio.turn_detector.lock() = detector;
            }
        }
    }

    /// `_pending_block`: tareas vigiladas y enjambres activos para el prompt.
    pub fn pending_block(&self) -> String {
        let mut parts = vec![self.inner.agents.tasks.prompt_block()];
        let swarm = self.inner.agents.swarm.lock().clone();
        if let Some(swarm) = swarm {
            match swarm.prompt_block() {
                Ok(block) => parts.push(block),
                Err(error) => log(format!("No pude leer el estado del enjambre: {error}")),
            }
        }
        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `_create_swarm`: monta el modo enjambre; si falla, la app sigue sin él.
    pub fn create_swarm(&self) -> Option<Arc<SwarmStack>> {
        let base = self.inner.agents.tasks.path.with_file_name("swarm");
        let host = Arc::new(HerdrHost::new(Arc::clone(&self.inner.agents.herdr)))
            as Arc<dyn ez_swarm::supervisor::SwarmHost>;
        let events = Arc::clone(&self.inner.ui.events);
        let announce: ez_swarm::supervisor::Announce = Arc::new({
            let events = Arc::clone(&events);
            move |text: &str| {
                events.put(AppEvent::SwarmNotice(text.to_string(), false));
            }
        });
        let ask: ez_swarm::supervisor::Announce = Arc::new(move |text: &str| {
            events.put(AppEvent::SwarmNotice(text.to_string(), true));
        });
        if let Some(factory) = &self.inner.runtime.swarm_factory {
            return factory(super::services::SwarmArgs {
                base_dir: base,
                host,
                announce,
                ask: Some(ask),
                command: "ezs".to_string(),
            });
        }
        match SwarmStack::new(base, host, announce, Some(ask), "ezs") {
            Ok(stack) => Some(Arc::new(stack)),
            Err(error) => {
                log(format!("Modo enjambre no disponible: {error}"));
                None
            }
        }
    }

    /// `_create_agent_runtime`: núcleo agéntico, o `None` si no está disponible.
    pub fn create_agent_runtime(&self, cfg: &VoiceConfig) -> Option<AgentRuntime> {
        let weak = Arc::downgrade(&self.inner);
        let swarm = self
            .inner
            .agents
            .swarm
            .lock()
            .clone()
            .map(|stack| Arc::new(SwarmActive(stack)) as Arc<dyn AgentSwarmStack>);
        let callbacks = RuntimeCallbacks {
            watch: text_hook(&weak, |hooks, text| hooks.watch(text)),
            request_close: void_hook(&weak, |hooks| hooks.request_close()),
            mic_mute: bool_hook(&weak, |hooks, muted| hooks.mic_mute(muted)),
            last_summary: text_producer(&weak, |app| app.last_summary()),
            mute: mute_hook(&weak),
            get_last_user_text: text_producer(&weak, |app| app.hooks().last_user_text()),
            remember_task: remember_hook(&weak),
            enqueue_chain: spec_hook(&weak, |hooks, value| hooks.enqueue_chain(value)),
            deliver_answer: text_hook(&weak, |hooks, text| hooks.deliver_answer(text)),
            prewarm_chain: spec_hook(&weak, |hooks, value| hooks.prewarm_chain(value)),
            async_launch: true,
            pending_tasks: text_producer(&weak, |app| app.pending_block()),
            swarm,
            on_confirm_ask: Some({
                let events = Arc::clone(&self.inner.ui.events);
                Arc::new(move |summary: &str, strong: bool, token: Option<&str>| {
                    events.put(AppEvent::ConfirmAsk(
                        summary.to_string(),
                        strong,
                        token.map(str::to_string),
                    ));
                })
            }),
            on_confirm_done: Some({
                let events = Arc::clone(&self.inner.ui.events);
                Arc::new(move || events.put(AppEvent::ConfirmDone))
            }),
            on_warn: Some({
                let events = Arc::clone(&self.inner.ui.events);
                Arc::new(move |message: &str| {
                    events.put(AppEvent::AgentWarn(message.to_string()));
                })
            }),
        };
        let herdr: Arc<dyn ez_agent::runtime::HerdrApi> =
            Arc::new(AgentHerdr(Arc::clone(&self.inner.agents.herdr)));
        match AgentRuntime::new(cfg.clone(), Some(herdr), callbacks, None) {
            Ok(runtime) => {
                self.install_extra_tools(&runtime, &weak);
                log("Núcleo agéntico disponible.");
                Some(runtime)
            }
            Err(error) => {
                log(format!("Núcleo agéntico no disponible: {error}"));
                None
            }
        }
    }

    /// Grupos de herramientas que dependen de la app: Herdr y enjambre.
    ///
    /// `AgentRuntime::new` deja fuera los grupos que necesitan `ez-herdr` y
    /// `ez-swarm` (F4-09); el Python los construye dentro del runtime con los
    /// mismos callbacks de la app.
    fn install_extra_tools(&self, runtime: &AgentRuntime, weak: &Weak<Inner>) {
        let enqueue = weak.clone();
        let prewarm = weak.clone();
        let failed = weak.clone();
        let callbacks = ez_agent::tools_herdr::HerdrCallbacks {
            watch: text_hook(weak, |hooks, text| hooks.watch(text)),
            request_close: void_hook(weak, |hooks| hooks.request_close()),
            mic_mute: bool_hook(weak, |hooks, muted| hooks.mic_mute(muted)),
            last_summary: text_producer(weak, |app| app.last_summary()),
            mute: mute_hook(weak),
            get_last_user_text: text_producer(weak, |app| app.hooks().last_user_text()),
            remember_task: remember_hook(weak),
            enqueue_chain: Arc::new(move |value: Value| {
                super::callbacks::upgrade(&enqueue, |app| app.hooks().enqueue_chain(&value));
            }),
            deliver_answer: text_hook(weak, |hooks, text| hooks.deliver_answer(text)),
            prewarm_chain: Arc::new(move |value: Value| {
                super::callbacks::upgrade(&prewarm, |app| app.hooks().prewarm_chain(&value));
            }),
            async_launch: true,
            launch_failed: Arc::new(move |address: &str, agent: &str, error: &str| {
                super::callbacks::upgrade(&failed, |app| {
                    if let Some(runtime) = app.inner.agents.agent.lock().as_ref() {
                        runtime.launch_failed(address, agent, error);
                    }
                });
            }),
        };
        let herdr = Arc::clone(&self.inner.agents.herdr);
        runtime.add_tool_group(Arc::new(move |_: &AgentRuntime| {
            ez_agent::tools_herdr::build_herdr_tools(Arc::clone(&herdr), callbacks.clone())
        }));
        let swarm = self.inner.agents.swarm.lock().clone();
        if let Some(stack) = swarm {
            runtime.add_tool_group(Arc::new(move |_: &AgentRuntime| {
                ez_agent::tools_swarm::build_swarm_tools(Some(Arc::clone(&stack)))
            }));
        }
    }

    /// `Messenger(...)` con los avisos de contexto y de turno ocupado.
    pub fn create_messenger(
        &self,
        cfg: &VoiceConfig,
        runtime: Option<Arc<dyn AgentRuntimeApi>>,
    ) -> Messenger {
        let weak = Arc::downgrade(&self.inner);
        let callbacks = MessengerCallbacks {
            last_summary: Some(text_producer(&weak, |app| app.last_summary())),
            watch: Some(text_hook(&weak, |hooks, text| hooks.watch(text))),
            mute: Some(mute_hook(&weak)),
            request_close: Some(void_hook(&weak, |hooks| hooks.request_close())),
            mic_mute: Some(bool_hook(&weak, |hooks, muted| hooks.mic_mute(muted))),
        };
        let messenger = Messenger::new(cfg.clone(), callbacks, runtime);
        let chat = Arc::clone(self.inner.chat.chat.inner());
        messenger.set_on_context_event(Arc::new(move |text| {
            chat.add_note(text);
        }));
        let events = Arc::clone(&self.inner.ui.events);
        let turn = Arc::downgrade(&self.inner);
        messenger.set_on_busy(Arc::new(move |mode| {
            let generation = turn
                .upgrade()
                .map_or(0, |inner| inner.turn.turn_gen.load(Ordering::SeqCst));
            events.put(AppEvent::BusyNotice(generation, mode.to_string()));
        }));
        messenger
    }
}
