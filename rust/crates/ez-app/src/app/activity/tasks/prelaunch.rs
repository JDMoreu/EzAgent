//! Apertura anticipada de los destinos de una cadena (parte de F6-07).

use std::time::{Duration, Instant};

use ez_core::pyvalue::{py_strip, truthy};
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{NameKind, RunAgent};
use serde_json::{Map, Value};

use super::super::super::core::{log, VoiceApp};
use super::{optional, or_empty, raw_text};

impl VoiceApp {
    /// `_start_prelaunch(spec)`: abre ya el destino de una cadena.
    pub fn start_prelaunch(&self, spec: &Map<String, Value>) {
        let chain_id = or_empty(spec.get("chain_id"));
        let agent = py_strip(&or_empty(spec.get("agent"))).to_string();
        if chain_id.is_empty()
            || agent.is_empty()
            || spec.get("background").is_some_and(truthy)
            || spec.get("gui").is_some_and(truthy)
        {
            return;
        }
        {
            let mut registry = self.inner.agents.prelaunch.lock();
            if registry.contains_key(&chain_id) {
                return; // ya se está abriendo (calentado antes de lanzar el origen)
            }
            let mut entry = Map::new();
            entry.insert("address".to_string(), Value::String(String::new()));
            entry.insert("done".to_string(), Value::Bool(false));
            registry.insert(chain_id.clone(), entry);
        }
        let worker = self.clone();
        let spec = spec.clone();
        self.start_worker("ez-prelaunch", move || {
            worker.prelaunch_worker(&spec, &chain_id, &agent)
        });
    }

    /// El hilo `ez-prelaunch` de `_start_prelaunch`.
    fn prelaunch_worker(&self, spec: &Map<String, Value>, chain_id: &str, agent: &str) {
        let address = self.open_prelaunched(spec, agent);
        if let Some(error) = address.as_ref().err() {
            log(format!("No pude abrir {agent} por adelantado: {error}"));
        }
        let mut registry = self.inner.agents.prelaunch.lock();
        if let Some(entry) = registry.get_mut(chain_id) {
            let opened = address.unwrap_or_default();
            if !opened.is_empty() {
                entry.insert("address".to_string(), Value::String(opened));
            }
            entry.insert("done".to_string(), Value::Bool(true));
        }
    }

    /// `run_agent(agent, "", **kwargs)`: `Ok(address)` o el texto del error.
    fn open_prelaunched(&self, spec: &Map<String, Value>, agent: &str) -> Result<String, String> {
        let herdr = &self.inner.agents.herdr;
        // DECISION: el lanzamiento previo corre en segundo plano y no puede
        // preguntar; con un espacio dudoso no se abre nada y lo resolverá la
        // herramienta con su confirmación.
        let space = match optional(spec.get("space")) {
            Some(spoken) => match herdr.resolve_safely(NameKind::Space, &spoken) {
                Some(value) => Some(value),
                None => return Ok(String::new()),
            },
            None => None,
        };
        let cwd = optional(spec.get("cwd"));
        let name = optional(spec.get("name"));
        let model = optional(spec.get("model"));
        let effort = optional(spec.get("effort"));
        let driver = AgentDriver::new(std::sync::Arc::clone(herdr), herdr.config());
        let result = herdr
            .run_agent(
                driver,
                RunAgent {
                    agent,
                    task: "",
                    cwd: cwd.as_deref(),
                    name: name.as_deref(),
                    confirmed: true,
                    background: false,
                    gui: false,
                    space: space.as_deref(),
                    model: model.as_deref(),
                    effort: effort.as_deref(),
                    on_ready: None,
                    env: None,
                    path_prepend: None,
                    extra_args: None,
                },
            )
            .map_err(|error| error.to_string())?;
        if result.get("ok").is_some_and(truthy) {
            let address = or_empty(result.get("address"));
            if address.is_empty() {
                Ok(or_empty(result.get("terminal")))
            } else {
                Ok(address)
            }
        } else {
            Err(raw_text(result.get("error")))
        }
    }

    /// `_take_prelaunched(spec)`: dirección ya abierta, o `""`.
    // DECISION: el `threading.Event` del Python no cabe en la fila JSON del
    // registro; se sondea `done` sin retener el lock (30 s como allí).
    pub fn take_prelaunched(&self, spec: &Map<String, Value>) -> String {
        let chain_id = or_empty(spec.get("chain_id"));
        if chain_id.is_empty() {
            return String::new();
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let current = {
                let registry = self.inner.agents.prelaunch.lock();
                match registry.get(&chain_id) {
                    Some(entry) => (
                        entry.get("done").is_some_and(truthy),
                        or_empty(entry.get("address")),
                    ),
                    None => return String::new(),
                }
            };
            if current.0 || Instant::now() >= deadline {
                self.inner.agents.prelaunch.lock().remove(&chain_id);
                return current.1;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
