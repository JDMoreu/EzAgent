//! Port of `src/ezagent/voice/config.py` (F2-02).
//!
//! El dataclass de Python no valida tipos: guarda lo que traiga `voice.json`.
//! Para reproducirlo tal cual, `VoiceConfig` envuelve un mapa JSON ordenado con
//! los 88 campos y sus valores; `load` copia lo que lee y `save` lo escribe con
//! `ez_core::pyjson` byte a byte (mismos separadores, orden, sangría y
//! flotantes).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::dirs;
use crate::pyjson::{dumps_with, DumpOptions};

/// Versión de los perfiles de fábrica (`AGENT_PROFILES_VERSION`).
pub const AGENT_PROFILES_VERSION: i64 = 8;

/// Campos de conocimiento del CLI que se refrescan al subir la versión.
const PROFILE_KNOWLEDGE_KEYS: [&str; 16] = [
    "label",
    "aliases",
    "models",
    "efforts",
    "model_efforts",
    "prelaunch",
    "discover",
    "model_command",
    "model_args",
    "effort_args",
    "model_prefix",
    "env",
    "env_args",
    "task_args",
    "session_commands",
    "notes",
];

/// Ruta de `voice.json` (`%APPDATA%\EzAgent\voice.json` y equivalentes).
pub fn config_path() -> PathBuf {
    dirs::config_path()
}

mod defaults;
mod llm_env;

use self::defaults::default_values;
use self::llm_env::{env_text, resolve_llm, ResolvedLlm};

pub use self::defaults::{default_agents, default_profiles};

/// Configuración de `ezagent-voice`.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct VoiceConfig {
    values: Map<String, Value>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            values: default_values(),
        }
    }
}

