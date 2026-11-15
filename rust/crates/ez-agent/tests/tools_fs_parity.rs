//! Paridad de `tools_fs.py` contra `parity/fixtures/tools_fs.jsonl` (P-AGENT-B).
//!
//! Un solo `#[test]`: cada caso monta su perfil temporal, registra la Papelera y
//! la apertura del sistema (el oráculo las sustituye por dobles) y congela el
//! reloj para que las copias `.bak` y el diario sean deterministas.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use ez_agent::journal::Journal;
use ez_agent::registry::{ToolReturn, ToolSpec};
use ez_agent::tools_fs::{
    build_fs_tools, build_fs_tools_with, count_items, count_phrase, Clock, Opener, Trash,
    TrashHandle,
};
use ez_core::agent_config::AgentConfig;
use ez_parity::fixtures::{self, FixtureCase};
use regex::Regex;
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_fs.jsonl";

fn entry_id() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\d{20}-\d{8}-[0-9a-f]{12}").expect("id válido"))
}

/// `_norm_text`: ids abreviados, perfil a `<tmp>` y barras normales.
fn normalize(value: &Value, home: &str) -> Value {
    match value {
        Value::String(text) => Value::String(
            entry_id()
                .replace_all(text, "<id>")
                .replace(home, "<tmp>")
                .replace('\\', "/"),
        ),
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

fn base_config(home: &Path) -> AgentConfig {
    let (config, _) = AgentConfig::load(Some(&home.join("app").join("permissions.toml")));
    config
}

fn case_config(base: &AgentConfig, workspace: &Path, home: &Path) -> AgentConfig {
    let mut config = base.clone();
    config.general.max_file_mb = 0.001;
    config.paths.read_roots = vec![
        workspace.display().to_string(),
        home.join("docs-root").display().to_string(),
    ];
    config.paths.write_roots = vec![workspace.display().to_string()];
    config.paths.deny = vec![workspace.join("secretos").display().to_string()];
    config.paths.deny_globs = vec!["**/.env".into(), "**/*.pem".into()];
    config.workspace = workspace.to_path_buf();
    config
}

/// Doble de `send2trash`: registra y mueve a `<home>/trash/<label>/NNN-nombre`.
struct Recorder {
    home: PathBuf,
    label: String,
    trashed: Mutex<Vec<String>>,
    counter: Mutex<usize>,
}

impl Recorder {
    fn new(home: &Path, label: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            label: label.to_string(),
            trashed: Mutex::new(Vec::new()),
            counter: Mutex::new(0),
        }
    }

    fn trashed(&self) -> Vec<String> {
        self.trashed.lock().expect("registro").clone()
    }
}

impl Trash for Recorder {
    fn delete(&self, path: &Path) -> Result<(), String> {
        self.trashed
            .lock()
            .expect("registro")
            .push(path.display().to_string());
        let mut counter = self.counter.lock().expect("contador");
        *counter += 1;
        let directory = self.home.join("trash").join(&self.label);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let target = directory.join(format!("{counter:03}-{name}"));
        std::fs::rename(path, target).map_err(|error| error.to_string())
    }
}

/// Reloj del diario: `datetime.now().astimezone().isoformat(timespec="seconds")`.
fn journal_clock() -> Clock {
    Arc::new(|| "2025-01-01T00:00:00+00:00".to_string())
}

fn info_dict(spec: &ToolSpec, args: &Map<String, Value>) -> Value {
    let info = spec.describe(args);
    json!({
        "level": info.level,
        "bulk": info.bulk,
        "always_confirm": info.always_confirm,
        "summary": info.summary,
        "bulk_forced": info.bulk_forced,
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
        "info": info_dict(spec, &Map::new()),
    })
}

fn call(spec: &ToolSpec, args: &Value) -> Value {
    let args = args.as_object().expect("argumentos objeto");
    match (spec.handler)(args) {
        Ok(ToolReturn::Result(result)) => result.to_dict(),
        Ok(ToolReturn::Value(data)) => ez_agent::registry::ToolResult::ok(data).to_dict(),
        Err(message) => ez_agent::registry::ToolResult::failed(message).to_dict(),
    }
}

/// `_tree`: rutas relativas a `work` sin `app/`, ordenadas como `sorted(rglob)`.
fn tree_of(work: &Path) -> Value {
    let mut items: Vec<(String, bool, Option<String>)> = Vec::new();
    fn walk(root: &Path, current: &Path, items: &mut Vec<(String, bool, Option<String>)>) {
        let Ok(listing) = std::fs::read_dir(current) else {
            return;
        };
        for entry in listing.filter_map(Result::ok) {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("ruta relativa")
                .to_string_lossy()
                .replace('\\', "/");
            if relative == "app" || relative.starts_with("app/") {
                continue;
            }
            if path.is_dir() {
                items.push((relative, true, None));
                walk(root, &path, items);
            } else {
                let content = std::fs::read(&path).unwrap_or_default();
                items.push((
                    relative,
                    false,
                    Some(String::from_utf8_lossy(&content).into_owned()),
                ));
            }
        }
    }
    walk(work, work, &mut items);
    // `sorted(rglob)` de Python compara `_parts_normcase`: componente a componente.
    items.sort_by_key(|item| {
        item.0
            .to_lowercase()
            .split('/')
            .map(str::to_string)
            .collect::<Vec<_>>()
    });
    Value::Array(
        items
            .into_iter()
            .map(|(path, is_dir, content)| {
                json!({"path": path, "content": if is_dir { Value::Null } else { json!(content) }})
            })
            .collect(),
    )
}

fn read_journal(directory: &Path) -> (Value, Vec<String>) {
    let Ok(listing) = std::fs::read_dir(directory) else {
        return (Value::Array(Vec::new()), Vec::new());
    };
    let mut files: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut entries = Vec::new();
    let mut names = Vec::new();
    for file in files {
        names.push(
            file.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        );
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        match serde_json::from_str::<Value>(&text) {
            Ok(value) => entries.push(value),
            Err(_) => entries.push(json!({"unreadable": true})),
        }
    }
    (Value::Array(entries), names)
}

fn run_calls(case: &FixtureCase, home: &Path, label: &str) -> Value {
    let work = home.join("fs-cases").join(label);
    let workspace = work.join("workspace");
    write_files(&case.input["files"], &workspace, home);
    let config = case_config(&base_config(home), &workspace, home);
    let recorder = Arc::new(Recorder::new(home, label));
    let trash: TrashHandle = recorder.clone();
    let clock: Clock = Arc::new(|| "20250101-000000".to_string());
    let journal = Arc::new(
        Journal::with_hooks(
            Some(&work.join("app").join("journal")),
            Arc::clone(&trash),
            journal_clock(),
        )
        .expect("diario del caso"),
    );
    let opened = Arc::new(Mutex::new(Vec::new()));
    let opener: Opener = {
        let opened = Arc::clone(&opened);
        Arc::new(move |target: &str| {
            opened.lock().expect("aperturas").push(target.to_string());
            Ok(())
        })
    };
    let tools = build_fs_tools_with(&config, Arc::clone(&journal), opener, trash, clock);
    let by_name: std::collections::HashMap<&str, &ToolSpec> = tools
        .iter()
        .map(|tool| (tool.name.as_str(), tool))
        .collect();
    let mut results = Vec::new();
    let mut infos = Vec::new();
    let home_text = home.display().to_string();
    for call_spec in case.input["calls"].as_array().expect("llamadas") {
        let name = call_spec["tool"].as_str().expect("nombre de herramienta");
        let spec = by_name.get(name).expect("herramienta del registro");
        let args = expand(&call_spec["args"], &home_text);
        let map = args.as_object().expect("argumentos objeto");
        infos.push(info_dict(spec, map));
        results.push(call(spec, &args));
    }
    let (journal_entries, journal_files) = read_journal(journal.directory());
    json!({
        "results": results,
        "infos": infos,
        "tree": tree_of(&work),
        "journal": journal_entries,
        "journal_files": journal_files,
        "trashed": recorder.trashed(),
        "opened": opened.lock().expect("aperturas").clone(),
    })
}

fn count_structure(home: &Path, kind: &str) -> PathBuf {
    let root = home.join("count-cases");
    match kind {
        "limit" => {
            let many = root.join("many");
            std::fs::create_dir_all(&many).expect("carpeta many");
            for index in 0..5001 {
                std::fs::write(many.join(format!("f{index:04}.txt")), "").expect("archivo");
            }
            many
        }
        "tree" => {
            let tree = root.join("tree");
            std::fs::create_dir_all(tree.join("uno")).expect("uno");
            std::fs::create_dir_all(tree.join("dos")).expect("dos");
            for file in ["a.txt", "uno/b.txt", "dos/c.txt"] {
                std::fs::write(tree.join(file), "").expect("archivo");
            }
            tree
        }
        "file" => {
            std::fs::create_dir_all(&root).expect("raíz de conteo");
            let single = root.join("single.txt");
            std::fs::write(&single, "").expect("archivo");
            single
        }
        _ => root.join("no-existe"),
    }
}

#[test]
fn every_tools_fs_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tools_fs legibles");
    let home = std::env::temp_dir().join(format!(
        "ez-tools-fs-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&home).expect("carpeta temporal");
    let raw_home = home
        .canonicalize()
        .expect("ruta canónica")
        .display()
        .to_string();
    let home_text = raw_home.strip_prefix(r"\\?\").unwrap_or(&raw_home);
    let home = PathBuf::from(home_text);

    let mut cases_run = 0;
    for case in &cases {
        let suffix = case.case.rsplit('/').next().unwrap_or_default();
        match case.function.as_str() {
            "_count_items" => {
                let target = count_structure(&home, suffix);
                assert_eq!(
                    json!(count_items(&target)),
                    case.output["count"],
                    "caso {}",
                    case.case
                );
            }
            "_count_phrase" => {
                let count = case.input["count"].as_u64().expect("conteo") as usize;
                assert_eq!(
                    json!(count_phrase(count)),
                    case.output["text"],
                    "caso {}",
                    case.case
                );
            }
            "build_fs_tools" if suffix == "specs" => {
                let config = case_config(&base_config(&home), &home.join("specs"), &home);
                let recorder: TrashHandle = Arc::new(Recorder::new(&home, "specs"));
                let clock: Clock = Arc::new(|| "20250101-000000".to_string());
                let journal = Arc::new(
                    Journal::with_hooks(
                        Some(&home.join("specs").join("journal")),
                        Arc::clone(&recorder),
                        journal_clock(),
                    )
                    .expect("diario"),
                );
                let opener: Opener = Arc::new(|_| Ok(()));
                let tools = build_fs_tools_with(&config, journal, opener, recorder, clock);
                let produced: Vec<Value> = tools.iter().map(spec_dict).collect();
                assert_eq!(
                    normalize(&Value::Array(produced), home_text),
                    case.output["tools"],
                    "caso {}",
                    case.case
                );
            }
            "build_fs_tools" => {
                let produced = run_calls(case, &home, suffix);
                assert_eq!(
                    normalize(&produced, home_text),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "describe" => {
                let workspace = home.join("fs-cases").join("describe").join("workspace");
                std::fs::create_dir_all(&workspace).expect("workspace");
                write_files(&case.input["files"], &workspace, &home);
                let config = case_config(&base_config(&home), &workspace, &home);
                let recorder: TrashHandle = Arc::new(Recorder::new(&home, "describe"));
                let clock: Clock = Arc::new(|| "20250101-000000".to_string());
                let journal = Arc::new(
                    Journal::with_hooks(
                        Some(&home.join("fs-cases").join("describe").join("journal")),
                        Arc::clone(&recorder),
                        journal_clock(),
                    )
                    .expect("diario"),
                );
                let opener: Opener = Arc::new(|_| Ok(()));
                let tools = build_fs_tools_with(&config, journal, opener, recorder, clock);
                let by_name: std::collections::HashMap<&str, &ToolSpec> = tools
                    .iter()
                    .map(|tool| (tool.name.as_str(), tool))
                    .collect();
                let mut produced = Vec::new();
                for entry in case.output["cases"].as_array().expect("casos de describe") {
                    let name = entry["tool"].as_str().expect("herramienta");
                    let args = expand(&entry["args"], home_text);
                    let spec = by_name.get(name).expect("herramienta del registro");
                    let info = info_dict(spec, args.as_object().expect("objeto"));
                    produced.push(json!({"tool": name, "args": entry["args"], "info": info}));
                }
                assert_eq!(
                    normalize(&Value::Array(produced), home_text),
                    case.output["cases"],
                    "caso {}",
                    case.case
                );
            }
            other => panic!("caso {}: función desconocida: {other}", case.case),
        }
        cases_run += 1;
    }
    // `build_fs_tools` también construye las siete herramientas sin llamarlas.
    let config = case_config(&base_config(&home), &home.join("plain"), &home);
    let journal =
        Arc::new(Journal::new(Some(&home.join("plain").join("journal"))).expect("diario"));
    let tools = build_fs_tools(&config, journal, Arc::new(|_| Ok(())));
    assert_eq!(tools.len(), 7);
    assert_eq!(tools[4].name, "delete_path");

    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 120, "muy pocos casos: {cases_in_file}");
}
