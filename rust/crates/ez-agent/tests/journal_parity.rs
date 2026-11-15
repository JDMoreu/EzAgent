//! Paridad de `journal` contra `parity/fixtures/journal.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: los escenarios comparten el perfil temporal y el reloj y
//! la Papelera están inyectados, como en el oráculo.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_agent::journal::{default_journal_dir, Journal, Validate};
use ez_agent::tools_fs::{Clock, Trash, TrashHandle};
use ez_parity::fixtures::{self, FixtureCase};
use regex::Regex;
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/journal.jsonl";
const FROZEN: &str = "2025-01-01T00:00:00+00:00";

fn entry_id() -> Regex {
    Regex::new(r"\d{20}-\d{8}-[0-9a-f]{12}").expect("id")
}

fn normalize(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(normalize_text(text, temp)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| normalize(item, temp)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), normalize(item, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn normalize_text(text: &str, temp: &str) -> String {
    let mut text = entry_id().replace_all(text, "<id>").into_owned();
    text = text.replace("\\\\", "/");
    for prefix in [temp.replace('\\', "/"), temp.to_string()] {
        text = text.replace(&prefix, "<tmp>");
    }
    text.replace('\\', "/")
}

fn work_path(raw: &str, work: &Path, temp: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("<work>/") {
        return work.join(rest);
    }
    if let Some(rest) = raw.strip_prefix("<home>/") {
        return Path::new(temp).join(rest);
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        work.join(path)
    }
}

fn steps(value: &Value, work: &Path, temp: &str) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let item = if ["path", "src", "dst"].contains(&key.as_str()) {
                        match item.as_str() {
                            Some(text) if !text.is_empty() => {
                                Value::String(work_path(text, work, temp).display().to_string())
                            }
                            Some(_) => Value::String(String::new()),
                            None => item.clone(),
                        }
                    } else {
                        steps(item, work, temp)
                    };
                    (key.clone(), item)
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| steps(item, work, temp)).collect())
        }
        other => other.clone(),
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta");
    }
    std::fs::write(path, text.as_bytes()).expect("archivo");
}

