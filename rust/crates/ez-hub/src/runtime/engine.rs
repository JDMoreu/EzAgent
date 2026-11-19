//! Implementacion de HubRuntime (arranque, agentes principales y arbol).

use super::*;

impl HubRuntime {
    /// Arranca el motor en modo normal. Nunca se llama en `--demo`.
    pub fn start(data_dir: &Path) -> Result<HubRuntime, String> {
        let spaces = load_spaces(&data_dir.join("espacios.json"))?;
        let profiles =
            ez_team::profiles::load_or_init(data_dir).map_err(|error| error.to_string())?;
        let subscriptions =
            ez_team::subscriptions::load(data_dir).map_err(|error| error.to_string())?;
        let exe_dir = exe_directory()?;
        let Some((client, hc_path)) = connect_hc()? else {
            return Ok(Self::degraded(spaces, data_dir, HC_MISSING));
        };
        let ez_ok = exe_dir.join(exe_name()).is_file();
        let tokens = Arc::new(Tokens::new());
        let url = Arc::new(RwLock::new(None::<String>));
        let (launcher, arbiter) =
            build_engine(data_dir, &spaces, profiles, client, &tokens, &url, exe_dir)?;
        let elephant_tokens = ElephantTokens::load(data_dir.join("elefantes.json"));
        let principales = Principales::load(data_dir.join("principales.json"));
        register_elephant_tokens(&elephant_tokens, &tokens, &arbiter, &spaces);
        let handler = Arc::new(ArbiterHandler::new(
            Arc::clone(&arbiter) as Arc<dyn handler::ArbiterApi>,
            launcher.profiles.clone(),
            subscriptions,
            spaces.clone(),
        ));
        let need_relaunch = Arc::new(AtomicBool::new(false));
        let service_url = start_server(Arc::clone(&handler), &tokens, &need_relaunch)?;
        *url.write() = Some(service_url.clone());
        let event_log = Arc::new(Mutex::new(Vec::new()));
        collect_arbiter_events(&arbiter, Arc::clone(&event_log))?;
        let changed = unbounded();
        events::spawn(
            &hc_path,
            Arc::clone(&arbiter),
            Arc::clone(&launcher),
            changed.0.clone(),
        )?;
        let status = if ez_ok {
            RuntimeStatus::normal(service_url)
        } else {
            RuntimeStatus::degraded(
                "No encontré ez.exe junto a la ventana; el comando `ez` no funcionará hasta reinstalar EzAgent.",
            )
        };
        Ok(HubRuntime {
            status,
            spaces,
            tokens,
            launcher: Some(launcher),
            arbiter: Some(arbiter),
            handler: Some(handler),
            elephant_tokens,
            principales,
            need_relaunch,
            relaunched: Mutex::new(BTreeSet::new()),
            changed,
            event_log,
        })
    }

    /// Motor caído antes de conectarse, con un motivo para la vista.
    pub fn failed(data_dir: &Path, reason: &str) -> HubRuntime {
        Self::degraded(Vec::new(), data_dir, reason)
    }

