//! Paridad de `tools_core.py` contra `parity/fixtures/tools_core.jsonl` (P-AGENT-B).
//!
//! Un solo `#[test]`: el oráculo congela el reloj y cambia `HOME` para `~`.
//! `get_datetime_at` recibe ese instante; `list_dir` y `read_file` se ejecutan
//! sobre un perfil temporal con los archivos que describe cada caso.

use std::path::{Path, PathBuf};

use chrono::{FixedOffset, TimeZone, Utc};
use ez_agent::registry::{ToolResult, ToolReturn, ToolSpec};
use ez_agent::tools_core::{build_core_tools, get_datetime_at};
use ez_core::agent_config::AgentConfig;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_core.jsonl";

fn default_config() -> AgentConfig {
    let (config, _) = AgentConfig::load(None);
    config
}

fn info_dict(spec: &ToolSpec, args: &serde_json::Map<String, Value>) -> Value {
    let info = spec.describe(args);
    json!({
        "level": info.level,
        "bulk": info.bulk,
        "always_confirm": info.always_confirm,
        "summary": info.summary,
    })
}

fn spec_dict(spec: &ToolSpec) -> Value {
    json!({
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.parameters,
        "level": spec.level,
        "untrusted_output": spec.untrusted_output,
        "timeout_s": spec.timeout_s,
        "schema": spec.schema(),
        "info": info_dict(spec, &serde_json::Map::new()),
    })
}

fn call(spec: &ToolSpec, args: &Value) -> ToolResult {
    let args = args.as_object().expect("argumentos objeto");
    match (spec.handler)(args) {
        Ok(ToolReturn::Result(result)) => result,
        Ok(ToolReturn::Value(data)) => ToolResult::ok(data),
        Err(message) => ToolResult::failed(message),
    }
}

fn normalize(value: &Value, home: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(home, "<tmp>").replace('\\', "/")),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| normalize(item, home)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), normalize(item, home)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn expand(value: &Value, home: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("<tmp>", home)),
        Value::Array(items) => Value::Array(items.iter().map(|item| expand(item, home)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), expand(item, home)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `_write`: las claves con `<tmp>/` van al perfil; `null` crea una carpeta.
fn write_files(files: &Value, workspace: &Path, home: &Path) {
    for (relative, content) in files.as_object().expect("mapa de archivos") {
        let target = match relative.strip_prefix("<tmp>/") {
            Some(rest) => home.join(rest),
            None => workspace.join(relative),
        };
        if content.is_null() {
            std::fs::create_dir_all(&target).expect("carpeta del caso");
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("carpeta padre");
        }
        std::fs::write(&target, content.as_str().expect("texto")).expect("archivo del caso");
    }
}

fn case_config(workspace: &Path, home: &Path) -> AgentConfig {
    let docs = home.join("docs-root");
    let mut config = default_config();
    config.paths.read_roots = vec![workspace.display().to_string(), docs.display().to_string()];
    config.paths.write_roots = vec![workspace.display().to_string()];
    config.paths.deny = vec![workspace.join("secretos").display().to_string()];
    config.paths.deny_globs = vec!["**/.env".into(), "**/*.pem".into()];
    config.workspace = workspace.to_path_buf();
    config
}

/// Instante congelado del caso (`summerNN` usa otro reloj en el oráculo).
fn instant_of(case: &FixtureCase) -> chrono::DateTime<Utc> {
    match case.case.rsplit('/').next().unwrap_or_default() {
        "summer01" => Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap(),
        "summer02" => Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap(),
        "summer03" => Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        _ => Utc.with_ymd_and_hms(2025, 1, 1, 12, 34, 56).unwrap(),
    }
}

fn compare_datetime(case: &FixtureCase) {
    let args = case.input["args"].as_object().expect("argumentos");
    let instant = instant_of(case);
    let local = instant.with_timezone(&FixedOffset::east_opt(0).expect("UTC"));
    let produced = get_datetime_at(args, instant, local, "UTC");
    assert_eq!(
        produced.to_dict(),
        case.output["result"],
        "caso {}",
        case.case
    );
    assert_eq!(
        json!({"level": 0, "bulk": 0, "always_confirm": false, "summary": ""}),
        case.output["info"],
        "caso {}",
        case.case
    );
}

fn compare_tool(case: &FixtureCase, home: &Path, label: &str) {
    let workspace = home.join("core-cases").join(label).join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace del caso");
    write_files(&case.input["files"], &workspace, home);
    let config = case_config(&workspace, home);
    let tools = build_core_tools(&config);
    let spec = tools
        .iter()
        .find(|tool| tool.name == case.function)
        .expect("herramienta del caso");
    let args = expand(&case.input["args"], &home.display().to_string());
    let map = args.as_object().expect("argumentos objeto");
    let produced = call(spec, &args);
    let prefix = home.display().to_string();
    assert_eq!(
        normalize(&produced.to_dict(), &prefix),
        case.output["result"],
        "caso {}",
        case.case
    );
    assert_eq!(
        normalize(&info_dict(spec, map), &prefix),
        case.output["info"],
        "caso {}",
        case.case
    );
}

#[test]
fn every_tools_core_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tools_core legibles");
    let home = std::env::temp_dir().join(format!(
        "ez-tools-core-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&home).expect("carpeta temporal");
    let raw_home = home
        .canonicalize()
        .expect("ruta canónica")
        .display()
        .to_string();
    let home_text = raw_home
        .strip_prefix(r"\\?\")
        .unwrap_or(&raw_home)
        .to_string();
    let home = PathBuf::from(&home_text);
    let previous_home = std::env::var_os("HOME");
    let previous_profile = std::env::var_os("USERPROFILE");
    std::env::set_var("HOME", &home_text);
    std::env::set_var("USERPROFILE", &home_text);
    let _restore = EnvRestore {
        home: previous_home,
        profile: previous_profile,
    };

    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "build_core_tools" => {
                let config = case_config(&home.join("core-cases").join("specs"), &home);
                let tools: Vec<Value> = build_core_tools(&config).iter().map(spec_dict).collect();
                assert_eq!(
                    Value::Array(tools),
                    case.output["tools"],
                    "caso {}",
                    case.case
                );
            }
            "get_datetime" => compare_datetime(case),
            "list_dir" | "read_file" => {
                let suffix = case.case.rsplit('/').next().unwrap_or_default();
                let label = format!("{}-{suffix}", case.function);
                compare_tool(case, &home, &label);
            }
            other => panic!("función desconocida: {other}"),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 86, "muy pocos casos: {cases_in_file}");
}

struct EnvRestore {
    home: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match &self.home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match &self.profile {
            Some(value) => std::env::set_var("USERPROFILE", value),
            None => std::env::remove_var("USERPROFILE"),
        }
    }
}
