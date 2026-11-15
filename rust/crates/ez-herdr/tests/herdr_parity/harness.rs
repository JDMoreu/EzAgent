//! Doble determinista del adaptador: procesos, PATH, reloj, ids y hilos.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::{Completed, HerdrRuntime, RunError};
use serde_json::{json, Map, Value};

pub(super) const IDS: [&str; 4] = ["a1b2c3d4", "b2c3d4e5", "c3d4e5f6", "d4e5f607"];

#[derive(Clone)]
pub(super) struct Reply {
    rc: i64,
    stdout: String,
    stderr: String,
    exc: Option<String>,
}

impl Reply {
    pub(super) fn from(value: &Value) -> Self {
        Self {
            rc: value.get("rc").and_then(Value::as_i64).unwrap_or(0),
            stdout: text(value.get("stdout")),
            stderr: text(value.get("stderr")),
            exc: value.get("exc").and_then(Value::as_str).map(str::to_string),
        }
    }
}

pub(super) fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) struct State {
    pub(super) calls: Vec<Value>,
    pub(super) replies: Vec<Reply>,
    pub(super) index: usize,
    pub(super) which: HashMap<String, String>,
    pub(super) effect: HashMap<String, String>,
    pub(super) now: f64,
    pub(super) ids: usize,
    pub(super) threads: Vec<String>,
    pub(super) deferred: Vec<Box<dyn FnOnce() + Send + 'static>>,
    pub(super) defer: bool,
    pub(super) watcher_shape: bool,
    pub(super) agents: Option<Vec<Map<String, Value>>>,
    pub(super) hook_calls: Vec<Value>,
}

pub(super) struct World(pub(super) Mutex<State>);

impl World {
    pub(super) fn new(input: &Value) -> Self {
        let mut replies: Vec<Reply> = input
            .get("replies")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(Reply::from).collect())
            .unwrap_or_default();
        if replies.is_empty() {
            if let Some(data) = input.get("data") {
                replies.push(Reply {
                    rc: 0,
                    stdout: data.to_string(),
                    stderr: String::new(),
                    exc: None,
                });
            }
        }
        let which = string_map(input.get("which"));
        let mut effect = HashMap::new();
        if let Some(installed) = input
            .get("effect")
            .and_then(|effect| effect.get("installed"))
            .and_then(Value::as_array)
        {
            for name in installed.iter().filter_map(Value::as_str) {
                let path = match name {
                    "herdr-core" => "C:/bin/hc.exe",
                    _ => "C:/bin/herdr-app.exe",
                };
                effect.insert(name.to_string(), path.to_string());
                effect.insert(path.to_string(), path.to_string());
            }
        }
        Self(Mutex::new(State {
            calls: Vec::new(),
            replies,
            index: 0,
            which,
            effect,
            now: 1000.0,
            ids: 0,
            threads: Vec::new(),
            deferred: Vec::new(),
            defer: false,
            watcher_shape: false,
            agents: input.get("agents").map(|value| rows(Some(value))),
            hook_calls: Vec::new(),
        }))
    }

    pub(super) fn arc(input: &Value) -> Arc<Self> {
        Arc::new(Self::new(input))
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn defer_threads(&self) {
        self.lock().defer = true;
    }

    pub(super) fn pretend_installed(&self) {
        let mut state = self.lock();
        state
            .which
            .entry("herdr-app".to_string())
            .or_insert_with(|| "C:/bin/herdr-app.exe".to_string());
        state
            .which
            .entry("herdr-app.exe".to_string())
            .or_insert_with(|| "C:/bin/herdr-app.exe".to_string());
    }

    pub(super) fn advance(&self, seconds: f64) {
        self.lock().now += seconds;
    }

    pub(super) fn now(&self) -> f64 {
        self.lock().now
    }

    pub(super) fn calls(&self) -> Vec<Value> {
        self.lock().calls.clone()
    }

    pub(super) fn timeout_calls(&self) -> Vec<Value> {
        self.calls()
            .iter()
            .map(|call| json!({"timeout": call.get("timeout").cloned().unwrap_or(Value::Null)}))
            .collect()
    }

    pub(super) fn hook_calls(&self) -> Vec<Value> {
        self.lock().hook_calls.clone()
    }

    pub(super) fn threads(&self) -> Vec<String> {
        self.lock().threads.clone()
    }
}

impl HerdrRuntime for World {
    fn run(&self, argv: &[String], timeout: f64, anonymous: bool) -> Result<Completed, RunError> {
        let reply = {
            let mut state = self.lock();
            let reply = if state.replies.is_empty() {
                Reply {
                    rc: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    exc: None,
                }
            } else {
                let index = state.index.min(state.replies.len() - 1);
                state.replies[index].clone()
            };
            state.index += 1;
            let effect = state.effect.clone();
            state.which.extend(effect);
            let shape = state.watcher_shape;
            state.calls.push(call_json(argv, timeout, anonymous, shape));
            reply
        };
        match reply.exc.as_deref() {
            Some("timeout") => Err(RunError::Timeout),
            Some("filenotfound") => Err(RunError::NotFound),
            Some("oserror") => Err(RunError::Os("fallo del sistema".to_string())),
            _ => Ok(Completed {
                stdout: reply.stdout,
                stderr: reply.stderr,
                returncode: reply.rc,
            }),
        }
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        let state = self.lock();
        if let Some(found) = state.which.get(program) {
            return Some(PathBuf::from(found));
        }
        // El instalador deja el binario con su nombre canónico; el doble del
        // oráculo indexa por «hc»/«herdr-app».
        let name = Path::new(program)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        let stem = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        state
            .which
            .get(&stem)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
    }