impl VoiceConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    pub fn to_value(&self) -> Value {
        Value::Object(self.values.clone())
    }

    /// `VoiceConfig(**{k: v for k, v in data.items() if known})`.
    pub fn from_json(data: &Map<String, Value>) -> Self {
        let mut config = Self::new();
        for (key, value) in data {
            if config.values.contains_key(key) {
                config.values.insert(key.clone(), value.clone());
            }
        }
        config
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    fn truthy(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(truthy)
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.values.get(key).and_then(Value::as_f64)
    }

    /// `VoiceConfig.load`: valores de fábrica + archivo + migraciones + entorno.
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let path = path.map(Path::to_path_buf).unwrap_or_else(config_path);
        let data = match read_json_value(&path) {
            None => Map::new(),
            Some(Value::Object(map)) => map,
            Some(other) => {
                return Err(format!(
                    "'{}' object has no attribute 'items'",
                    python_type_name(&other)
                ))
            }
        };
        let mut config = Self::from_json(&data);
        // DECISION: «notch_hidden» pasó a «hud_hidden»; se lee la clave vieja.
        if !data.contains_key("hud_hidden") {
            if let Some(value) = data.get("notch_hidden") {
                config.set("hud_hidden", Value::Bool(truthy(value)));
            }
        }
        let override_profiles = config.get("agent_profiles").and_then(Value::as_object);
        let merged = merge_agent_profiles(None, override_profiles);
        config.set("agent_profiles", Value::Object(merged));
        if let Some(Value::Object(agents)) = config.values.get_mut("agents") {
            for (name, command) in default_agents() {
                agents.entry(name).or_insert(command);
            }
        }
        let has_data = !data.is_empty();
        let version = int_value(
            data.get("agent_profiles_version").unwrap_or(&Value::Null),
            1,
        );
        if has_data && version < AGENT_PROFILES_VERSION {
            config.refresh_profile_knowledge();
        }
        config.set("agent_profiles_version", json!(AGENT_PROFILES_VERSION));
        let vad = int_value(data.get("vad_defaults_version").unwrap_or(&Value::Null), 1);
        if has_data && vad < 2 {
            if config.number("min_rms") == Some(0.0015) {
                config.set("min_rms", json!(0.0006));
            }
            if config.number("min_speech_secs") == Some(0.3) {
                config.set("min_speech_secs", json!(0.2));
            }
            config.set("vad_defaults_version", json!(2));
        }
        let defaults = int_value(data.get("defaults_version").unwrap_or(&Value::Null), 1);
        if has_data && defaults < 2 {
            if config.number("stt_timeout_secs") == Some(60.0) {
                config.set("stt_timeout_secs", json!(10.0));
            }
            config.set("defaults_version", json!(2));
        }
        config.apply_env()?;
        Ok(config)
    }

    /// `apply_env`: el entorno y `llm.json` mandan sobre lo guardado.
    pub fn apply_env(&mut self) -> Result<(), String> {
        if self.truthy("llm_pinned") {
            let resolved = resolve_llm(
                self.text("llm_provider").filter(|text| !text.is_empty()),
                self.text("llm_model").filter(|text| !text.is_empty()),
                self.text("llm_base_url").filter(|text| !text.is_empty()),
                self.text("api_key").filter(|text| !text.is_empty()),
                self.text("llm_api_style").filter(|text| !text.is_empty()),
            );
            if let Ok(resolved) = resolved {
                self.apply_resolved(&resolved);
            }
        } else {
            let resolved = resolve_llm(None, None, None, None, None)?;
            if resolved.source == "env" || resolved.source == "file" {
                self.apply_resolved(&resolved);
            } else {
                if self.text("api_key").unwrap_or("").is_empty() {
                    self.set("api_key", json!(resolved.api_key));
                }
                if self.text("llm_api_style").unwrap_or("").is_empty() {
                    self.set("llm_api_style", json!(resolved.api_style));
                }
            }
        }
        if let Some(url) = env_text("WHISPER_URL").filter(|text| !text.is_empty()) {
            self.set("whisper_url", json!(url));
        }
        if let Some(engine) = env_text("WHISPER_ENGINE").filter(|text| !text.is_empty()) {
            self.set("stt_engine", json!(engine));
        }
        if self.get("output_device_index").is_none_or(Value::is_null) {
            if let Some(raw) = env_text("EZ_OUTPUT_DEVICE_INDEX").filter(|raw| !raw.is_empty()) {
                if let Some(index) = python_int(&raw) {
                    self.set("output_device_index", json!(index));
                }
            }
        }
        if !self.truthy("default_cwd") {
            if let Ok(directory) = std::env::current_dir() {
                self.set("default_cwd", json!(directory.to_string_lossy()));
            }
        }
        Ok(())
    }

    fn apply_resolved(&mut self, resolved: &ResolvedLlm) {
        self.set("llm_provider", json!(resolved.provider));
        self.set("llm_model", json!(resolved.model));
        self.set("llm_base_url", json!(resolved.base_url));
        self.set("api_key", json!(resolved.api_key));
        self.set("llm_api_style", json!(resolved.api_style));
    }

    fn refresh_profile_knowledge(&mut self) {
        let defaults = default_profiles();
        let Some(Value::Object(profiles)) = self.values.get_mut("agent_profiles") else {
            return;
        };
        for (name, default) in &defaults {
            if !profiles.get(name).is_some_and(Value::is_object) {
                profiles.insert(name.clone(), json!({}));
            }
            let Some(Value::Object(profile)) = profiles.get_mut(name) else {
                continue;
            };
            let default = default.as_object().cloned().unwrap_or_default();
            for key in PROFILE_KNOWLEDGE_KEYS {
                match default.get(key) {
                    Some(value) => {
                        profile.insert(key.to_string(), value.clone());
                    }
                    None => {
                        profile.shift_remove(key);
                    }
                }
            }
        }
    }

    /// `save`: escritura atómica y con merge de claves desconocidas.
    pub fn save(&self, path: Option<&Path>) -> Result<(), String> {
        let path = path.map(Path::to_path_buf).unwrap_or_else(config_path);
        let mut data = read_json_object(&path);
        for (key, value) in &self.values {
            data.insert(key.clone(), value.clone());
        }
        data.shift_remove("notch_hidden");
        // La clave vive en .env; no se duplica en voice.json.
        data.insert("api_key".to_string(), Value::String(String::new()));
        write_json(&path, &data)
    }

    /// `save_key`: persiste una sola clave instantánea (`hud_hidden` o `theme`).
    pub fn save_key(&self, key: &str, value: Value, path: Option<&Path>) -> Result<(), String> {
        let path = path.map(Path::to_path_buf).unwrap_or_else(config_path);
        let mut data = read_json_object(&path);
        data.insert(key.to_string(), value);
        write_json(&path, &data)
    }
}

/// `normalize_agent_name`: forma comparable de un nombre dictado.
pub fn normalize_agent_name(value: &str) -> String {
    value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .collect::<String>()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        .collect()
}

/// `resolve_agent_key`: clave canónica de un agente a partir de cómo se dictó.
pub fn resolve_agent_key(
    name: &str,
    agents: &Map<String, Value>,
    profiles: Option<&Map<String, Value>>,
) -> Option<String> {
    let wanted = normalize_agent_name(name);
    if wanted.is_empty() {
        return None;
    }
    for key in agents.keys() {
        if normalize_agent_name(key) == wanted {
            return Some(key.clone());
        }
    }
    for (key, profile) in profiles? {
        if !agents.contains_key(key) {
            continue;
        }
        let Value::Object(profile) = profile else {
            continue;
        };
        let Some(Value::Array(aliases)) = profile.get("aliases") else {
            continue;
        };
        let found = aliases
            .iter()
            .any(|alias| normalize_agent_name(&python_str(alias)) == wanted);
        if found {
            return Some(key.clone());
        }
    }
    None
}

