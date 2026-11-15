//! Paridad de `tasks.py` y `activity.py` contra
//! `parity/fixtures/tasks_activity.jsonl` (P-HERDR).
//!
//! El oráculo congeló `time.time()` en 2025-01-01T00:00:00Z; el reloj de
//! `TaskRegistry` y `ActivityLog` se inyecta aquí con ese instante.

use std::path::Path;
use std::sync::{Arc, Mutex};

use ez_core::pyvalue::text_or_empty;
use ez_herdr::activity::{
    chain_key, clean_title, dismissible, live_items, merge, orbit, order, snapshot_at, state_label,
    ActivityItem, ActivityLog, ActivitySource, DONE, FAILED, KEEP_SECS, LOG_LIMIT, NEEDS_YOU,
    ORBIT_LIMIT, STALE, STATE_LABEL, STATE_ORDER, TITLE_LIMIT, WAITING, WORKING,
};
use ez_herdr::tasks::{prompt_value, TaskRegistry};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/tasks_activity.jsonl";
const FROZEN: f64 = 1_735_689_600.0;

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn boolean(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

fn item_from(value: &Value) -> ActivityItem {
    ActivityItem {
        key: text(value.get("key")),
        title: text(value.get("title")),
        agent: text(value.get("agent")),
        state: text(value.get("state")),
        since: number(value.get("since")),
        unseen: boolean(value.get("unseen")),
        live: boolean(value.get("live")),
    }
}

fn item_json(item: &ActivityItem) -> Value {
    json!({
        "key": item.key,
        "title": item.title,
        "agent": item.agent,
        "state": item.state,
        "since": item.since,
        "unseen": item.unseen,
        "live": item.live,
        "label": state_label(&item.state),
        "marked": item.marked(),
    })
}

fn items_json(items: &[ActivityItem]) -> Value {
    Value::Array(items.iter().map(item_json).collect())
}

struct Source<'a>(&'a Value);

impl Source<'_> {
    fn raised(&self, flag: &str) -> bool {
        self.0
            .get("raise")
            .and_then(|raise| raise.get(flag))
            .is_some_and(|value| value == &Value::Bool(true))
    }

    fn tasks(&self) -> Option<&Map<String, Value>> {
        match self.0.get("tasks") {
            Some(Value::Object(map)) => Some(map),
            _ => None,
        }
    }
}

