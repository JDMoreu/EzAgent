//! Port of `src/ezagent/voice/tasks.py` (F3-15).
//!
//! Registro persistente de tareas que el usuario quiere vigilar. Guarda en
//! `%APPDATA%/EzAgent/tasks.json` qué agente se está vigilando y **qué tarea**
//! pidió el usuario. El archivo es del usuario y editable; la escritura es
//! atómica y el acceso está protegido por un cerrojo porque lo usan el hilo de
//! respuesta, el watcher y el hilo de UI.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{
    collapse_spaces, head_chars, py_rstrip, py_strip, python_str, text_or_empty, truthy,
};
use serde_json::{Map, Value};

type Row = Map<String, Value>;

struct State {
    tasks: Map<String, Value>,
    chains: Vec<Row>,
}

/// `str(value or "")` seguido de `str.strip()`.
pub(crate) fn stripped(value: Option<&Value>) -> String {
    py_strip(&text_or_empty(value)).to_string()
}

/// `float(value or fallback)`; un valor no numérico también cae al `fallback`.
pub(crate) fn number_or(value: Option<&Value>, fallback: f64) -> f64 {
    let Some(value) = value else {
        return fallback;
    };
    if !truthy(value) {
        return fallback;
    }
    match value {
        Value::Number(number) => number.as_f64().unwrap_or(fallback),
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        Value::String(text) => py_strip(text).parse().unwrap_or(fallback),
        _ => fallback,
    }
}

fn sort_key(row: &Row) -> f64 {
    row.get("created_at").and_then(Value::as_f64).unwrap_or(0.0)
}

fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by(|left, right| sort_key(left).total_cmp(&sort_key(right)));
    rows
}

fn task_row(item: &Map<String, Value>) -> Option<(String, Row)> {
    let address = stripped(item.get("address"));
    if address.is_empty() {
        return None;
    }
    let mut row = Map::new();
    row.insert("address".to_string(), Value::String(address.clone()));
    row.insert(
        "agent".to_string(),
        Value::String(text_or_empty(item.get("agent"))),
    );
    row.insert(
        "task".to_string(),
        Value::String(text_or_empty(item.get("task"))),
    );
    row.insert(
        "created_at".to_string(),
        Value::from(number_or(item.get("created_at"), timestamp())),
    );
    Some((address, row))
}

fn chain_row(item: &Map<String, Value>) -> Option<Row> {
    let source = stripped(item.get("source"));
    let agent = stripped(item.get("agent"));
    if source.is_empty() || agent.is_empty() {
        return None;
    }
    let mut spec = item.clone();
    spec.insert("source".to_string(), Value::String(source));
    spec.insert("agent".to_string(), Value::String(agent));
    spec.insert(
        "created_at".to_string(),
        Value::from(number_or(item.get("created_at"), timestamp())),
    );
    Some(spec)
}

fn load(path: &Path) -> State {
    let data = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let mut state = State {
        tasks: Map::new(),
        chains: Vec::new(),
    };
    let Some(data) = data else {
        return state;
    };
    let entries: Vec<Value> = match &data {
        Value::Object(map) => match map.get("tasks") {
            Some(Value::Array(items)) => items.clone(),
            _ => map.values().cloned().collect(),
        },
        Value::Array(items) => items.clone(),
        _ => Vec::new(),
    };
    for item in &entries {
        if let Value::Object(item) = item {
            if let Some((address, row)) = task_row(item) {
                state.tasks.insert(address, Value::Object(row));
            }
        }
    }
    if let Value::Object(map) = &data {
        if let Some(Value::Array(raw_chains)) = map.get("chains") {
            for item in raw_chains {
                if let Value::Object(item) = item {
                    if let Some(spec) = chain_row(item) {
                        state.chains.push(spec);
                    }
                }
            }
        }
    }
    state
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    // `Path.write_text` traduce los finales de línea a los del sistema.
    let text = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, path)
}

fn tasks_of(state: &State) -> Vec<Row> {
    sorted(
        state
            .tasks
            .values()
            .filter_map(Value::as_object)
            .cloned()
            .collect(),
    )
}