/// `merge_agent_profiles`: mezcla perfiles conservando los valores por defecto.
pub fn merge_agent_profiles(
    base: Option<&Map<String, Value>>,
    override_profiles: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let mut result = base.cloned().unwrap_or_else(default_profiles);
    let Some(overrides) = override_profiles else {
        return result;
    };
    for (raw_name, raw_profile) in overrides {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        let Value::Object(raw_profile) = raw_profile else {
            continue;
        };
        let mut profile = match result.get(name) {
            Some(Value::Object(existing)) => existing.clone(),
            _ => Map::new(),
        };
        for (key, value) in raw_profile {
            profile.insert(key.clone(), value.clone());
        }
        result.insert(name.to_string(), Value::Object(profile));
    }
    result
}

/// Límite de `int()` para cadenas (`sys.int_info.str_digits_check_threshold`).
const MAX_STR_DIGITS: usize = 4300;

/// Inicio de cada decena de dígitos decimales (`Nd`) de Unicode 15.0.
const DECIMAL_STARTS: [u32; 68] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650,
    0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16A60, 0x16AC0,
    0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E950,
    0x1FBF0,
];

/// `VoiceConfig._int`: `int(value)` de Python, con `default` si falla.
pub fn int_value(value: &Value, default: i64) -> i64 {
    match value {
        Value::Bool(flag) => i64::from(*flag),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                return integer;
            }
            match number.as_f64() {
                Some(float) if float.is_finite() => float.trunc() as i64,
                _ => default,
            }
        }
        Value::String(text) => python_int(text).unwrap_or(default),
        _ => default,
    }
}

/// `int(texto)` de Python: signo ASCII, dígitos decimales (`Nd`) y guiones bajos
/// simples entre dígito y dígito.
pub fn python_int(text: &str) -> Option<i64> {
    // `int()` recorta el espacio Unicode (`White_Space`), no `\x1c`-`\x1f`:
    // el mismo conjunto que `char::is_whitespace`.
    let trimmed = text.trim_matches(char::is_whitespace);
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1i128, rest),
        None => (1i128, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let mut value: i128 = 0;
    let mut count = 0usize;
    let mut after_digit = false;
    for character in digits.chars() {
        if character == '_' {
            if !after_digit {
                return None;
            }
            after_digit = false;
            continue;
        }
        let digit = decimal_digit(character)?;
        after_digit = true;
        count += 1;
        value = value * 10 + i128::from(digit);
        if value > i128::from(i64::MAX) + 1 {
            return None;
        }
    }
    // `int()` limita las cadenas a `sys.int_info.str_digits_check_threshold`.
    if !after_digit || count > MAX_STR_DIGITS {
        return None;
    }
    i64::try_from(sign * value).ok()
}

/// Valor decimal de un dígito `Nd` de Unicode (Python acepta `int("１２")`).
fn decimal_digit(character: char) -> Option<u32> {
    let code = character as u32;
    let index = DECIMAL_STARTS.partition_point(|start| *start <= code);
    let start = *DECIMAL_STARTS.get(index.checked_sub(1)?)?;
    (code - start < 10).then_some(code - start)
}

/// `bool(value)` de Python.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_none_or(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// `str(value)` de Python para los tipos que llegan del JSON.
pub fn python_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(text) => text.clone(),
        other => crate::pyvalue::python_repr(other),
    }
}

fn python_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// `json.loads` del archivo; `None` si no existe o no es JSON legible.
fn read_json_value(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_json_object(path: &Path) -> Map<String, Value> {
    match read_json_value(path) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn write_json(path: &Path, data: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut options = DumpOptions::default().with_indent(2);
    options.ensure_ascii = false;
    let body = dumps_with(&Value::Object(data.clone()), &options);
    // `Path.write_text` traduce los finales de línea a los del sistema.
    let text = format!("{body}\n");
    let text = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text
    };
    let name = path.file_name().map(|name| name.to_string_lossy());
    let tmp = path.with_file_name(format!("{}.tmp", name.as_deref().unwrap_or_default()));
    let written = std::fs::write(&tmp, text.as_bytes()).and_then(|()| std::fs::rename(&tmp, path));
    let _ = std::fs::remove_file(&tmp);
    written.map_err(|error| format!("{}: {error}", path.display()))
}
