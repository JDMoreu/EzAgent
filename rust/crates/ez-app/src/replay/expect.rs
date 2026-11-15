//! Aserciones de alto nivel (`check_expect` de `parity/replay/run_py.py`).
//!
//! Se evalúan al terminar el guion; el proceso sale con código 1 si alguna
//! falla. Se porta cada comprobación con su texto exacto, incluidas las de
//! `audit`, `hc`, `files` y `llm_routes` (las usan los escenarios 07-25).

use std::path::{Path, PathBuf};

use ez_core::pyvalue::{py_splitlines, py_strip, python_str};
use serde_json::{Map, Value};

use super::driver::Snapshot;
use super::trace::Trace;

mod extra;

use self::extra::check_extra;

/// Datos del entorno que algunas aserciones consultan.
pub struct Context {
    pub hc_path: Option<PathBuf>,
    pub requests_path: Option<PathBuf>,
}

/// `check_expect`: lista de fallos (vacía si todo cuadra).
pub fn check(
    expect: &Value,
    snapshot: &Snapshot,
    trace: &Trace,
    requests: usize,
    context: &Context,
) -> Vec<String> {
    let Some(expect) = expect.as_object() else {
        return Vec::new();
    };
    let mut errors = check_messages(expect, snapshot);
    let speech = &snapshot.speech;
    errors.extend(check_speech(expect, speech, trace));
    errors.extend(check_tools(expect, trace));
    if let Some(wanted) = expect.get("llm_requests").map(python_int) {
        if requests as i64 != wanted {
            errors.push(format!(
                "peticiones LLM {requests} != {}",
                python_str(expect.get("llm_requests").unwrap_or(&Value::Null))
            ));
        }
    }
    errors.extend(check_confirm(expect, trace));
    errors.extend(check_audit(expect.get("audit"), trace));
    errors.extend(check_hc(expect.get("hc"), context.hc_path.as_deref()));
    errors.extend(check_files(expect.get("files")));
    errors.extend(check_routes(
        expect.get("llm_routes"),
        context.requests_path.as_deref(),
    ));
    if let Some(final_spec) = expect.get("final").and_then(Value::as_object) {
        errors.extend(snapshot.matches_final(final_spec));
    }
    errors.extend(check_never(expect, snapshot));
    errors.extend(check_extra(expect, snapshot, trace));
    errors
}

/// `user`, `assistant` y los textos vetados (`never`).
fn check_messages(expect: &Map<String, Value>, snapshot: &Snapshot) -> Vec<String> {
    let mut errors = Vec::new();
    let users = snapshot.texts_for("user");
    let assistants = snapshot.texts_for("assistant");
    if let Some(wanted) = expect.get("user") {
        if Value::from(users.clone()) != *wanted {
            errors.push(format!(
                "usuarios: {} != {}",
                py_repr(&Value::from(users)),
                py_repr(wanted)
            ));
        }
    }
    if let Some(wanted) = expect.get("assistant") {
        if Value::from(assistants.clone()) != *wanted {
            errors.push(format!(
                "respuestas: {} != {}",
                py_repr(&Value::from(assistants)),
                py_repr(wanted)
            ));
        }
    }
    errors
}

/// `never`: el texto no aparece en mensajes, voz ni transcripciones.
fn check_never(expect: &Map<String, Value>, snapshot: &Snapshot) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(never) = expect.get("never").and_then(Value::as_array) else {
        return errors;
    };
    let mut haystack = snapshot.texts_for("user");
    haystack.extend(snapshot.texts_for("assistant"));
    haystack.extend(snapshot.speech.clone());
    haystack.extend(snapshot.transcripts.clone());
    for text in never.iter().map(python_str) {
        if haystack.iter().any(|item| item.contains(&text)) {
            errors.push(format!("apareció texto vetado: '{text}'"));
        }
    }
    errors
}

