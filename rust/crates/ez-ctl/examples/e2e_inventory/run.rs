//! Running `ez` and `hc` with the sandbox environment, and reading their JSON.

use serde_json::Value;

use super::proc::{self, EnvVars};
use super::setup::Sandbox;

/// Runs `ez` with the sandbox environment and an optional parent terminal.
pub fn ez(sandbox: &Sandbox, parent: Option<&str>, args: &[String]) -> proc::Output {
    let mut env = sandbox.env.clone();
    if let Some(parent) = parent {
        env.set("HC_TERMINAL_ID", parent);
    }
    proc::run(&sandbox.ez, args, &env, Some(&sandbox.work))
}

/// Runs `ez` and parses its JSON output.
pub fn ez_json(sandbox: &Sandbox, parent: Option<&str>, args: &[String]) -> Result<Value, String> {
    let output = ez(sandbox, parent, args);
    if !output.ok() {
        return Err(format!("código {}: {}", output.code, output.message()));
    }
    proc::json(&output)
}

/// Runs `hc` with no environment overlay.
pub fn hc(sandbox: &Sandbox, args: &[String]) -> proc::Output {
    proc::run(&sandbox.hc, args, &EnvVars::default(), None)
}

/// Runs `hc` and parses its JSON output.
pub fn hc_json(sandbox: &Sandbox, args: &[String]) -> Result<Value, String> {
    let output = hc(sandbox, args);
    if !output.ok() {
        return Err(format!("código {}: {}", output.code, output.message()));
    }
    proc::json(&output)
}

/// The terminals that carry a label.
pub fn terminals_with_label(sandbox: &Sandbox, label: &str) -> Vec<Value> {
    let args = vec![
        "ls".to_string(),
        "--all".to_string(),
        "--json".to_string(),
        "--label".to_string(),
        label.to_string(),
    ];
    let Ok(data) = hc_json(sandbox, &args) else {
        return Vec::new();
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// All terminals `hc` knows about (live or finished).
pub fn all_terminals(sandbox: &Sandbox) -> Vec<Value> {
    let args = vec!["ls".to_string(), "--all".to_string(), "--json".to_string()];
    let Ok(data) = hc_json(sandbox, &args) else {
        return Vec::new();
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// `hc get` of a terminal.
pub fn terminal(sandbox: &Sandbox, address: &str) -> Result<Value, String> {
    hc_json(sandbox, &["get".to_string(), address.to_string()])
}

/// A text field of a JSON value.
pub fn text(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A list of texts of a JSON value.
pub fn strings(data: &Value, key: &str) -> Vec<String> {
    match data.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().unwrap_or_default().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// A numeric field of a JSON value.
pub fn number(data: &Value, key: &str) -> f64 {
    data.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// A boolean field of a JSON value.
pub fn flag(data: &Value, key: &str) -> bool {
    data.get(key).and_then(Value::as_bool).unwrap_or(false)
}
