//! Paridad de `voice/config.py` contra `parity/fixtures/config.jsonl` (F2-02).
//!
//! Un solo `#[test]` porque los casos tocan el entorno y el directorio actual,
//! que son del proceso: ejecutarlos en paralelo los haría competir.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ez_core::config::{
    config_path, default_agents, int_value, merge_agent_profiles, normalize_agent_name,
    resolve_agent_key, VoiceConfig,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/config.jsonl";

/// Variables que el oráculo de Python limpia antes de generar los casos.
const CLEARED_ENV: [&str; 19] = [
    "LLM_PROVIDER",
    "LLM_MODEL",
    "LLM_BASE_URL",
    "LLM_API_KEY",
    "LLM_API_STYLE",
    "OPENCODE_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GROQ_API_KEY",
    "CEREBRAS_API_KEY",
    "DEEPSEEK_API_KEY",
    "OPENROUTER_API_KEY",
    "WHISPER_URL",
    "WHISPER_ENGINE",
    "EZ_OUTPUT_DEVICE_INDEX",
    "EZ_LLM_CONFIG",
    "APPDATA",
    "XDG_CONFIG_HOME",
];

/// Newline que `Path.write_text` produce en este sistema.
fn line_separator() -> &'static str {
    if cfg!(windows) {
        "\r\n"
    } else {
        "\n"
    }
}

fn newline_kind(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "crlf"
    } else {
        "lf"
    }
}

#[test]
fn every_config_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de config legibles");
    let temp = std::env::temp_dir().join(format!("ez-config-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("appdata")).expect("carpeta temporal");
    let original_dir = std::env::current_dir().expect("directorio actual");
    std::env::set_current_dir(&temp).expect("directorio temporal");
    let guard = EnvGuard::capture(&CLEARED_ENV);
    for name in CLEARED_ENV {
        std::env::remove_var(name);
    }
    // El oráculo no tenía `llm.json`: `apply_env` parte de los presets.
    std::env::set_var("APPDATA", temp.join("appdata"));
    let prefixes = PathPrefixes::new(&temp);

    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "VoiceConfig" => compare_defaults(case),
            "VoiceConfig.load" => compare_load(case, &temp, &prefixes),
            "VoiceConfig.save" => compare_save(case, &temp, &prefixes),
            "VoiceConfig.save_key" => compare_save_key(case, &temp),
            "VoiceConfig._int" => compare_int(case),
            "config_path" => compare_config_path(case, &temp, &prefixes),
            "normalize_agent_name" => compare_normalize(case),
            "resolve_agent_key" => compare_resolve(case),
            "merge_agent_profiles" => compare_merge(case),
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        }
        cases_run += 1;
    }

    drop(guard);
    std::env::set_current_dir(&original_dir).expect("restaurar el directorio");
    let _ = std::fs::remove_dir_all(&temp);

    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "se saltó algún caso de {FIXTURES}"
    );
}