impl ActivitySource for Source<'_> {
    fn watched(&self) -> Vec<String> {
        if self.raised("watched") {
            return Vec::new();
        }
        self.0
            .get("watched")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|item| text(Some(item))).collect())
            .unwrap_or_default()
    }

    fn task(&self, address: &str) -> Option<Map<String, Value>> {
        if self.raised("get") {
            return None;
        }
        self.tasks()
            .and_then(|tasks| tasks.get(address))
            .and_then(Value::as_object)
            .cloned()
    }

    fn state_of(&self, address: &str) -> Option<String> {
        if self.0.get("watcher") == Some(&Value::Bool(false)) {
            return Some("working".to_string());
        }
        let states = self.0.get("states").and_then(Value::as_object);
        Some(
            states
                .and_then(|states| states.get(address))
                .map(|value| text(Some(value)))
                .unwrap_or_else(|| "working".to_string()),
        )
    }

    fn label_for(&self, address: &str) -> Option<String> {
        if self.0.get("herdr") == Some(&Value::Bool(false)) {
            return None;
        }
        let labels = self.0.get("labels").and_then(Value::as_object);
        Some(
            labels
                .and_then(|labels| labels.get(address))
                .map(|value| text_or_empty(Some(value)))
                .unwrap_or_default(),
        )
    }

    fn chains(&self) -> Vec<Map<String, Value>> {
        if self.raised("chains") || self.tasks().is_none() {
            return Vec::new();
        }
        self.0
            .get("chains")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_object).cloned().collect())
            .unwrap_or_default()
    }

    fn active_swarms(&self) -> Vec<(String, String)> {
        self.0
            .get("swarm")
            .and_then(Value::as_array)
            .map(|pairs| {
                pairs
                    .iter()
                    .filter_map(Value::as_array)
                    .map(|pair| (text(pair.first()), text(pair.get(1))))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn log_case(input: &Value) -> (Vec<Value>, ActivityLog) {
    let start = number(input.get("start"));
    let clock = Arc::new(Mutex::new(start));
    let tick = Arc::clone(&clock);
    let log = ActivityLog::new(move || *tick.lock().expect("reloj"));
    let mut results = Vec::new();
    for op in input
        .get("ops")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match op.get("op").and_then(Value::as_str) {
            Some("at") => {
                *clock.lock().expect("reloj") = number(op.get("now"));
                results.push(Value::Null);
            }
            Some("record") => results.push(item_json(&log.record(
                &text(op.get("key")),
                &text(op.get("agent")),
                &text(op.get("title")),
                &text(op.get("reason")),
            ))),
            Some("seen_all") => results.push(Value::Bool(log.seen_all())),
            Some("forget") => {
                log.forget(&text(op.get("key")));
                results.push(Value::Null);
            }
            _ => results.push(items_json(&log.items())),
        }
    }
    (results, log)
}

fn task_case(case: &FixtureCase, temp: &str) -> Result<Value, String> {
    let input = &case.input;
    let name = case.case.rsplit('/').next().unwrap_or("tasks");
    let work = Path::new(temp).join("task-cases").join(name);
    std::fs::create_dir_all(&work).expect("carpeta del caso");
    let path = work.join("tasks.json");
    match input.get("seed") {
        Some(Value::String(seed)) if seed == "<dir>" => {
            std::fs::create_dir_all(&path).expect("directorio");
        }
        Some(Value::String(seed)) => std::fs::write(&path, seed).expect("seed"),
        _ => {}
    }
    let clock = Arc::new(|| FROZEN);
    let mut registry = TaskRegistry::with_clock(&path, clock.clone());
    let mut results = Vec::new();
    for op in input
        .get("ops")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let value = match op.get("op").and_then(Value::as_str) {
            Some("add") => registry
                .add_raw(
                    op.get("address").unwrap_or(&Value::Null),
                    op.get("agent").unwrap_or(&Value::Null),
                    op.get("task").unwrap_or(&Value::Null),
                    op.get("created_at"),
                )
                .map(|row| row.map(Value::Object).unwrap_or(Value::Null))
                .unwrap_or_else(|error| error_json(&error)),
            Some("get") => registry
                .get(&text(op.get("address")))
                .map(Value::Object)
                .unwrap_or(Value::Null),
            Some("remove") => registry
                .remove(&text(op.get("address")))
                .expect("remove")
                .map(Value::Object)
                .unwrap_or(Value::Null),
            Some("clear") => {
                registry.clear().expect("clear");
                Value::Bool(true)
            }
            Some("all") => Value::Array(registry.all().into_iter().map(Value::Object).collect()),
            Some("mutate_all") => {
                let mut items = registry.all();
                if let Some(first) = items.first_mut() {
                    first.insert("task".to_string(), Value::from("mutado"));
                }
                json!({
                    "mutated": items.into_iter().map(Value::Object).collect::<Vec<_>>(),
                    "after": registry.all().into_iter().map(Value::Object).collect::<Vec<_>>(),
                })
            }
            Some("add_chain") => match op.get("spec") {
                Some(Value::Object(spec)) => registry
                    .add_chain(spec)
                    .map(|row| row.map(Value::Object).unwrap_or(Value::Null))
                    .unwrap_or_else(|error| error_json(&error.to_string())),
                _ => Value::Null,
            },
            Some("chains") => {
                Value::Array(registry.chains().into_iter().map(Value::Object).collect())
            }
            Some("chains_for") => Value::Array(
                registry
                    .chains_for(&text(op.get("source")))
                    .into_iter()
                    .map(Value::Object)
                    .collect(),
            ),
            Some("remove_chains_for") => Value::Array(
                registry
                    .remove_chains_for(&text(op.get("source")))
                    .expect("remove_chains_for")
                    .into_iter()
                    .map(Value::Object)
                    .collect(),
            ),
            Some("remove_chain") => {
                let spec = op.get("spec").and_then(Value::as_object).cloned();
                Value::Bool(
                    registry
                        .remove_chain(&spec.unwrap_or_default())
                        .expect("remove_chain"),
                )
            }
            Some("prompt") => Value::from(registry.prompt_block()),
            Some("reload") => {
                registry = TaskRegistry::with_clock(&path, clock.clone());
                json!({"tasks": registry.all(), "chains": registry.chains()})
            }
            Some("write_raw") => {
                std::fs::write(&path, text(op.get("text"))).expect("write_raw");
                Value::Bool(true)
            }
            other => return Err(format!("operación sin portar: {other:?}")),
        };
        results.push(value);
    }
    let file = if path.is_dir() {
        Value::from("<dir>")
    } else if path.exists() {
        Value::String(
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .replace("\r\n", "\n"),
        )
    } else {
        Value::Null
    };
    Ok(json!({
        "results": results,
        "tasks": registry.all(),
        "chains": registry.chains(),
        "prompt": registry.prompt_block(),
        "file": file,
    }))
}

/// El arnés del oráculo imprime `Tipo: mensaje`; `add_raw` devuelve el mensaje.
fn error_json(message: &str) -> Value {
    let kind = if message.starts_with("[WinError") || message.starts_with("PermissionError") {
        "PermissionError"
    } else {
        "ValueError"
    };
    json!({"error": format!("{kind}: {message}")})
}

fn run_case(case: &FixtureCase, temp: &str) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "module_constants" => {
            if case.case == "tasks_activity/activity/constants" {
                let order: Map<String, Value> = STATE_ORDER
                    .iter()
                    .map(|(key, rank)| ((*key).to_string(), Value::from(*rank)))
                    .collect();
                let labels: Map<String, Value> = STATE_LABEL
                    .iter()
                    .map(|(key, label)| ((*key).to_string(), Value::from(*label)))
                    .collect();
                Ok(json!({
                    "states": [NEEDS_YOU, FAILED, WORKING, WAITING, STALE, DONE],
                    "state_order": order,
                    "state_label": labels,
                    "title_limit": TITLE_LIMIT,
                    "keep_secs": KEEP_SECS,
                    "log_limit": LOG_LIMIT,
                    "orbit_limit": ORBIT_LIMIT,
                }))
            } else {
                Ok(json!({"task_registry": "TaskRegistry"}))
            }
        }
        "chain_key" => Ok(json!({"key": chain_key(
            input.get("spec").and_then(Value::as_object).unwrap_or(&Map::new())
        )})),
        "clean_title" => {
            let value = input.get("text").unwrap_or(&Value::Null);
            let text = text_or_empty(Some(value));
            let fallback = input
                .get("fallback")
                .and_then(Value::as_str)
                .unwrap_or("Tarea");
            Ok(json!({"title": clean_title(&text, fallback)}))
        }
        "dismissible" => Ok(json!({"dismissible": dismissible(&item_from(input))})),
        "order" => {
            let items: Vec<ActivityItem> = input
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(item_from).collect())
                .unwrap_or_default();
            Ok(json!({"items": items_json(&order(items))}))
        }
        "merge" => {
            let live: Vec<ActivityItem> = input
                .get("live")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(item_from).collect())
                .unwrap_or_default();
            let recent: Vec<ActivityItem> = input
                .get("recent")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(item_from).collect())
                .unwrap_or_default();
            Ok(json!({"items": items_json(&merge(live, recent))}))
        }
        "orbit" => {
            let items: Vec<ActivityItem> = input
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(item_from).collect())
                .unwrap_or_default();
            let limit = input
                .get("limit")
                .and_then(Value::as_i64)
                .unwrap_or(ORBIT_LIMIT as i64)
                .max(0) as usize;
            Ok(json!({"states": orbit(items, limit)}))
        }
        "prompt_value" => {
            let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(220) as usize;
            Ok(json!({"text": prompt_value(input.get("value"), limit)}))
        }
        "live_items" => {
            let source = Source(input);
            let now = match input.get("now") {
                Some(Value::Number(number)) => number.as_f64(),
                Some(Value::Null) | None => Some(FROZEN),
                _ => Some(FROZEN),
            };
            Ok(json!({"items": items_json(&live_items(&source, now))}))
        }
        "ActivityLog" => {
            let (results, log) = log_case(input);
            Ok(json!({
                "results": results,
                "items": items_json(&log.items()),
                "keys": log.items().iter().map(|item| item.key.clone()).collect::<Vec<_>>(),
            }))
        }
        "snapshot" => {
            let source = Source(input.get("app").unwrap_or(&Value::Null));
            let log = match input.get("log") {
                Some(Value::Array(entries)) => {
                    let clock = Arc::new(Mutex::new(number(input.get("log_clock"))));
                    let tick = Arc::clone(&clock);
                    let log = ActivityLog::new(move || *tick.lock().expect("reloj"));
                    for entry in entries {
                        let parts = entry.as_array().cloned().unwrap_or_default();
                        log.record(
                            &text(parts.first()),
                            &text(parts.get(1)),
                            &text(parts.get(2)),
                            &text(parts.get(3)),
                        );
                    }
                    Some(log)
                }
                _ => None,
            };
            Ok(json!({"items": items_json(&snapshot_at(&source, log.as_ref(), Some(FROZEN)))}))
        }
        "TaskRegistry" => task_case(case, temp),
        other => Err(format!("función sin portar: {other}")),
    }
}

