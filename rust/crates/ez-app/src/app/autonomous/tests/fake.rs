//! Doble de `HerdrPort` con pantallas guionizadas y registro de lo enviado.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ez_agent::tools_herdr::port::{HerdrPort, PortError};
use ez_herdr::herdr::{NameCandidate, NameKind, RunAgent};
use parking_lot::Mutex;
use serde_json::{json, Map, Value};

/// Una terminal del doble.
#[derive(Clone)]
pub struct FakeAgent {
    pub state: String,
    pub screen: String,
    pub space: String,
    /// Pantallas que se devuelven en lecturas sucesivas (luego, la última).
    pub script: VecDeque<String>,
}

/// Lo que se pidió lanzar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub agent: String,
    pub task: String,
    pub address: String,
}

/// Doble del puerto: nada de red, ni procesos, ni ventanas.
pub struct FakePort {
    agents: Mutex<BTreeMap<String, FakeAgent>>,
    next: AtomicUsize,
    launches: Mutex<Vec<Launch>>,
    sends: Mutex<Vec<(String, String)>>,
    keys: Mutex<Vec<(String, Vec<String>)>>,
    kills: Mutex<Vec<String>>,
}

impl FakePort {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            agents: Mutex::new(BTreeMap::new()),
            next: AtomicUsize::new(1),
            launches: Mutex::new(Vec::new()),
            sends: Mutex::new(Vec::new()),
            keys: Mutex::new(Vec::new()),
            kills: Mutex::new(Vec::new()),
        })
    }

    pub fn add(&self, address: &str, state: &str, screen: &str) {
        self.agents.lock().insert(
            address.to_string(),
            FakeAgent {
                state: state.to_string(),
                screen: screen.to_string(),
                space: "proyecto".to_string(),
                script: VecDeque::new(),
            },
        );
    }

    pub fn set(&self, address: &str, state: &str, screen: &str) {
        let mut agents = self.agents.lock();
        if let Some(agent) = agents.get_mut(address) {
            agent.state = state.to_string();
            agent.screen = screen.to_string();
            agent.script.clear();
        } else {
            drop(agents);
            self.add(address, state, screen);
        }
    }

    /// Lecturas sucesivas con pantallas distintas (diálogo que cambia).
    pub fn script(&self, address: &str, screens: &[&str]) {
        let mut agents = self.agents.lock();
        if let Some(agent) = agents.get_mut(address) {
            agent.script = screens.iter().map(|screen| (*screen).to_string()).collect();
        }
    }

    pub fn state_of(&self, address: &str) -> String {
        self.agents
            .lock()
            .get(address)
            .map_or_else(String::new, |agent| agent.state.clone())
    }

    pub fn set_state(&self, address: &str, state: &str) {
        if let Some(agent) = self.agents.lock().get_mut(address) {
            agent.state = state.to_string();
        }
    }

    pub fn launches(&self) -> Vec<Launch> {
        self.launches.lock().clone()
    }

    pub fn sends(&self) -> Vec<(String, String)> {
        self.sends.lock().clone()
    }

    pub fn keys_sent(&self) -> Vec<(String, Vec<String>)> {
        self.keys.lock().clone()
    }

    pub fn killed(&self) -> Vec<String> {
        self.kills.lock().clone()
    }

    pub fn last_launch(&self) -> Option<Launch> {
        self.launches.lock().last().cloned()
    }
}

fn row(agent: &FakeAgent, address: &str) -> Map<String, Value> {
    json!({
        "address": address,
        "agent": "claude",
        "state": agent.state,
        "space": agent.space,
        "where": format!("{} / proyecto", agent.space),
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

impl HerdrPort for FakePort {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        Some(name.to_string())
    }

    fn label_for(&self, _address: &str) -> String {
        String::new()
    }

    fn name_candidates(&self, _kind: NameKind, _fresh: bool) -> Vec<NameCandidate> {
        Vec::new()
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        Ok(self
            .agents
            .lock()
            .iter()
            .map(|(address, agent)| row(agent, address))
            .collect())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        let agents = self.agents.lock();
        match agents.get(address) {
            Some(agent) => Ok(row(agent, address)),
            None => Err(PortError::Herdr(ez_herdr::herdr::HerdrError::not_found(
                "no existe",
            ))),
        }
    }

    fn read_agent(&self, address: &str, _lines: Option<i64>) -> Result<String, PortError> {
        let mut agents = self.agents.lock();
        let Some(agent) = agents.get_mut(address) else {
            return Err(PortError::Herdr(ez_herdr::herdr::HerdrError::not_found(
                "no existe",
            )));
        };
        if agent.script.len() > 1 {
            Ok(agent.script.pop_front().unwrap_or_default())
        } else if let Some(screen) = agent.script.front() {
            Ok(screen.clone())
        } else {
            Ok(agent.screen.clone())
        }
    }

    fn read_full(&self, address: &str) -> Result<String, PortError> {
        self.read_agent(address, None)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        self.sends
            .lock()
            .push((address.to_string(), text.to_string()));
        self.set_state(address, "working");
        Ok(json!({"ok": true, "address": address})
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        let number = self.next.fetch_add(1, Ordering::SeqCst);
        let address = format!("t{number}");
        self.agents.lock().insert(
            address.clone(),
            FakeAgent {
                state: "working".to_string(),
                screen: String::new(),
                space: params.space.unwrap_or("proyecto").to_string(),
                script: VecDeque::new(),
            },
        );
        self.launches.lock().push(Launch {
            agent: params.agent.to_string(),
            task: params.task.to_string(),
            address: address.clone(),
        });
        Ok(json!({"ok": true, "address": address, "terminal": address})
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    fn open_terminal(
        &self,
        _name: &str,
        _cwd: Option<&str>,
        _shell: &str,
        _space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        Vec::new()
    }

    fn configure_agent(
        &self,
        _address: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> Map<String, Value> {
        Map::new()
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, PortError> {
        self.keys.lock().push((address.to_string(), keys.to_vec()));
        self.set(address, "idle", "Listo.\n");
        Ok(json!({"ok": true, "address": address})
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        Ok(json!({"ok": true, "address": address})
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    fn kill_agent(&self, address: &str, _confirmed: bool) -> Result<Map<String, Value>, PortError> {
        self.kills.lock().push(address.to_string());
        self.agents.lock().remove(address);
        Ok(Map::new())
    }

    fn save_context(&self, _text: &str) -> Option<String> {
        None
    }
}