fn chains_of(state: &State) -> Vec<Row> {
    sorted(state.chains.clone())
}

fn prompt_value(value: Option<&Value>, limit: usize) -> String {
    let text = collapse_spaces(&text_or_empty(value));
    let text = if text.chars().count() > limit {
        format!("{}…", py_rstrip(head_chars(&text, limit - 1)))
    } else {
        text
    };
    // html.escape(text, quote=False).
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn or_marker(value: String, marker: &str) -> String {
    if value.is_empty() {
        marker.to_string()
    } else {
        value
    }
}

fn task_line(item: &Row) -> String {
    let address = or_marker(prompt_value(item.get("address"), 96), "?");
    let agent = or_marker(prompt_value(item.get("agent"), 80), "agente");
    let task = prompt_value(item.get("task"), 220);
    let task = or_marker(task, "tarea sin describir");
    format!("- {address} · {agent} · «{task}»")
}

fn chain_line(item: &Row) -> String {
    let source = or_marker(prompt_value(item.get("source"), 96), "?");
    let agent = or_marker(prompt_value(item.get("agent"), 80), "agente");
    let task = prompt_value(item.get("task"), 220);
    let task = or_marker(task, "tarea sin describir");
    format!("- cadena: cuando termine {source} → {agent} · «{task}»")
}

/// Tareas pendientes indexadas por la dirección del agente de Herdr.
pub struct TaskRegistry {
    pub path: PathBuf,
    state: Mutex<State>,
}

impl TaskRegistry {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let state = load(&path);
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_locked(&self, state: &State) -> std::io::Result<()> {
        let mut data = Map::new();
        data.insert("version".to_string(), Value::from(1));
        data.insert(
            "tasks".to_string(),
            Value::Array(tasks_of(state).into_iter().map(Value::Object).collect()),
        );
        data.insert(
            "chains".to_string(),
            Value::Array(state.chains.iter().cloned().map(Value::Object).collect()),
        );
        let options = DumpOptions::utf8().with_indent(2);
        let text = format!("{}\n", dumps_with(&Value::Object(data), &options));
        write_atomic(&self.path, &text)
    }

    pub fn all(&self) -> Vec<Row> {
        tasks_of(&self.lock())
    }

    pub fn get(&self, address: &str) -> Option<Row> {
        let key = py_strip(address);
        if key.is_empty() {
            return None;
        }
        self.lock()
            .tasks
            .get(key)
            .and_then(Value::as_object)
            .cloned()
    }

    pub fn add(
        &self,
        address: &str,
        agent: &str,
        task: &str,
        created_at: Option<f64>,
    ) -> std::io::Result<Option<Row>> {
        let key = py_strip(address).to_string();
        if key.is_empty() {
            return Ok(None);
        }
        let mut entry = Map::new();
        entry.insert("address".to_string(), Value::String(key.clone()));
        entry.insert("agent".to_string(), Value::String(agent.to_string()));
        entry.insert(
            "task".to_string(),
            Value::String(py_strip(task).to_string()),
        );
        entry.insert(
            "created_at".to_string(),
            Value::from(created_at.unwrap_or_else(timestamp)),
        );
        let mut state = self.lock();
        state.tasks.insert(key, Value::Object(entry.clone()));
        self.write_locked(&state)?;
        Ok(Some(entry))
    }

    pub fn remove(&self, address: &str) -> std::io::Result<Option<Row>> {
        let key = py_strip(address);
        if key.is_empty() {
            return Ok(None);
        }
        let mut state = self.lock();
        let Some(item) = state.tasks.remove(key).and_then(|item| match item {
            Value::Object(row) => Some(row),
            _ => None,
        }) else {
            return Ok(None);
        };
        self.write_locked(&state)?;
        Ok(Some(item))
    }

    pub fn clear(&self) -> std::io::Result<()> {
        let mut state = self.lock();
        if state.tasks.is_empty() && state.chains.is_empty() {
            return Ok(());
        }
        state.tasks.clear();
        state.chains.clear();
        self.write_locked(&state)
    }

    pub fn add_chain(&self, spec: &Map<String, Value>) -> std::io::Result<Option<Row>> {
        let source = stripped(spec.get("source"));
        let agent = stripped(spec.get("agent"));
        if source.is_empty() || agent.is_empty() {
            return Ok(None);
        }
        let mut entry = spec.clone();
        entry.insert("source".to_string(), Value::String(source));
        entry.insert("agent".to_string(), Value::String(agent));
        entry.insert(
            "task".to_string(),
            Value::String(stripped(spec.get("task"))),
        );
        entry.insert(
            "created_at".to_string(),
            Value::from(number_or(spec.get("created_at"), timestamp())),
        );
        let mut state = self.lock();
        state.chains.push(entry.clone());
        self.write_locked(&state)?;
        Ok(Some(entry))
    }

    pub fn chains(&self) -> Vec<Row> {
        chains_of(&self.lock())
    }

    pub fn chains_for(&self, source: &str) -> Vec<Row> {
        let key = py_strip(source);
        if key.is_empty() {
            return Vec::new();
        }
        self.chains()
            .into_iter()
            .filter(|item| {
                item.get("source")
                    .is_some_and(|value| python_str(value) == key)
            })
            .collect()
    }

    pub fn remove_chains_for(&self, source: &str) -> std::io::Result<Vec<Row>> {
        let key = py_strip(source);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let mut state = self.lock();
        let removed: Vec<Row> = state
            .chains
            .iter()
            .filter(|item| {
                item.get("source")
                    .is_some_and(|value| python_str(value) == key)
            })
            .cloned()
            .collect();
        if removed.is_empty() {
            return Ok(removed);
        }
        state.chains.retain(|item| {
            !item
                .get("source")
                .is_some_and(|value| python_str(value) == key)
        });
        self.write_locked(&state)?;
        Ok(removed)
    }

    /// Quita una sola etapa en espera (misma fuente, agente y creación).
    pub fn remove_chain(&self, spec: &Map<String, Value>) -> std::io::Result<bool> {
        let ident = (
            spec.get("source").unwrap_or(&Value::Null).clone(),
            spec.get("agent").unwrap_or(&Value::Null).clone(),
            spec.get("created_at").unwrap_or(&Value::Null).clone(),
        );
        let mut state = self.lock();
        let before = state.chains.len();
        state.chains.retain(|item| {
            let current = (
                item.get("source").unwrap_or(&Value::Null).clone(),
                item.get("agent").unwrap_or(&Value::Null).clone(),
                item.get("created_at").unwrap_or(&Value::Null).clone(),
            );
            !ez_core::pyvalue::py_eq(&current.0, &ident.0)
                || !ez_core::pyvalue::py_eq(&current.1, &ident.1)
                || !ez_core::pyvalue::py_eq(&current.2, &ident.2)
        });
        if state.chains.len() == before {
            return Ok(false);
        }
        self.write_locked(&state)?;
        Ok(true)
    }

    pub fn prompt_block(&self) -> String {
        let (tasks, chains) = {
            let state = self.lock();
            (tasks_of(&state), chains_of(&state))
        };
        if tasks.is_empty() && chains.is_empty() {
            return String::new();
        }
        let mut lines = vec!["<pending_tasks>".to_string()];
        // DECISION: bound prompt context to recent work, retaining the full registry.
        for item in tasks.iter().skip(tasks.len().saturating_sub(12)) {
            lines.push(task_line(item));
        }
        for chain in chains.iter().skip(chains.len().saturating_sub(8)) {
            lines.push(chain_line(chain));
        }
        if tasks.len() > 12 || chains.len() > 8 {
            lines.push(format!(
                "Otras pendientes: {} tareas y {} cadenas.",
                tasks.len().saturating_sub(12),
                chains.len().saturating_sub(8)
            ));
        }
        lines.push("</pending_tasks>".to_string());
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests;