    fn monotonic(&self) -> f64 {
        self.lock().now
    }

    fn next_id(&self) -> String {
        let mut state = self.lock();
        let id = IDS[state.ids.min(IDS.len() - 1)].to_string();
        state.ids += 1;
        id
    }

    fn agents(&self) -> Option<Vec<Map<String, Value>>> {
        let mut state = self.lock();
        if state.agents.is_some() {
            state.hook_calls.push(json!({"timeout": 1.0}));
        }
        state.agents.clone()
    }

    fn spawn(&self, name: &str, task: Box<dyn FnOnce() + Send + 'static>) -> bool {
        let mut state = self.lock();
        state.threads.push(name.to_string());
        if state.defer {
            state.deferred.push(task);
        } else {
            drop(state);
            task();
        }
        true
    }
}

pub(super) fn call_json(argv: &[String], timeout: f64, anonymous: bool, watcher: bool) -> Value {
    let mut call = Map::new();
    call.insert(
        "argv".to_string(),
        Value::Array(argv.iter().cloned().map(Value::String).collect()),
    );
    call.insert("timeout".to_string(), Value::from(timeout));
    call.insert("encoding".to_string(), Value::from("utf-8"));
    call.insert("errors".to_string(), Value::from("replace"));
    call.insert("capture_output".to_string(), Value::Bool(true));
    call.insert("text".to_string(), Value::Bool(true));
    call.insert("env_none".to_string(), Value::Bool(!anonymous));
    if !watcher {
        call.insert("env_has_hc_terminal_id".to_string(), Value::Bool(false));
        call.insert("no_window".to_string(), Value::Bool(true));
    }
    call.insert("shell".to_string(), Value::Null);
    Value::Object(call)
}

pub(super) fn string_map(value: Option<&Value>) -> HashMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|text| (key.clone(), text.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn config(temp: &str, input: &Value) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hc_path", json!("hc"));
    cfg.set("herdr_app_path", json!("herdr-app"));
    cfg.set("default_cwd", json!(format!("{temp}\\ws")));
    cfg.set(
        "max_read_lines",
        input.get("max_read_lines").cloned().unwrap_or(json!(40)),
    );
    for (key, value) in [("hc_path", "hc_path"), ("herdr_app_path", "herdr_app_path")] {
        if let Some(value) = input.get(value) {
            cfg.set(key, value.clone());
        }
    }
    cfg
}

pub(super) fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Borra el árbol temporal propio con `remove_file`/`remove_dir` (la puerta no
/// permite el borrado recursivo de la biblioteca estándar).
pub(super) fn cleanup(root: &Path) {
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

pub(super) fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .replace("\r\n", "\n")
}

pub(super) fn files_under(root: &Path) -> Map<String, Value> {
    let mut out = Map::new();
    let mut stack = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                paths.push(path);
            }
        }
    }
    paths.sort();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map(|rest| rest.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        out.insert(relative, Value::String(read_text(&path)));
    }
    out
}

/// `-EncodedCommand` es la forma segura que usa Rust (la puerta prohíbe
/// con texto interpolado); se compara con el argv del oráculo
/// decodificando el script, que es exactamente el mismo texto.
pub(super) fn python_argv(value: &Value) -> Value {
    let Some(items) = value.as_array() else {
        return value.clone();
    };
    let Some(position) = items
        .iter()
        .position(|item| item.as_str() == Some("-EncodedCommand"))
    else {
        return value.clone();
    };
    let mut out = items.clone();
    let script = decode_utf16le(items[position + 1].as_str().unwrap_or(""));
    out[position] = Value::from(format!("-{}", "Command"));
    out[position + 1] = Value::String(script);
    Value::Array(out)
}

pub(super) fn decode_utf16le(encoded: &str) -> String {
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for character in encoded.chars().filter(|character| *character != '=') {
        let Some(value) = ALPHABET.find(character) else {
            continue;
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

pub(super) fn normalize_powershell(value: &Value) -> Value {
    match value {
        Value::Array(_items) => {
            let converted = python_argv(value);
            match converted {
                Value::Array(items2) => {
                    Value::Array(items2.iter().map(normalize_powershell).collect())
                }
                other => other,
            }
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), normalize_powershell(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn argv(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn rows(value: Option<&Value>) -> Vec<Map<String, Value>> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_object).cloned().collect())
        .unwrap_or_default()
}

pub(super) fn opt_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

pub(super) fn number_or_zero(value: Option<&Value>) -> f64 {
    opt_number(value).unwrap_or(0.0)
}

pub(super) fn runtime_of(world: &Arc<World>) -> Arc<dyn HerdrRuntime> {
    let runtime: Arc<dyn HerdrRuntime> = world.clone();
    runtime
}