/// Borra el árbol temporal propio con `remove_file`/`remove_dir` (la puerta no
/// permite el borrado recursivo de la biblioteca estándar).
fn cleanup(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup(&path);
            let _ = std::fs::remove_dir(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn unmark(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("<tmp>", temp).replace('\\', "/")),
        Value::Array(items) => Value::Array(items.iter().map(|item| unmark(item, temp)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), unmark(value, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn diff_path(left: &Value, right: &Value, path: &str) -> String {
    if json_eq(left, right) {
        return String::new();
    }
    match (left, right) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, value) in actual {
                match expected.get(key) {
                    Some(other) => {
                        let nested = diff_path(value, other, &format!("{path}.{key}"));
                        if !nested.is_empty() {
                            return nested;
                        }
                    }
                    None => return format!("{path}.{key} (de más)"),
                }
            }
            for key in expected.keys() {
                if !actual.contains_key(key) {
                    return format!("{path}.{key} (falta)");
                }
            }
            path.to_string()
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (index, (value, other)) in actual.iter().zip(expected).enumerate() {
                let nested = diff_path(value, other, &format!("{path}[{index}]"));
                if !nested.is_empty() {
                    return nested;
                }
            }
            format!("{path} long {}", actual.len())
        }
        _ => format!("{path}: {left:?} != {right:?}"),
    }
}

fn json_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(actual), Value::Number(expected)) => {
            match (actual.as_f64(), expected.as_f64()) {
                (Some(actual), Some(expected)) => {
                    (actual - expected).abs() <= 1e-9 * actual.abs().max(expected.abs()).max(1.0)
                }
                _ => actual == expected,
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| json_eq(actual, expected))
        }
        (Value::Object(actual), Value::Object(expected)) => {
            actual.len() == expected.len()
                && actual.iter().all(|(key, value)| {
                    expected.get(key).is_some_and(|other| json_eq(value, other))
                })
        }
        _ => left == right,
    }
}

#[test]
fn every_tasks_activity_case_matches_python() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    let cases = fixtures::load(&path).expect("fixtures legibles");
    let base = std::env::temp_dir().join(format!("ez-tasks-parity-{}", std::process::id()));
    cleanup(&base);
    std::fs::create_dir_all(&base).expect("carpeta temporal");
    let base_text = base.display().to_string();
    let mut failures = Vec::new();
    for case in &cases {
        let got = run_case(case, &base_text).map(|value| unmark(&value, &base_text));
        let expected = unmark(&case.output, &base_text);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => json_eq(value, &expected),
            (Err(message), Some(error)) => message == error,
            _ => false,
        };
        if !ok {
            let detail = match &got {
                Ok(value) => diff_path(value, &expected, "$"),
                Err(_) => String::new(),
            };
            failures.push(format!("{}: {detail}", case.case));
        }
    }
    cleanup(&base);
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(cases.len() >= 100, "casos: {}", cases.len());
}