    fn degraded(spaces: Vec<SpaceConfig>, data_dir: &Path, reason: &str) -> HubRuntime {
        HubRuntime {
            status: RuntimeStatus::degraded(reason),
            spaces,
            tokens: Arc::new(Tokens::new()),
            launcher: None,
            arbiter: None,
            handler: None,
            elephant_tokens: ElephantTokens::load(data_dir.join("elefantes.json")),
            principales: Principales::load(data_dir.join("principales.json")),
            need_relaunch: Arc::new(AtomicBool::new(false)),
            relaunched: Mutex::new(BTreeSet::new()),
            changed: unbounded(),
            event_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Estado actual del motor para la vista.
    pub fn status(&self) -> RuntimeStatus {
        self.status.clone()
    }

    /// Canal de avisos para la vista; el hilo de Tauri emite `hub://changed`.
    pub fn subscribe_changed(&self) -> Receiver<()> {
        self.changed.1.clone()
    }

    /// Crea un agente principal con su worktree, guarda su elección y lo lanza.
    pub fn main_agent_create(
        &self,
        space: &str,
        name: &str,
        choice: &MainChoice,
    ) -> Result<MainAgentInfo, String> {
        let arbiter = self.arbiter()?;
        let agent = arbiter
            .create_main_agent(space, name)
            .map_err(|rejection| rejection.message)?;
        self.principales
            .set(&agent.key.elephant, choice)
            .map_err(|error| error.to_string())?;
        let address = self.ensure_main_agent(space, &agent.key.elephant)?;
        Ok(MainAgentInfo {
            id: agent.key.elephant.clone(),
            name: agent.name.clone(),
            worktree: agent.worktree.to_string_lossy().into_owned(),
            branch: agent.branch.clone(),
            base_branch: agent.base_branch.clone(),
            address,
        })
    }

    /// Quita un agente principal, su elección y su token.
    pub fn main_agent_remove(&self, space: &str, elephant: &str) -> Result<(), String> {
        let arbiter = self.arbiter()?;
        let key = ElephantKey {
            space: space.to_string(),
            elephant: elephant.to_string(),
        };
        arbiter
            .remove_main_agent(&key)
            .map_err(|rejection| rejection.message)?;
        if let Some(token) = self.elephant_tokens.get(elephant) {
            self.tokens.revoke(&token);
        }
        self.elephant_tokens.remove(elephant);
        self.principales.remove(elephant);
        Ok(())
    }

    /// Dirección hc de un agente principal; lo lanza si no está vivo.
    pub fn ensure_main_agent(&self, space: &str, elephant: &str) -> Result<String, String> {
        let launcher = self.launcher()?;
        let agent = self.main_agent(space, elephant)?;
        let choice = self.choice_for(&agent)?;
        let labels = main_labels(space, elephant);
        let alive: Vec<_> = launcher
            .list(&labels)?
            .into_iter()
            .filter(|terminal| terminal.state.eq_ignore_ascii_case("live"))
            .collect();
        if let Some(terminal) = alive.first() {
            if self.must_relaunch(elephant) {
                launcher.kill(&terminal.address)?;
            } else {
                return Ok(terminal.address.clone());
            }
        }
        self.launch_main(&agent, &choice, space, elephant)
    }

    fn launch_main(
        &self,
        agent: &MainAgent,
        choice: &MainChoice,
        space: &str,
        elephant: &str,
    ) -> Result<String, String> {
        let launcher = self.launcher()?;
        let token = self
            .elephant_tokens
            .get(elephant)
            .filter(|token| !token.is_empty())
            .unwrap_or_else(new_token);
        self.tokens.register(
            &token,
            Caller::Elephant {
                space: space.to_string(),
                elephant: elephant.to_string(),
            },
        );
        match launcher.launch_main(agent, choice, &token) {
            Ok(address) => {
                self.elephant_tokens.set(elephant, &token)?;
                self.relaunched.lock().insert(elephant.to_string());
                Ok(address)
            }
            Err(error) => {
                self.tokens.revoke(&token);
                Err(error)
            }
        }
    }

    fn main_agent(&self, space: &str, elephant: &str) -> Result<MainAgent, String> {
        self.arbiter()?
            .main_agents(space)
            .into_iter()
            .find(|agent| agent.key.elephant == elephant)
            .ok_or_else(|| format!("No conozco el agente principal «{elephant}»."))
    }

    fn choice_for(&self, agent: &MainAgent) -> Result<MainChoice, String> {
        self.principales
            .get(&agent.key.elephant)
            .or_else(|| {
                self.spaces
                    .iter()
                    .find(|space| space.id == agent.key.space)
                    .and_then(|space| space.suggested.clone())
            })
            .ok_or_else(|| {
                format!(
                    "El agente principal «{}» necesita agente y modelo.",
                    agent.key.elephant
                )
            })
    }

    fn must_relaunch(&self, elephant: &str) -> bool {
        self.need_relaunch.load(Ordering::SeqCst) && !self.relaunched.lock().contains(elephant)
    }

    fn launcher(&self) -> Result<&Arc<RuntimeLauncher>, String> {
        self.launcher
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())
    }

    fn arbiter(&self) -> Result<&Arc<Arbiter>, String> {
        self.arbiter
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())
    }

