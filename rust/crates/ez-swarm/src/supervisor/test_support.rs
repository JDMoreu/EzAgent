//! Doble de [`SwarmHost`] para las pruebas: no abre terminales ni gasta red.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Map, Value};

use super::{LaunchRequest, SwarmHost};

/// Host falso con lo justo para el supervisor y la recuperación.
#[derive(Default)]
pub(crate) struct FakeHost {
    pub agents: Mutex<Vec<Map<String, Value>>>,
    pub detail: Mutex<Option<Map<String, Value>>>,
    pub screens: Mutex<HashMap<String, String>>,
    pub sent: Mutex<Vec<(String, String)>>,
    pub launched: Mutex<Vec<(String, String, LaunchRequest)>>,
    pub run_result: Mutex<Option<Result<Map<String, Value>, String>>>,
    pub send_ok: Mutex<bool>,
    pub resolved: Mutex<Vec<String>>,
    pub names: Mutex<Vec<String>>,
    pub default_cwd: Mutex<String>,
    pub saved: Mutex<Vec<String>>,
    pub save_path: Mutex<Option<String>>,
    pub swarm_args: Mutex<Vec<String>>,
}

impl FakeHost {
    pub(crate) fn new() -> Self {
        Self {
            send_ok: Mutex::new(true),
            ..Self::default()
        }
    }

    pub(crate) fn set_agents(&self, agents: Vec<Map<String, Value>>) {
        *self
            .agents
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = agents;
    }

    pub(crate) fn set_screen(&self, address: &str, screen: &str) {
        self.screens
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(address.to_string(), screen.to_string());
    }

    pub(crate) fn sent_lines(&self) -> Vec<(String, String)> {
        self.sent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn launched_count(&self) -> usize {
        self.launched
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }
}

impl SwarmHost for FakeHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(self
            .agents
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone())
    }

    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, String> {
        self.detail
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or_else(|| "no existe".to_string())
    }

    fn read_agent(&self, address: &str, _lines: i64) -> Result<String, String> {
        self.screens
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(address)
            .cloned()
            .ok_or_else(|| "sin pantalla".to_string())
    }

    fn send_task(&self, address: &str, line: &str) -> Result<Map<String, Value>, String> {
        self.sent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((address.to_string(), line.to_string()));
        let ok = *self
            .send_ok
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(ok));
        Ok(row)
    }

    fn run_agent(
        &self,
        agent: &str,
        task: &str,
        request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        self.launched
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((agent.to_string(), task.to_string(), request));
        match self
            .run_result
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        {
            Some(result) => result,
            None => {
                let mut row = Map::new();
                row.insert("ok".to_string(), Value::Bool(true));
                row.insert("address".to_string(), Value::String(format!("t-{agent}")));
                Ok(row)
            }
        }
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.resolved
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(name.to_string());
        let names = self.names.lock().unwrap_or_else(|error| error.into_inner());
        if names.is_empty() {
            (!name.is_empty()).then(|| name.to_string())
        } else {
            names.iter().find(|known| known.as_str() == name).cloned()
        }
    }

    fn swarm_args(&self, _agent: &str) -> Vec<String> {
        self.swarm_args
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn save_context(&self, text: &str) -> Option<String> {
        self.saved
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(text.to_string());
        self.save_path
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn default_cwd(&self) -> String {
        self.default_cwd
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn agent_names(&self) -> Vec<String> {
        self.names
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}
