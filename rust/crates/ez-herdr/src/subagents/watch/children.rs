//! Lo que el vigilante lee de `hc ls`: las terminales del padre y su estado.

use serde_json::Value;

/// Una terminal que cuelga del padre, según `hc ls`.
#[derive(Debug, Clone)]
pub(super) struct Child {
    pub(super) address: String,
    pub(super) live: bool,
    pub(super) exit_code: Option<i64>,
    /// `status.state` del agente: `working`, `idle`, `blocked`…
    pub(super) agent_state: String,
}

/// El estado de una terminal en el JSON de `hc`.
pub(super) fn state_of(data: &Value) -> String {
    if let Some(state) = data.get("state").and_then(Value::as_str) {
        return state.to_string();
    }
    match data.get("status") {
        Some(Value::Object(status)) => status
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some(Value::String(state)) => state.clone(),
        _ => String::new(),
    }
}

/// Las terminales de `hc ls --json`.
pub(super) fn parse_children(data: &Value) -> Vec<Child> {
    let Some(terminals) = data.get("terminals").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    for terminal in terminals {
        let Some(address) = terminal.get("address").and_then(Value::as_str) else {
            continue;
        };
        let state = state_of(terminal);
        children.push(Child {
            address: address.to_string(),
            live: state == "live",
            exit_code: terminal.get("exit_code").and_then(Value::as_i64),
            agent_state: agent_state_of(terminal),
        });
    }
    children
}

/// El estado del agente (`status.state`), con el nivel superior como respaldo.
fn agent_state_of(data: &Value) -> String {
    match data.get("status") {
        Some(Value::Object(status)) => status
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some(Value::String(state)) => state.clone(),
        _ => state_of(data),
    }
}