fn compare_defaults(case: &FixtureCase) {
    let produced = VoiceConfig::default().to_value();
    let expected = case.output.get("values").cloned().unwrap_or(Value::Null);
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn compare_load(case: &FixtureCase, temp: &Path, prefixes: &PathPrefixes) {
    let path = temp.join("voice.json");
    write_case_file(&path, case.input.get("file"));
    let saved = case
        .input
        .get("env")
        .and_then(Value::as_object)
        .map(|values| set_env(values, temp));
    let loaded = VoiceConfig::load(Some(&path));
    if let Some(saved) = saved {
        restore_env(saved);
    }
    match case.error.as_deref() {
        Some(expected) => {
            let error = loaded.expect_err(&format!("{} debía fallar", case.case));
            assert_eq!(error, expected, "caso {}", case.case);
        }
        None => {
            let config = loaded.unwrap_or_else(|_| panic!("{} debía cargar", case.case));
            let produced = prefixes.normalize(&config.to_value());
            assert_eq!(produced, case.output, "caso {}", case.case);
        }
    }
}

fn compare_save(case: &FixtureCase, temp: &Path, prefixes: &PathPrefixes) {
    let path = temp.join(path_of(case));
    write_case_file(&path, case.input.get("file"));
    let config = config_of(case);
    config.save(Some(&path)).expect("guardar voice.json");
    let text = std::fs::read_to_string(&path).expect("leer voice.json");
    let expected = case
        .output
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let expected = expected.replace("\r\n", line_separator());
    assert_eq!(text, expected, "texto de {}", case.case);
    if let Some(kind) = case.output.get("newline").and_then(Value::as_str) {
        assert_eq!(newline_kind(&text), kind, "final de línea de {}", case.case);
    }
    if let Some(expected_data) = case.output.get("data") {
        let data: Value = serde_json::from_str(&text).expect("json guardado");
        assert_eq!(&data, expected_data, "datos de {}", case.case);
    }
    if let Some(expected) = case.output.get("tmp_left") {
        let tmp = tmp_path(&path);
        assert_eq!(json!(tmp.exists()), *expected, "temporal de {}", case.case);
    }
    if let Some(expected) = case.output.get("exists") {
        assert_eq!(
            json!(path.exists()),
            *expected,
            "existencia de {}",
            case.case
        );
    }
    if case.output.get("round_trip_text").is_some() {
        compare_round_trip(case, temp, prefixes, &path, &text);
    }
}

fn compare_round_trip(
    case: &FixtureCase,
    temp: &Path,
    prefixes: &PathPrefixes,
    path: &Path,
    first: &str,
) {
    let second_path = temp.join("voice2.json");
    let reloaded = VoiceConfig::load(Some(path)).expect("recargar voice.json");
    reloaded
        .save(Some(&second_path))
        .expect("guardar voice2.json");
    let second = std::fs::read_to_string(&second_path).expect("leer voice2.json");
    let config = config_of(case);
    let checks = [
        ("round_trip_text", json!(first == second)),
        (
            "round_trip_values",
            json!(config.to_value() == reloaded.to_value()),
        ),
    ];
    for (key, produced) in checks {
        if let Some(expected) = case.output.get(key) {
            assert_eq!(produced, *expected, "{key} de {}", case.case);
        }
    }
    if let Some(expected) = case.output.get("reloaded") {
        assert_eq!(
            prefixes.normalize(&reloaded.to_value()),
            *expected,
            "recarga de {}",
            case.case
        );
    }
}

fn compare_save_key(case: &FixtureCase, temp: &Path) {
    let path = temp.join("voice.json");
    write_case_file(&path, case.input.get("file"));
    let config = VoiceConfig::default();
    let key = case.input.get("key").and_then(Value::as_str).unwrap_or("");
    let value = case.input.get("value").cloned().unwrap_or(Value::Null);
    config
        .save_key(key, value, Some(&path))
        .expect("guardar una clave");
    let text = std::fs::read_to_string(&path).expect("leer voice.json");
    let expected = case
        .output
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert_eq!(
        text,
        expected.replace("\r\n", line_separator()),
        "texto de {}",
        case.case
    );
    if let Some(expected_data) = case.output.get("data") {
        let data: Value = serde_json::from_str(&text).expect("json guardado");
        assert_eq!(&data, expected_data, "datos de {}", case.case);
    }
}

fn compare_int(case: &FixtureCase) {
    let value = case.input.get("value").cloned().unwrap_or(Value::Null);
    let default = case
        .input
        .get("default")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let produced = json!(int_value(&value, default));
    assert_eq!(
        produced,
        *case.output.get("int").unwrap_or(&Value::Null),
        "caso {}",
        case.case
    );
}

fn compare_config_path(case: &FixtureCase, temp: &Path, prefixes: &PathPrefixes) {
    let saved = case
        .input
        .get("env")
        .and_then(Value::as_object)
        .map(|values| set_env(values, temp));
    let produced = prefixes.normalize(&json!(config_path().to_string_lossy()));
    if let Some(saved) = saved {
        restore_env(saved);
    }
    let expected = case.output.get("path").cloned().unwrap_or(Value::Null);
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn compare_normalize(case: &FixtureCase) {
    let value = case
        .input
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("");
    let produced = json!(normalize_agent_name(value));
    let expected = case.output.get("name").cloned().unwrap_or(Value::Null);
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn compare_resolve(case: &FixtureCase) {
    let name = case.input.get("name").and_then(Value::as_str).unwrap_or("");
    let agents = case
        .input
        .get("agents")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(default_agents);
    let profiles = case.input.get("profiles").and_then(Value::as_object);
    let produced = json!(resolve_agent_key(name, &agents, profiles));
    let expected = case.output.get("key").cloned().unwrap_or(Value::Null);
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn compare_merge(case: &FixtureCase) {
    let base = case.input.get("base").and_then(Value::as_object);
    let overrides = case.input.get("override").and_then(Value::as_object);
    let produced = json!(merge_agent_profiles(base, overrides));
    let expected = case.output.get("profiles").cloned().unwrap_or(Value::Null);
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn config_of(case: &FixtureCase) -> VoiceConfig {
    let values = case
        .input
        .get("config")
        .and_then(Value::as_object)
        .expect("config del caso");
    VoiceConfig::from_json(values)
}

fn path_of(case: &FixtureCase) -> PathBuf {
    PathBuf::from(
        case.input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("voice.json"),
    )
}

fn tmp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|name| name.to_string_lossy());
    path.with_file_name(format!("{}.tmp", name.as_deref().unwrap_or_default()))
}

/// Escribe el archivo inicial del caso como lo hace el oráculo (`_put`).
fn write_case_file(path: &Path, file: Option<&Value>) {
    let _ = std::fs::remove_file(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta del caso");
    }
    match file {
        None | Some(Value::Null) => {}
        Some(Value::Array(items)) if items.len() == 2 && items[0] == json!("raw") => {
            let text = items[1].as_str().unwrap_or("");
            std::fs::write(path, text.as_bytes()).expect("archivo crudo");
        }
        Some(value) => {
            let mut options = ez_core::pyjson::DumpOptions::default().with_indent(2);
            options.ensure_ascii = false;
            let text = format!("{}\n", ez_core::pyjson::dumps_with(value, &options));
            std::fs::write(path, text.as_bytes()).expect("archivo json");
        }
    }
}

/// Aplica el entorno del caso sustituyendo `<tmp>` por la carpeta temporal.
fn set_env(values: &Map<String, Value>, temp: &Path) -> Vec<(String, Option<OsString>)> {
    let mut saved = Vec::new();
    for (name, value) in values {
        saved.push((name.clone(), std::env::var_os(name)));
        match value.as_str() {
            Some(text) => {
                // `_norm`/`_put` del oráculo sustituyen `<tmp>` por la carpeta
                // temporal; `pathlib` normaliza las barras de esa ruta.
                let (text, was_path) = (
                    text.replace("<tmp>", &temp.to_string_lossy()),
                    text.contains("<tmp>"),
                );
                let text = if cfg!(windows) && was_path {
                    text.replace('/', "\\")
                } else {
                    text
                };
                std::env::set_var(name, text);
            }
            None => std::env::remove_var(name),
        }
    }
    saved
}

fn restore_env(saved: Vec<(String, Option<OsString>)>) {
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(&name, value),
            None => std::env::remove_var(&name),
        }
    }
}

/// Sustituye las rutas de la máquina por `<tmp>`, como el oráculo.
struct PathPrefixes {
    prefixes: Vec<String>,
    home: String,
}

impl PathPrefixes {
    fn new(temp: &Path) -> Self {
        let current = std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temporary = temp.to_string_lossy().into_owned();
        let home = ez_core::dirs::home().to_string_lossy().into_owned();
        let mut prefixes = vec![current, temporary, home.clone()];
        prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
        prefixes.dedup();
        Self { prefixes, home }
    }

    fn normalize(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.normalize_text(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.normalize(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), self.normalize(item)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    fn normalize_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for prefix in &self.prefixes {
            if prefix.is_empty() {
                continue;
            }
            result = result.replace(prefix.as_str(), "<tmp>");
        }
        result = result.replace(self.home.as_str(), "<tmp>");
        result
    }
}

/// Guarda y restaura las variables de entorno que toca la prueba.
struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    fn capture(names: &[&str]) -> Self {
        Self {
            saved: names
                .iter()
                .map(|name| ((*name).to_string(), std::env::var_os(name)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
