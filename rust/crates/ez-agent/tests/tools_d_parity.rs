//! Paridad de `tools_d.py` contra `parity/fixtures/tools_d.jsonl` (P-AGENT-B).
//!
//! Un solo `#[test]`: los casos cambian el directorio actual y escriben
//! `tools.d/*.toml`. El proceso de los subprocesos se sustituye por un doble,
//! igual que el oráculo.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use ez_agent::registry::ToolSpec;
use ez_agent::tools_d::{
    call_handler, clean_environment, load_tools_d, load_tools_d_with, Completed, RunError, Runner,
};
use ez_core::agent_config::AgentConfig;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_d.jsonl";
const EXECUTABLES: [&str; 2] = ["echo", "code"];

struct CurrentDir(PathBuf);

impl CurrentDir {
    fn change_to(target: &Path) -> Self {
        let previous = std::env::current_dir().expect("directorio actual");
        std::env::set_current_dir(target).expect("directorio temporal");
        Self(previous)
    }
}

impl Drop for CurrentDir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

fn base_config() -> AgentConfig {
    std::fs::create_dir_all("app").expect("carpeta app");
    let (config, _) = AgentConfig::load(Some(Path::new("app/permissions.toml")));
    config
}

fn config_for(directory: &str, executables: &[String], base: &AgentConfig) -> AgentConfig {
    let mut config = base.clone();
    config.workspace = PathBuf::from("ws");
    config.tools_d_dir = PathBuf::from(directory);
    config.tools_d.allowed_executables = executables.to_vec();
    config.paths.read_roots = vec!["ws".into()];
    config.paths.write_roots = vec!["ws".into()];
    // El oráculo usa esta política solo en los casos que la necesitan; aquí se
    // deja fija porque ninguna otra ruta de los casos cae dentro.
    config.paths.deny = vec!["ws/secretos".into()];
    config.paths.deny_globs = vec!["**/.env".into()];
    config
}

fn write_files(directory: &Path, files: &Value) {
    std::fs::create_dir_all(directory).expect("carpeta tools.d");
    for (name, content) in files.as_object().expect("mapa de archivos") {
        std::fs::write(directory.join(name), content.as_str().expect("texto"))
            .expect("archivo de tools.d");
    }
}

fn executables_of(case: &FixtureCase) -> Vec<String> {
    case.input["executables"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| EXECUTABLES.iter().map(|name| (*name).to_string()).collect())
}

fn spec_dict(spec: &ToolSpec) -> Value {
    let info = spec.describe(&serde_json::Map::new());
    json!({
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.parameters,
        "level": spec.level,
        "untrusted_output": spec.untrusted_output,
        "timeout_s": spec.timeout_s,
        "schema": spec.schema(),
        "info": {
            "level": info.level,
            "bulk": info.bulk,
            "always_confirm": info.always_confirm,
            "summary": info.summary,
        },
    })
}

fn normalize(value: &Value, home: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(home, "<tmp>")),
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

/// `_expand`: las rutas del fixture usan `<tmp>` para el perfil del oráculo.
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

fn compare_load(case: &FixtureCase, base: &AgentConfig, index: usize, home: &str) {
    let directory = format!("tools.d/load-{index:03}");
    if let Some(files) = case.input.get("files") {
        write_files(Path::new(&directory), files);
    }
    let config = config_for(&directory, &executables_of(case), base);
    let tools = load_tools_d(&config);
    let produced = if case.output["tools"]
        .as_array()
        .and_then(|items| items.first())
        .is_some_and(Value::is_object)
    {
        Value::Array(tools.iter().map(spec_dict).collect())
    } else {
        Value::Array(tools.iter().map(|tool| json!(tool.name)).collect())
    };
    assert_eq!(
        normalize(&produced, home),
        case.output["tools"],
        "caso {}",
        case.case
    );
}

fn reply_runner(reply: &Value, calls: Arc<Mutex<Vec<Value>>>) -> Runner {
    let reply = reply.clone();
    Arc::new(
        move |argv: &[String], cwd: &Path, env: &[(String, String)], timeout: f64| {
            let mut keys: Vec<&str> = env.iter().map(|(key, _)| key.as_str()).collect();
            keys.sort_unstable();
            calls.lock().expect("registro").push(json!({
                "argv": argv,
                "shell": false,
                "cwd": cwd.display().to_string(),
                "timeout": timeout,
                "encoding": "utf-8",
                "errors": "replace",
                "check": false,
                "env_keys": keys,
            }));
            match reply["exc"].as_str() {
                Some("timeout") => Err(RunError::Timeout {
                    stdout: String::new(),
                }),
                Some("oserror") => Err(RunError::Os("no se encontró el programa".into())),
                _ => Ok(Completed {
                    stdout: reply["stdout"].as_str().unwrap_or("hola\n").to_string(),
                    stderr: reply["stderr"].as_str().unwrap_or("").to_string(),
                    returncode: reply["rc"].as_i64().unwrap_or(0),
                }),
            }
        },
    )
}

fn compare_handler(case: &FixtureCase, base: &AgentConfig, index: usize, home: &str) {
    let directory = format!("tools.d/handler-{index:03}");
    write_files(
        Path::new(&directory),
        &json!({"tool.toml": case.input["tool"]}),
    );
    let config = config_for(&directory, &EXECUTABLES.map(str::to_string), base);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let tools = load_tools_d_with(
        &config,
        reply_runner(&case.input["reply"], Arc::clone(&calls)),
    );
    let spec = tools.first().expect("una herramienta válida");
    let args = expand(&case.input["args"], home);
    let produced = normalize(&call_handler(spec, &args).to_dict(), home);
    assert_eq!(produced, case.output["result"], "caso {}", case.case);
    let recorded = normalize(&Value::Array(calls.lock().expect("registro").clone()), home);
    assert_eq!(
        recorded, case.output["calls"],
        "caso {}: llamadas",
        case.case
    );
}

#[test]
fn every_tools_d_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tools_d legibles");
    let home = std::env::temp_dir().join(format!(
        "ez-tools-d-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&home).expect("carpeta temporal");
    let home_path = home.canonicalize().expect("ruta canónica");
    let _guard = CurrentDir::change_to(&home_path);
    let raw_home = std::env::current_dir()
        .expect("directorio temporal")
        .display()
        .to_string();
    // `canonicalize` devuelve la forma verbatim de Windows; el oráculo usa la normal.
    let home = raw_home
        .strip_prefix(r"\\?\")
        .unwrap_or(&raw_home)
        .to_string();
    std::fs::create_dir_all("ws").expect("workspace");
    let base = base_config();

    let mut cases_run = 0;
    for (index, case) in cases.iter().enumerate() {
        match case.function.as_str() {
            "load_tools_d" => compare_load(case, &base, index, &home),
            "ToolSpec.handler" => compare_handler(case, &base, index, &home),
            "clean_environment" => {
                let environment = clean_environment();
                let mut keys: Vec<&str> = environment.iter().map(|(key, _)| key.as_str()).collect();
                keys.sort_unstable();
                assert_eq!(json!(keys), case.output["keys"], "caso {}", case.case);
                assert!(case.output["all_str"].as_bool().unwrap_or(false));
            }
            other => panic!("función desconocida: {other}"),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 82, "muy pocos casos: {cases_in_file}");
}