fn check_speech(expect: &Map<String, Value>, speech: &[String], trace: &Trace) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(items) = expect.get("speech_contains").and_then(Value::as_array) {
        for text in items.iter().map(python_str) {
            if !speech.contains(&text) {
                errors.push(format!(
                    "no se dijo '{text}' (dichas: {})",
                    py_repr(&Value::from(speech.to_vec()))
                ));
            }
        }
    }
    if let Some(items) = expect.get("speech_any").and_then(Value::as_array) {
        for text in items.iter().map(python_str) {
            if !speech.iter().any(|said| said.contains(&text)) {
                errors.push(format!(
                    "ninguna locución contiene '{text}' (dichas: {})",
                    py_repr(&Value::from(speech.to_vec()))
                ));
            }
        }
    }
    if let Some(counts) = expect.get("speech_count").and_then(Value::as_object) {
        for (text, count) in counts {
            let said = speech.iter().filter(|item| *item == text).count();
            if said as i64 != python_int(count) {
                errors.push(format!(
                    "'{text}' dicho {said} veces, se esperaban {}",
                    python_str(count)
                ));
            }
        }
    }
    if let Some(items) = expect.get("states").and_then(Value::as_array) {
        let states: Vec<String> = trace
            .with_event("hud.show")
            .iter()
            .filter_map(|item| {
                item.get("state")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect();
        let wanted: Vec<String> = items.iter().map(python_str).collect();
        if !subsequence(&wanted, &states) {
            errors.push(format!(
                "estados {} no contienen {} en orden",
                py_repr(&Value::from(states)),
                py_repr(&Value::from(wanted))
            ));
        }
    }
    errors
}

fn check_tools(expect: &Map<String, Value>, trace: &Trace) -> Vec<String> {
    let mut errors = Vec::new();
    let calls = trace.with_event("tool.call");
    let tools: Vec<String> = calls
        .iter()
        .map(|item| {
            format!(
                "{} {}",
                item.get("name").map(python_str).unwrap_or_default(),
                item.get("args").map(python_str).unwrap_or_default()
            )
        })
        .collect();
    if let Some(wanted) = expect.get("tool_calls") {
        if Value::from(tools.clone()) != *wanted {
            errors.push(format!(
                "herramientas {} != {}",
                py_repr(&Value::from(tools)),
                py_repr(wanted)
            ));
        }
    }
    let names: Vec<String> = calls
        .iter()
        .map(|item| item.get("name").map(python_str).unwrap_or_default())
        .collect();
    if let Some(wanted) = expect.get("tool_names") {
        if Value::from(names.clone()) != *wanted {
            errors.push(format!(
                "nombres de herramientas {} != {}",
                py_repr(&Value::from(names)),
                py_repr(wanted)
            ));
        }
    }
    errors
}

fn check_confirm(expect: &Map<String, Value>, trace: &Trace) -> Vec<String> {
    let mut errors = Vec::new();
    let confirms: Vec<String> = trace
        .with_event("hud.confirm")
        .iter()
        .map(|item| item.get("summary").map(python_str).unwrap_or_default())
        .collect();
    if let Some(items) = expect.get("confirms").and_then(Value::as_array) {
        for text in items.iter().map(python_str) {
            if !confirms.iter().any(|summary| summary.contains(&text)) {
                errors.push(format!(
                    "ninguna confirmación contiene '{text}' (vistas: {})",
                    py_repr(&Value::from(confirms.clone()))
                ));
            }
        }
    }
    if let Some(wanted) = expect.get("confirm_count") {
        if confirms.len() as i64 != python_int(wanted) {
            errors.push(format!(
                "confirmaciones {} != {}",
                confirms.len(),
                python_str(wanted)
            ));
        }
    }
    if expect.get("no_confirm").is_some_and(truthy) && !confirms.is_empty() {
        errors.push(format!(
            "no debía pedir confirmación: {}",
            py_repr(&Value::from(confirms))
        ));
    }
    errors
}

fn check_audit(wanted: Option<&Value>, trace: &Trace) -> Vec<String> {
    let Some(wanted) = wanted
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
    else {
        return Vec::new();
    };
    let actual: Vec<Value> = trace
        .with_event("audit.line")
        .iter()
        .map(|item| {
            let mut projected = Map::new();
            for key in ["tool", "decision", "taint", "ok"] {
                projected.insert(
                    key.to_string(),
                    item.get(key).cloned().unwrap_or(Value::Null),
                );
            }
            Value::Object(projected)
        })
        .collect();
    let mut index = 0;
    for expected in wanted {
        let Some(expected) = expected.as_object() else {
            continue;
        };
        while index < actual.len() && audit_mismatch(&actual[index], expected) {
            index += 1;
        }
        if index >= actual.len() {
            return vec![format!(
                "auditoría {} no contiene {} en orden",
                py_repr(&Value::from(actual)),
                py_repr(&Value::from(wanted.clone()))
            )];
        }
        index += 1;
    }
    Vec::new()
}

/// `any(key in expected and actual[key] != value)`.
fn audit_mismatch(actual: &Value, expected: &Map<String, Value>) -> bool {
    expected
        .iter()
        .any(|(key, value)| actual.get(key).is_some_and(|current| current != value))
}

fn check_hc(wanted: Option<&Value>, path: Option<&Path>) -> Vec<String> {
    let Some(wanted) = wanted
        .and_then(Value::as_object)
        .filter(|map| !map.is_empty())
    else {
        return Vec::new();
    };
    let Some(path) = path else {
        return vec!["no hay estado de fake-hc para comprobar".to_string()];
    };
    let state = read_hc(path);
    let mut errors = Vec::new();
    for (key, expected) in wanted {
        let Some(terminal) = find_terminal(&state, key) else {
            errors.push(format!("no hay terminal '{key}' en fake-hc"));
            continue;
        };
        let Some(expected) = expected.as_object() else {
            continue;
        };
        for (field, value) in expected {
            if field == "screen_contains" {
                let screen = terminal.get("screen").map(python_str).unwrap_or_default();
                for text in value.as_array().into_iter().flatten().map(python_str) {
                    if !screen.contains(&text) {
                        errors.push(format!("la pantalla de '{key}' no contiene '{text}'"));
                    }
                }
            } else if field == "state" && terminal.get("state") != Some(value) {
                errors.push(format!(
                    "estado de '{key}' {} != {}",
                    terminal
                        .get("state")
                        .map_or_else(|| "None".to_string(), py_repr),
                    py_repr(value)
                ));
            } else if terminal.get(field) != Some(value) {
                errors.push(format!(
                    "{field} de '{key}' {} != {}",
                    terminal
                        .get(field)
                        .map_or_else(|| "None".to_string(), py_repr),
                    py_repr(value)
                ));
            }
        }
    }
    errors
}

fn check_files(wanted: Option<&Value>) -> Vec<String> {
    let Some(wanted) = wanted.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for (raw_path, expected) in wanted {
        let path = Path::new(raw_path);
        let exists = path.exists();
        if let Some(flag) = expected.get("exists") {
            if exists != truthy(flag) {
                errors.push(format!(
                    "{}: existe={} != {}",
                    path.file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                    python_str(&Value::Bool(exists)),
                    python_str(flag)
                ));
            }
        }
        if !exists {
            continue;
        }
        let text = std::fs::read(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        for needle in expected
            .get("contains")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(python_str)
        {
            if !text.contains(&needle) {
                errors.push(format!(
                    "{} no contiene '{needle}'",
                    path.file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
                ));
            }
        }
        for needle in expected
            .get("not_contains")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(python_str)
        {
            if text.contains(&needle) {
                errors.push(format!(
                    "{} contiene '{needle}'",
                    path.file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
                ));
            }
        }
    }
    errors
}

fn check_routes(wanted: Option<&Value>, path: Option<&Path>) -> Vec<String> {
    let Some(wanted) = wanted
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
    else {
        return Vec::new();
    };
    let Some(path) = path.filter(|path| path.exists()) else {
        return Vec::new();
    };
    let text = std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    let routes: Vec<Value> = py_splitlines(&text)
        .into_iter()
        .filter(|line| !py_strip(line).is_empty())
        .map(|line| {
            ez_core::pyloads::loads(line)
                .ok()
                .and_then(|value| value.get("route").cloned())
                .unwrap_or(Value::Null)
        })
        .collect();
    if Value::from(routes.clone()) != Value::from(wanted.clone()) {
        return vec![format!(
            "rutas LLM {} != {}",
            py_repr(&Value::from(routes)),
            py_repr(&Value::from(wanted.clone()))
        )];
    }
    Vec::new()
}

/// `_subsequence`: cada valor esperado aparece en orden.
fn subsequence(wanted: &[String], actual: &[String]) -> bool {
    let mut remaining = actual.iter();
    wanted
        .iter()
        .all(|value| remaining.any(|candidate| candidate == value))
}

/// `read_hc`: tres intentos y estado vacío si no se puede leer.
fn read_hc(path: &Path) -> Value {
    for _ in 0..3 {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(value) = ez_core::pyloads::loads(&text) {
                return value;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    serde_json::json!({"terminals": []})
}

/// `find_terminal`: primero por `address`, después por `name`.
fn find_terminal<'a>(state: &'a Value, key: &str) -> Option<&'a Value> {
    let terminals = state.get("terminals").and_then(Value::as_array)?;
    let by_address = terminals
        .iter()
        .find(|terminal| terminal.get("address").map(python_str).unwrap_or_default() == key);
    by_address.or_else(|| {
        terminals
            .iter()
            .find(|terminal| terminal.get("name").map(python_str).unwrap_or_default() == key)
    })
}

/// `truthy` de Python (`bool(value)`).
fn truthy(value: &Value) -> bool {
    ez_core::config::truthy(value)
}

/// `int(value)` de Python.
fn python_int(value: &Value) -> i64 {
    ez_core::config::int_value(value, 0)
}

/// `repr(value)` de Python para los textos de error.
pub fn py_repr(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("'{key}': {}", py_repr(item)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::String(text) => format!("'{text}'"),
        other => python_str(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Snapshot {
        Snapshot {
            state: "listening".to_string(),
            in_call: true,
            muted: false,
            messages: vec![
                ("user".to_string(), "hola".to_string()),
                ("assistant".to_string(), "buenas".to_string()),
            ],
            speech: vec!["buenas".to_string()],
            transcripts: vec!["hola".to_string()],
            config: serde_json::Map::new(),
            history: Vec::new(),
            swarm: None,
        }
    }

    #[test]
    fn subsequence_ignores_extra_values() {
        assert!(subsequence(
            &["thinking".to_string(), "listening".to_string()],
            &[
                "idle".to_string(),
                "thinking".to_string(),
                "speaking".to_string(),
                "listening".to_string()
            ]
        ));
        assert!(!subsequence(
            &["speaking".to_string(), "thinking".to_string()],
            &["thinking".to_string(), "speaking".to_string()]
        ));
    }

    #[test]
    fn final_and_never_use_python_texts() {
        let trace = Trace::new(Path::new("/tmp/x"));
        let errors = check(
            &json!({"final": {"state": "idle", "in_call": true}, "never": ["buenas"]}),
            &snapshot(),
            &trace,
            0,
            &Context {
                hc_path: None,
                requests_path: None,
            },
        );
        assert_eq!(
            errors,
            vec![
                "final state='listening' != 'idle'".to_string(),
                "apareció texto vetado: 'buenas'".to_string()
            ]
        );
    }

    #[test]
    fn file_checks_read_the_disk() {
        let path =
            std::env::temp_dir().join(format!("ez-replay-expect-{}.txt", std::process::id()));
        std::fs::write(&path, "contenido").expect("archivo");
        let errors = check_files(Some(&json!({
            (path.display().to_string()): {"exists": true, "contains": ["nid"], "not_contains": ["otro"]}
        })));
        let _ = std::fs::remove_file(&path);
        assert!(errors.is_empty());
    }
}