fn tree(root: &Path, temp: &str) -> Value {
    fn walk(root: &Path, path: &Path, entries: &mut Vec<(String, PathBuf)>) {
        let Ok(listing) = std::fs::read_dir(path) else {
            return;
        };
        for entry in listing.filter_map(Result::ok) {
            let child = entry.path();
            let relative = child
                .strip_prefix(root)
                .map(|item| item.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if relative
                .split('/')
                .next()
                .is_some_and(|first| first == "journal" || first == "_trash")
            {
                continue;
            }
            entries.push((relative.clone(), child.clone()));
            if child.is_dir() {
                walk(root, &child, entries);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort_by(|left, right| left.0.cmp(&right.0));
    let mut result = Map::new();
    for (relative, path) in found {
        let value = if path.is_dir() {
            "<dir>".to_string()
        } else {
            std::fs::read(&path)
                .map(|bytes| String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
                .unwrap_or_default()
        };
        result.insert(relative, Value::String(normalize_text(&value, temp)));
    }
    Value::Object(result)
}

struct Scenario {
    work: PathBuf,
    directory: PathBuf,
    temp: String,
    journal: Journal,
    hooks: Hooks,
    trashed: Vec<String>,
    validated: Vec<String>,
}

#[derive(Clone)]
struct Hooks {
    clock: Clock,
    trash: TrashHandle,
}

struct ClosureTrash<F: Fn(&Path) -> Result<(), String> + Send + Sync>(F);

impl<F: Fn(&Path) -> Result<(), String> + Send + Sync> Trash for ClosureTrash<F> {
    fn delete(&self, path: &Path) -> Result<(), String> {
        (self.0)(path)
    }
}

fn error(text: impl Into<String>) -> Value {
    json!({"error": format!("ValueError: {}", text.into())})
}

impl Scenario {
    fn run(&mut self, op: &Value) -> Value {
        let action = op["op"].as_str().unwrap_or("");
        let text = |key: &str| op[key].as_str().unwrap_or("").to_string();
        match action {
            "write" => {
                write(
                    &work_path(op["name"].as_str().unwrap_or(""), &self.work, &self.temp),
                    op["text"].as_str().unwrap_or(""),
                );
                json!(true)
            }
            "mkdir" => {
                std::fs::create_dir_all(work_path(
                    op["name"].as_str().unwrap_or(""),
                    &self.work,
                    &self.temp,
                ))
                .expect("carpeta");
                json!(true)
            }
            "rm" => {
                let path = work_path(op["name"].as_str().unwrap_or(""), &self.work, &self.temp);
                if path.is_dir() {
                    let _ = std::fs::remove_dir(&path);
                } else if path.exists() {
                    let _ = std::fs::remove_file(&path);
                }
                json!(true)
            }
            "save_entry" => {
                let escaped = serde_json::to_string(&self.work.display().to_string())
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string();
                write(
                    &self.directory.join(op["name"].as_str().unwrap_or("")),
                    &op["text"]
                        .as_str()
                        .unwrap_or("")
                        .replace("<work>", &escaped),
                );
                json!(true)
            }
            "reopen" => {
                self.journal = Journal::with_hooks(
                    Some(&self.directory),
                    Arc::clone(&self.hooks.trash),
                    Arc::clone(&self.hooks.clock),
                )
                .expect("reabrir");
                json!(self.directory.display().to_string())
            }
            "record" => match self.journal.record(
                &text("kind"),
                &text("summary"),
                steps(&op["steps"].clone(), &self.work, &self.temp)
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            ) {
                Ok(id) => json!(id),
                Err(message) => error(message),
            },
            "record_create" => {
                let path = work_path(&text("path"), &self.work, &self.temp);
                match self
                    .journal
                    .record_create(&path, &text("kind"), &text("summary"))
                {
                    Ok(id) => json!(id),
                    Err(message) => error(message),
                }
            }
            "record_copy" => {
                let src = work_path(&text("src"), &self.work, &self.temp);
                let dst = work_path(&text("dst"), &self.work, &self.temp);
                let backup = op["backup"]
                    .as_str()
                    .map(|raw| work_path(raw, &self.work, &self.temp));
                match self.journal.record_copy(
                    &src,
                    &dst,
                    op["overwrite"] == json!(true),
                    backup.as_deref(),
                ) {
                    Ok(id) => json!(id),
                    Err(message) => error(message),
                }
            }
            "record_move" => {
                let src = work_path(&text("src"), &self.work, &self.temp);
                let dst = work_path(&text("dst"), &self.work, &self.temp);
                let backup = op["backup"]
                    .as_str()
                    .map(|raw| work_path(raw, &self.work, &self.temp));
                match self.journal.record_move(
                    &src,
                    &dst,
                    op["overwrite"] == json!(true),
                    backup.as_deref(),
                ) {
                    Ok(id) => json!(id),
                    Err(message) => error(message),
                }
            }
            "record_overwrite" => {
                let target = work_path(&text("target"), &self.work, &self.temp);
                let backup = work_path(&text("backup"), &self.work, &self.temp);
                match self
                    .journal
                    .record_overwrite(&target, &backup, &text("summary"))
                {
                    Ok(id) => json!(id),
                    Err(message) => error(message),
                }
            }
            "undo" => self.undo(op),
            "load" => Value::Array(self.journal.load_entries_value()),
            "write_entry" => match self.journal.write_entry_value(&op["entry"]) {
                Ok(()) => json!(true),
                Err(message) => error(message),
            },
            other => error(format!("operación desconocida: {other}")),
        }
    }

    fn undo(&mut self, op: &Value) -> Value {
        let n = match op.get("n") {
            None => 1,
            Some(Value::Bool(_)) | Some(Value::String(_)) | Some(Value::Null) => {
                return error("n debe ser un número entero.")
            }
            Some(Value::Number(number)) => match number.as_i64() {
                Some(value) => value,
                None => return error("n debe ser un número entero."),
            },
            Some(_) => return error("n debe ser un número entero."),
        };
        if n < 1 {
            return error("n debe ser mayor o igual que 1.");
        }
        let mode = op["validate"].as_str().unwrap_or("none").to_string();
        let validated = Arc::new(Mutex::new(Vec::new()));
        let check = |path: &Path| -> Result<PathBuf, String> {
            validated
                .lock()
                .expect("validados")
                .push(normalize_text(&path.display().to_string(), &self.temp));
            match mode.as_str() {
                "reject" => Err("fuera de la raíz permitida".to_string()),
                "suffix" => Ok(path.with_file_name(format!(
                    "{}.ok",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ))),
                _ => Ok(path.to_path_buf()),
            }
        };
        let validate: Option<Validate<'_>> = if mode == "none" { None } else { Some(&check) };
        match self.journal.undo_last(n, validate) {
            Ok(outcomes) => {
                self.validated
                    .extend(validated.lock().expect("validados").iter().cloned());
                Value::Array(outcomes.iter().map(|outcome| outcome.to_dict()).collect())
            }
            Err(message) => error(message),
        }
    }
}

fn run_scenario(case: &FixtureCase, temp: &str) -> Value {
    let label = case.case.trim_start_matches("journal/");
    let work = Path::new(temp).join("journal-cases").join(label);
    std::fs::create_dir_all(&work).expect("caso");
    let directory = work.join("journal");
    let trashed = Arc::new(Mutex::new(Vec::<String>::new()));
    let trash_work = work.clone();
    let trash_temp = temp.to_string();
    let sink = Arc::clone(&trashed);
    let hooks = Hooks {
        clock: Arc::new(|| FROZEN.to_string()),
        trash: Arc::new(ClosureTrash(move |path: &Path| {
            sink.lock()
                .expect("papelera")
                .push(normalize_text(&path.display().to_string(), &trash_temp));
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let index = sink.lock().expect("papelera").len();
            let target = trash_work.join("_trash").join(format!("{index:03}-{name}"));
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::rename(path, target).map_err(|error| error.to_string())
        })),
    };
    let journal = Journal::with_hooks(
        Some(&directory),
        Arc::clone(&hooks.trash),
        Arc::clone(&hooks.clock),
    )
    .expect("diario");
    let mut scenario = Scenario {
        work: work.clone(),
        directory: directory.clone(),
        temp: temp.to_string(),
        journal,
        hooks,
        trashed: Vec::new(),
        validated: Vec::new(),
    };
    let mut results = Vec::new();
    for op in case.input["ops"].as_array().cloned().unwrap_or_default() {
        results.push(scenario.run(&op));
    }
    scenario.trashed = Arc::clone(&trashed).lock().expect("papelera").clone();
    let mut files = Vec::new();
    let mut listed: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default();
    listed.sort();
    for path in listed {
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read(&path)
            .map(|bytes| String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
            .unwrap_or_default();
        files.push(
            json!({"name": normalize_text(&name, temp), "text": normalize_text(&text, temp)}),
        );
    }
    normalize(
        &json!({
            "results": results,
            "validated": scenario.validated,
            "trashed": scenario.trashed,
            "entries": scenario.journal.load_entries_value(),
            "entry_files": files,
            "tree": tree(&work, temp),
            "directory": directory.display().to_string(),
            "directory_exists": directory.is_dir(),
        }),
        temp,
    )
}

fn env_case(case: &FixtureCase, temp: &str) -> Value {
    let saved: Vec<(String, Option<String>)> =
        ["APPDATA", "XDG_CONFIG_HOME", "USERPROFILE", "HOME"]
            .iter()
            .map(|name| ((*name).to_string(), std::env::var(name).ok()))
            .collect();
    std::env::set_var("USERPROFILE", temp);
    std::env::set_var("HOME", temp);
    for name in ["APPDATA", "XDG_CONFIG_HOME"] {
        match case.input.get(name) {
            None | Some(Value::Null) => std::env::remove_var(name),
            Some(value) => std::env::set_var(name, value.as_str().unwrap_or("")),
        }
    }
    let path = default_journal_dir();
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    normalize(&json!({"path": path.display().to_string()}), temp)
}

#[test]
fn every_journal_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de journal");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 37, "faltan casos: {cases_in_file}");
    let temp = std::env::temp_dir().join(format!("ez-journal-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("temporal");
    let temp_text = temp.display().to_string();
    let mut run = 0;
    for case in &cases {
        let value = match case.function.as_str() {
            "Journal" => run_scenario(case, &temp_text),
            "default_journal_dir" => env_case(case, &temp_text),
            other => panic!("función sin cubrir: {other}"),
        };
        let expected = normalize(&case.output, &temp_text);
        assert_eq!(value, expected, "caso {}", case.case);
        run += 1;
    }
    assert_eq!(run, cases_in_file, "se saltó algún caso");
    let _ = std::fs::remove_dir_all(&temp);
}
