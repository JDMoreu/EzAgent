//! Paridad de `agent/audit.py` contra `parity/fixtures/audit.jsonl` (P-AGENT-A).
//!
//! El oráculo congela el reloj en `2025-01-01T00:00:00.000+00:00`; aquí se usa
//! `AuditLog::log_at` y un directorio temporal propio por ejecución.

use std::path::Path;

use ez_agent::audit::{AuditEntry, AuditLog, CONTENT_KEYS, SECRET_KEY_PARTS};
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::truthy;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/audit.jsonl";
const FROZEN_TS: &str = "2025-01-01T00:00:00.000+00:00";
const TRAPS: [&str; 6] = [
    "SUPERSECRETO",
    "SUPERSECRETO2",
    "SUPERSECRETO3",
    "hunter2",
    "abc.DEF-123",
    "sk-proj",
];

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

/// `_expand` del oráculo: `{"$bytes": "..."}` se convierte en el texto.
fn expand(value: &Value) -> Value {
    match value {
        Value::Object(map) if map.len() == 1 && map.contains_key("$bytes") => {
            Value::String(map["$bytes"].as_str().unwrap_or_default().to_string())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), expand(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(expand).collect()),
        other => other.clone(),
    }
}

fn integer(value: &Value) -> i64 {
    match value {
        Value::Number(number) => number
            .as_i64()
            .unwrap_or_else(|| number.as_f64().unwrap_or(0.0).trunc() as i64),
        _ => 0,
    }
}

fn text_or<'a>(input: &'a Value, name: &str, default: &'a str) -> &'a str {
    input.get(name).and_then(Value::as_str).unwrap_or(default)
}

fn leaks(text: &str) -> Vec<Value> {
    TRAPS
        .iter()
        .filter(|needle| text.contains(*needle))
        .map(|needle| json!(needle))
        .collect()
}

fn redact_case(input: &Value, log: &AuditLog) -> Value {
    let args = expand(field(input, "args"));
    let redacted = log.redact(&args);
    let options = DumpOptions::utf8();
    let shown = dumps_with(&redacted, &options);
    json!({"redacted": redacted, "leaks": leaks(&shown)})
}

fn write_log(log: &AuditLog, input: &Value) {
    let args = expand(&input.get("args").cloned().unwrap_or_else(|| json!({})));
    let entry = AuditEntry {
        turn_id: text_or(input, "turn_id", "t1"),
        tool: text_or(input, "tool", "write_file"),
        level: integer(field(input, "level")),
        args: &args,
        decision: text_or(input, "decision", "allow"),
        taint: truthy(field(input, "taint")),
        result_ok: input.get("result_ok").is_none_or(truthy),
        ms: field(input, "ms").as_f64().unwrap_or(1.0),
        error_kind: input.get("error_kind").and_then(Value::as_str),
    };
    log.log_at(&entry, FROZEN_TS);
}

fn log_case(input: &Value, path: &Path) -> Value {
    let log = AuditLog::new(path);
    write_log(&log, input);
    let body = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = body.lines().collect();
    json!({
        "line": lines.last().copied().unwrap_or_default(),
        "lines": lines.len(),
        "created_dirs": path.parent().is_some_and(Path::is_dir),
        "leaks": leaks(&body),
    })
}

fn file_lines(folder: &Path) -> Map<String, Value> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut files = Map::new();
    for name in names {
        let body = std::fs::read_to_string(folder.join(&name)).unwrap_or_default();
        files.insert(name, json!(body.lines().count()));
    }
    files
}

fn rotate_case(input: &Value, root: &Path) -> Value {
    let backups = field(input, "backups").as_u64().unwrap_or(0) as usize;
    let folder = root.join(if backups == 0 { "no-rot" } else { "rot" });
    let log = AuditLog::with_limits(
        &folder.join("audit.jsonl"),
        field(input, "max_bytes").as_u64().unwrap_or(0),
        backups,
    );
    let entries = field(input, "entries").as_u64().unwrap_or(0);
    let args = input.get("args").cloned().unwrap_or_else(|| json!({}));
    for index in 0..entries {
        write_log(
            &log,
            &json!({"turn_id": format!("turno-{index:03}"), "args": args}),
        );
    }
    let files = file_lines(&folder);
    if backups == 0 {
        return json!({"files": files});
    }
    let live = std::fs::read_to_string(folder.join("audit.jsonl")).unwrap_or_default();
    let last = live
        .lines()
        .last()
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap_or(Value::Null);
    let everything = files
        .keys()
        .map(|name| std::fs::read_to_string(folder.join(name)).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    json!({
        "files": files,
        "last_turn": last.get("turn_id").cloned().unwrap_or(Value::Null),
        "has_oldest": everything.contains("turno-000"),
        "has_newest": everything.contains("turno-039"),
    })
}

fn constants_case() -> Value {
    let mut content: Vec<&str> = CONTENT_KEYS.to_vec();
    content.sort_unstable();
    json!({
        "content_keys": content,
        "secret_key_parts": SECRET_KEY_PARTS,
    })
}

fn run_case(case: &FixtureCase, root: &Path) -> Result<Value, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "module_constants" => constants_case(),
        "redact" => {
            let log = AuditLog::new(&root.join("redact.jsonl"));
            redact_case(input, &log)
        }
        "log" if input.get("entries").is_some() => rotate_case(input, root),
        "log" => log_case(
            input,
            &root
                .join("logs")
                .join(format!("{}.jsonl", case.case.replace('/', "-"))),
        ),
        other => return Err(format!("función sin portar: {other}")),
    })
}

#[test]
fn every_audit_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de audit legibles");
    let root = std::env::temp_dir().join(format!(
        "ez-audit-parity-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&root).expect("carpeta temporal");
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case, &root);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &case.output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
        cases_run += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 20, "faltan casos en el fixture");
}