    /// Árbol de agentes y subagentes del espacio, con sus direcciones hc.
    pub fn space_tree(&self, space: &str) -> Result<SpaceTree, String> {
        let terminals = self.launcher()?.list(&BTreeMap::new())?;
        let agents = self.arbiter()?.main_agents(space);
        let choice = |elephant: &str| {
            self.principales.get(elephant).or_else(|| {
                self.spaces
                    .iter()
                    .find(|item| item.id == space)
                    .and_then(|item| item.suggested.clone())
            })
        };
        let title = |elephant: &str| {
            self.arbiter()
                .ok()
                .and_then(|arbiter| {
                    arbiter.snapshot_main(&ElephantKey {
                        space: space.to_string(),
                        elephant: elephant.to_string(),
                    })
                })
                .and_then(|snapshot| {
                    snapshot
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        };
        Ok(tree::build(
            space,
            &agents,
            &terminals,
            choice,
            title,
            &tree::role_names(),
        ))
    }

    /// Dirección hc de un nodo; lanza el agente principal si aún no tiene terminal.
    pub fn node_address(&self, space: &str, node: &str) -> Result<String, String> {
        let tree = self.space_tree(space)?;
        if let Some(address) = find_address(&tree.nodes, node) {
            return Ok(address);
        }
        if self
            .arbiter()?
            .main_agents(space)
            .iter()
            .any(|agent| agent.key.elephant == node)
        {
            return self.ensure_main_agent(space, node);
        }
        Err(format!("El nodo «{node}» no tiene una terminal abierta."))
    }

    /// Envía texto crudo a la dirección hc de la pestaña activa.
    pub fn agent_send(&self, address: &str, text: &str) -> Result<(), String> {
        self.launcher()?.send_text(address, text)
    }

    /// Traza y tarjetas de un agente principal, tal como las emite el árbitro.
    pub fn hub_state(&self, space: &str, elephant: &str) -> Result<Value, String> {
        let events: Vec<_> = self
            .event_log
            .lock()
            .iter()
            .filter(|event| event.space == space && event.elephant.as_deref() == Some(elephant))
            .cloned()
            .collect();
        let cards = cards_from(&events);
        let snapshot = self.arbiter()?.snapshot_main(&ElephantKey {
            space: space.to_string(),
            elephant: elephant.to_string(),
        });
        Ok(json!({ "eventos": events, "tarjetas": cards, "tarea": snapshot }))
    }

    /// Resuelve una tarjeta del árbitro (incluye `merge` y `keep` de integración).
    pub fn card_action(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Value, String> {
        let outcome = self
            .arbiter()?
            .card_action(card, action, comment)
            .map_err(|rejection| rejection.message)?;
        Ok(json!({ "message": outcome.message, "data": outcome.data }))
    }

    /// Los mismos datos que `ez roles` para el agente principal dado.
    pub fn roles_list(&self, space: &str, elephant: &str) -> Result<Value, String> {
        let handler = self
            .handler
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())?;
        Ok(Value::String(
            handler
                .roles_message(space, elephant)
                .map_err(|error| error.to_string())?,
        ))
    }

    /// Los mismos datos que `ez inventory`.
    pub fn inventory(&self) -> Result<Value, String> {
        let handler = self
            .handler
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())?;
        let reply = handler.inventory_reply();
        Ok(json!({ "message": reply.message, "data": reply.data }))
    }

    /// Peces vivos de un espacio (legado de ECO-6a), con su dirección y papel.
    pub fn space_fish(&self, space: &str) -> Result<Vec<FishInfo>, String> {
        let launcher = self.launcher()?;
        let terminals = launcher.list(&BTreeMap::from([("ez".to_string(), "1".to_string())]))?;
        Ok(terminals
            .into_iter()
            .filter(|terminal| {
                terminal.labels.get("ez_space").map(String::as_str) == Some(space)
                    && terminal.labels.contains_key("ez_fish")
            })
            .map(|terminal| FishInfo {
                status: terminal
                    .status
                    .as_ref()
                    .map(|status| status.state.clone())
                    .unwrap_or_else(|| terminal.state.clone()),
                address: terminal.address,
                fish: label(&terminal.labels, "ez_fish"),
                role: label(&terminal.labels, "ez_role"),
                agent: label(&terminal.labels, "ez_agent"),
                model: label(&terminal.labels, "ez_model"),
            })
            .collect())
    }

    /// Envía texto crudo al agente principal por omisión (legado de ECO-6a).
    pub fn elephant_send(&self, space: &str, text: &str) -> Result<(), String> {
        let address = self.ensure_main_agent(space, &ElephantKey::default_for(space).elephant)?;
        self.agent_send(&address, text)
    }

    #[cfg(test)]
    pub(crate) fn with_parts(
        spaces: Vec<SpaceConfig>,
        launcher: Arc<RuntimeLauncher>,
        arbiter: Arc<Arbiter>,
        tokens: Arc<Tokens>,
        elephant_tokens: ElephantTokens,
        principales: Principales,
        need_relaunch: bool,
    ) -> HubRuntime {
        HubRuntime {
            status: RuntimeStatus::normal("http://127.0.0.1:1"),
            spaces,
            tokens,
            launcher: Some(launcher),
            arbiter: Some(arbiter),
            handler: None,
            elephant_tokens,
            principales,
            need_relaunch: Arc::new(AtomicBool::new(need_relaunch)),
            relaunched: Mutex::new(BTreeSet::new()),
            changed: unbounded(),
            event_log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
