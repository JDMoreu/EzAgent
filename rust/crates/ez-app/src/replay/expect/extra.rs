//! Aserciones de `harness_ext.py` (escenarios 16-25).

use std::path::PathBuf;

use ez_core::pyvalue::{py_eq, python_str};
use serde_json::{Map, Value};

use super::{py_repr, python_int, truthy, Snapshot, Trace};

/// Aserciones de `harness_ext.py` (escenarios 16-25).
pub(super) fn check_extra(
    expect: &Map<String, Value>,
    snapshot: &Snapshot,
    trace: &Trace,
) -> Vec<String> {
    let mut errors = Vec::new();
    errors.extend(check_swarm(expect, snapshot));
    errors.extend(check_config(expect, snapshot));
    errors.extend(check_voice_json(expect));
    if let Some(wanted) = expect.get("chat_history") {
        if Value::from(snapshot.history.clone()) != *wanted {
            errors.push(format!(
                "historial: {} != {}",
                py_repr(&Value::from(snapshot.history.clone())),
                py_repr(wanted)
            ));
        }
    }
    if let Some(wanted) = expect.get("degraded") {
        let degraded: Vec<String> = trace
            .events()
            .iter()
            .filter_map(|item| item.get("event").and_then(Value::as_str))
            .filter(|event| event.starts_with("degraded."))
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if Value::from(degraded.clone()) != *wanted {
            errors.push(format!(
                "degradaciones: {} != {}",
                py_repr(&Value::from(degraded)),
                py_repr(wanted)
            ));
        }
    }
    if let Some(wanted) = expect.get("tool_failures") {
        let failures = trace
            .with_event("tool.result")
            .iter()
            .filter(|item| !item.get("ok").is_some_and(truthy))
            .count() as i64;
        if failures != python_int(wanted) {
            errors.push(format!(
                "herramientas fallidas: {failures} != {}",
                python_str(wanted)
            ));
        }
    }
    if let Some(wanted) = expect.get("files").and_then(Value::as_array) {
        let mut names: Vec<String> = Vec::new();
        for item in trace.with_event("file.write") {
            let name = item.get("name").map(python_str).unwrap_or_default();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if Value::from(names.clone()) != Value::from(wanted.clone()) {
            errors.push(format!(
                "archivos escritos: {} != {}",
                py_repr(&Value::from(names)),
                py_repr(&Value::from(wanted.clone()))
            ));
        }
    }
    errors
}

fn check_swarm(expect: &Map<String, Value>, snapshot: &Snapshot) -> Vec<String> {
    let mut errors = Vec::new();
    let empty = Map::new();
    let swarm = snapshot
        .swarm
        .as_ref()
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    for (key, name) in [
        ("swarm_tasks", "tasks"),
        ("swarm_members", "members"),
        ("swarm_incidents", "incidents"),
        ("launches", "launches"),
    ] {
        if let Some(wanted) = expect.get(key) {
            let actual = swarm.get(name).cloned().unwrap_or(Value::Null);
            if actual != *wanted {
                errors.push(format!(
                    "enjambre {name}: {} != {}",
                    py_repr(&actual),
                    py_repr(wanted)
                ));
            }
        }
    }
    if let Some(wanted) = expect.get("swarm_status") {
        let actual = swarm.get("status").cloned().unwrap_or(Value::Null);
        if actual != *wanted {
            errors.push(format!(
                "enjambre status: {} != {}",
                py_repr(&actual),
                py_repr(wanted)
            ));
        }
    }
    if let Some(wanted) = expect.get("resumes_sent") {
        let actual = swarm.get("resumes_sent").cloned().unwrap_or(Value::Null);
        if python_int(&actual) != python_int(wanted) {
            errors.push(format!(
                "avisos de reanudación: {} != {}",
                py_repr(&actual),
                py_repr(wanted)
            ));
        }
    }
    errors
}

fn check_config(expect: &Map<String, Value>, snapshot: &Snapshot) -> Vec<String> {
    let mut errors = Vec::new();
    for (key, value) in expect
        .get("config")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let actual = snapshot.config.get(key).cloned().unwrap_or(Value::Null);
        if !py_eq(&actual, value) {
            errors.push(format!(
                "ajuste {key}: {} != {}",
                py_repr(&actual),
                py_repr(value)
            ));
        }
    }
    errors
}

fn check_voice_json(expect: &Map<String, Value>) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(wanted) = expect.get("voice_json").and_then(Value::as_object) else {
        return errors;
    };
    let saved = voice_json();
    for (key, value) in wanted {
        let actual = saved.get(key).cloned().unwrap_or(Value::Null);
        if !py_eq(&actual, value) {
            errors.push(format!(
                "voice.json {key}: {} != {}",
                py_repr(&actual),
                py_repr(value)
            ));
        }
    }
    errors
}

/// `_voice_json`: `APPDATA/EzAgent/voice.json`, vacío si no se puede leer.
fn voice_json() -> Value {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return Value::Object(Map::new());
    };
    let path = PathBuf::from(appdata).join("EzAgent").join("voice.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| ez_core::pyloads::loads(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}
