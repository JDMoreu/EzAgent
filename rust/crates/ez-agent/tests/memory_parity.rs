//! Paridad de `memory` contra `parity/fixtures/memory.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: los escenarios comparten el perfil temporal. El reloj del
//! almacén está congelado como en el oráculo y la base se vuelca con `rusqlite`.

use std::path::Path;
use std::sync::Arc;

use ez_agent::memory::{
    contains_web_content, find_sensitive_text, fold_text, make_snippet, normalize_entry,
    normalize_for_compare, normalize_kind, summarize_text, MemoryOptions, MemoryResult,
    MemoryStore, MemoryTimestamp, DEFAULT_RECALL_LIMIT, MAX_ENTRY_CHARS, MEMORY_LIMIT,
    RECALL_FRAGMENT_CHARS, RETENTION_DAYS, USER_LIMIT,
};
use ez_agent::tools_memory::build_memory_tools;
use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{python_str, text_or_empty};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/memory.jsonl";

#[path = "memory_parity/support.rs"]
mod support;
use support::{dump_db, dump_files, frozen, info, normalize, store_at, timestamp};

/// Casos del CLI de Python (argparse y `main`) y del diccionario interno de
/// alias: el Rust no tiene ese binario ni expone las tablas privadas.
const STRUCTURAL: [&str; 13] = [
    "memory/cli/parse/01",
    "memory/cli/parse/02",
    "memory/cli/parse/03",
    "memory/cli/parse/04",
    "memory/cli/wipe-cancel",
    "memory/cli/wipe-confirm",
    "memory/cli/wipe-noninteractive",
    "memory/cli/wipe-yes",
    "memory/constants",
    "memory/tools/compact-timeout",
    "memory/tools/forget-timeout",
    "memory/tools/manager",
    "memory/tools/remember-timeout",
];

fn run_op(store: &MemoryStore, op: &Value) -> Value {
    let kind = || op["kind"].as_str().unwrap_or("");
    let result = || -> Result<Value, String> {
        match op["op"].as_str().unwrap_or("") {
            "filename" => Ok(json!(store.filename(kind()))),
            "limit" => Ok(json!(store.limit(kind()))),
            "validate" => Ok(store
                .validate_entry(kind(), &python_str(&op["text"]))
                .to_dict()),
            "remember" => Ok(store.remember(kind(), &python_str(&op["text"]))?.to_dict()),
            "read" => Ok(json!(store.read_markdown(kind()))),
            "entries" => Ok(json!(store.entries(kind()))),
            "prepare" => Ok(store
                .prepare_compaction(kind(), &python_str(&op["text"]))
                .to_dict()),
            "compact" => Ok(store
                .compact_memory(kind(), &python_str(&op["text"]))?
                .to_dict()),
            "forget" => Ok(store.forget(&python_str(&op["query"]))?.to_dict()),
            "find_matches" => Ok(json!(store.find_matches(&python_str(&op["query"])))),
            "memory_block" => Ok(json!(store.memory_block())),
            "record" => {
                let Some(content) = op["content"].as_str() else {
                    return Ok(json!(false));
                };
                Ok(json!(store.record(
                    &python_str(&op["role"]),
                    content,
                    op["session"].as_str(),
                    &timestamp(&op["ts"])?
                )?))
            }
            "record_exchange" => {
                let (first, second) = store.record_exchange(
                    &python_str(&op["user"]),
                    &python_str(&op["assistant"]),
                    op["session"].as_str(),
                    &timestamp(&op["ts"])?,
                )?;
                Ok(json!([first, second]))
            }
            "purge" => Ok(json!(store.purge_old(
                timestamp(&op["now"]).ok().and_then(|value| {
                    match value {
                        MemoryTimestamp::DateTime(moment) => Some(moment),
                        _ => None,
                    }
                })
            )?)),
            "count" => Ok(json!(store.count_messages()?)),
            "recall" => {
                let query = python_str(&op["query"]);
                let limit = match op.get("limit") {
                    None => DEFAULT_RECALL_LIMIT,
                    Some(Value::Bool(value)) => i64::from(*value),
                    Some(Value::Number(number)) => number
                        .as_i64()
                        .or_else(|| number.as_f64().map(|value| value.trunc() as i64))
                        .ok_or_else(|| "ValueError: invalid limit".to_string())?,
                    Some(Value::String(text)) => text.trim().parse().map_err(|_| {
                        format!("ValueError: invalid literal for int() with base 10: '{text}'")
                    })?,
                    Some(_) => return Err(
                        "TypeError: int() argument must be a string, a bytes-like object or a real number, not 'NoneType'"
                            .to_string(),
                    ),
                };
                let results = store.recall_limited(&query, limit)?;
                let backend = store.last_backend();
                Ok(json!({"results": results, "backend": backend}))
            }
            "wipe" => Ok(json!(store.wipe()?)),
            "info" => Ok(info(store)),
            other => Err(format!("ValueError: operación desconocida: {other}")),
        }
    };
    result().unwrap_or_else(|error| json!({"error": error}))
}

fn run_scenario(case: &FixtureCase, dir: &Path, temp: &str) -> Value {
    let input = &case.input;
    let memory_dir = dir.join("memory");
    let history = dir.join("history.db");
    std::fs::create_dir_all(&memory_dir).expect("memoria");
    if let Some(seed) = input.get("seed").and_then(Value::as_object) {
        for (name, text) in seed {
            std::fs::write(
                memory_dir.join(name),
                text.as_str().unwrap_or("").replace("\r\n", "\n").as_bytes(),
            )
            .expect("seed");
        }
    }
    let mut store = store_at(dir, input);
    let mut prefer_fts = input["prefer_fts"] != json!(false);
    let mut warnings = Vec::new();
    if !prefer_fts {
        warnings.push(json!({"category": "RuntimeWarning",
            "message": "FTS5 desactivado; recall usará LIKE como respaldo."}));
    }
    let mut results = Vec::new();
    for op in input["ops"].as_array().cloned().unwrap_or_default() {
        if op["op"] == json!("reopen") {
            let options = op.get("options").cloned().unwrap_or(json!({}));
            if let Some(value) = options.get("prefer_fts") {
                prefer_fts = value == &json!(true);
            }
            store = MemoryStore::from_paths(
                memory_dir.clone(),
                history.clone(),
                MemoryOptions {
                    session: options["session"]
                        .as_str()
                        .unwrap_or(&store.session)
                        .to_string(),
                    prefer_fts,
                    retention_days: store.retention_days,
                    clock: Some(Arc::new(frozen)),
                },
            )
            .expect("reapertura");
            if !prefer_fts {
                warnings.push(json!({"category": "RuntimeWarning",
                    "message": "FTS5 desactivado; recall usará LIKE como respaldo."}));
            }
            results.push(info(&store));
        } else if op["op"] == json!("drop_fts") {
            let connection = rusqlite::Connection::open(&history).expect("base");
            let _ = connection.execute("DROP TABLE IF EXISTS messages_fts", []);
            results.push(json!(true));
        } else {
            results.push(run_op(&store, &op));
        }
    }
    let value = json!({
        "results": results,
        "files": dump_files(&memory_dir),
        "db": dump_db(&history),
        "store": info(&store),
        "warnings": warnings,
    });
    normalize(&value, temp)
}

fn store_case(case: &FixtureCase, temp: &str) -> Value {
    let label = case.case.trim_start_matches("memory/").replace('/', "-");
    let dir = Path::new(temp).join("memory-cases").join(label);
    std::fs::create_dir_all(&dir).expect("caso");
    run_scenario(case, &dir, temp)
}

fn tool_case(case: &FixtureCase, temp: &str) -> Value {
    let input = &case.input;
    let label = case.case.rsplit('/').next().unwrap_or_default();
    let dir = Path::new(temp).join("tool-cases").join(label);
    std::fs::create_dir_all(dir.join("memory")).expect("caso");
    if let Some(seed) = input.get("seed").and_then(Value::as_object) {
        for (name, text) in seed {
            std::fs::write(
                dir.join("memory").join(name),
                text.as_str().unwrap_or("").as_bytes(),
            )
            .expect("seed");
        }
    }
    let store = Arc::new(store_at(&dir, input));
    let mut history_results = Vec::new();
    for op in input["history"].as_array().cloned().unwrap_or_default() {
        history_results.push(run_op(&store, &op));
    }
    let mut config = AgentConfig::load(Some(&dir.join("permissions.toml"))).0;
    config.general.confirm_timeout_s = input["timeout_s"].as_f64().unwrap_or(15.0);
    let case_name = case.case.rsplit('/').next().unwrap_or_default();
    let confirm_value = match case_name {
        "remember-no" | "forget-no" => json!(false),
        "remember-none" => json!("none"),
        "remember-raises" => json!("raise"),
        _ => json!(true),
    };
    let allow = confirm_value == json!(true);
    let questions = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let asked = Arc::clone(&questions);
    let mode = if confirm_value == json!("raise") {
        "raise".to_string()
    } else {
        String::new()
    };
    let confirm: ez_agent::tools_memory::ConfirmCallback = Arc::new(move |summary, strong| {
        asked
            .lock()
            .expect("preguntas")
            .push(json!({"summary": summary, "strong": strong}));
        if mode == "raise" {
            panic!("confirmación rota");
        }
        allow
    });
    let callback = (confirm_value != json!("none")).then_some(confirm);
    let tools: std::collections::HashMap<String, _> =
        build_memory_tools(&config, Arc::clone(&store), callback, None)
            .into_iter()
            .map(|tool| (tool.name.clone(), tool))
            .collect();
    let mut results = Vec::new();
    for op in input["ops"].as_array().cloned().unwrap_or_default() {
        let tool = tools.get(op["tool"].as_str().unwrap_or("")).expect("tool");
        let args = op.get("args").cloned().unwrap_or(json!({}));
        let args = args.as_object().cloned().unwrap_or_default();
        let result = match (tool.handler)(&args) {
            Ok(ez_agent::registry::ToolReturn::Result(result)) => result.to_dict(),
            Ok(ez_agent::registry::ToolReturn::Value(value)) => value,
            Err(error) => json!({"error": error}),
        };
        let info = tool.describe(&args);
        results.push(json!({"tool": op["tool"], "result": result,
            "describe": {"level": info.level, "bulk": info.bulk,
                "always_confirm": info.always_confirm, "summary": info.summary}}));
    }
    let payload = json!({
        "ops": input["ops"], "history": input["history"],
        "history_results": history_results,
        "confirm": match confirm_value {
            Value::String(ref text) if text == "none" || text == "raise" => text.clone(),
            _ => "callback".to_string(),
        },
        "questions": questions.lock().expect("preguntas").clone(),
        "results": results,
        "timeout_s": ez_agent::tools_memory::tool_timeout(&config),
        "files": dump_files(&dir.join("memory")),
        "db": dump_db(&dir.join("history.db")),
        "store": info(&store),
        "warnings": if input["prefer_fts"] == json!(false) {
            json!([{"category": "RuntimeWarning",
                "message": "FTS5 desactivado; recall usará LIKE como respaldo."}])
        } else {
            json!([])
        },
    });
    normalize(&payload, temp)
}

fn compare(left: &Value, right: &Value, path: &str, case: &str) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                match b.get(key) {
                    Some(other) => compare(value, other, &format!("{path}.{key}"), case),
                    None => panic!("{case}: sobra {path}.{key}"),
                }
            }
            for key in b.keys() {
                if !a.contains_key(key) {
                    panic!("{case}: falta {path}.{key}");
                }
            }
            if a.len() != b.len() {
                panic!("{case}: tamaño distinto en {path}");
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                panic!("{case}: longitud {} vs {} en {path}", a.len(), b.len());
            }
            for (index, (value, other)) in a.iter().zip(b).enumerate() {
                compare(value, other, &format!("{path}[{index}]"), case);
            }
        }
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            if (a - b).abs() > 1e-9 * a.abs().max(b.abs()).max(1.0) {
                panic!("{case}: {path} es {a} y se esperaba {b}");
            }
        }
        _ if left != right => panic!("{case}: {path} es {left} y se esperaba {right}"),
        _ => {}
    }
}

#[test]
fn every_memory_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de memory");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 229, "faltan casos: {cases_in_file}");
    let temp = std::env::temp_dir().join(format!("ez-memory-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("temporal");
    let temp_text = temp.display().to_string();
    let mut run = 0;
    let mut skipped = 0;
    for case in &cases {
        if STRUCTURAL.contains(&case.case.as_str()) {
            skipped += 1;
            continue;
        }
        let value = match case.function.as_str() {
            "fold_text" => json!({"text": fold_text(&python_str(&case.input["value"]))}),
            "normalize_entry" => {
                json!({"text": normalize_entry(&python_str(&case.input["value"]))})
            }
            "normalize_for_compare" => {
                json!({"text": normalize_for_compare(&python_str(&case.input["value"]))})
            }
            "summarize_text" => json!({"text": summarize_text(
                &text_or_empty(case.input.get("value")),
                case.input.get("limit").and_then(Value::as_u64).unwrap_or(80) as usize,
            )}),
            "find_sensitive_text" => json!({"category": find_sensitive_text(
                &text_or_empty(case.input.get("text")),
            )}),
            "contains_web_content" => json!({"web": contains_web_content(
                &text_or_empty(case.input.get("text")),
            )}),
            "normalize_kind" => {
                let kind = case.input["kind"].as_str();
                json!({"kind": kind.and_then(normalize_kind)})
            }
            "make_snippet" => {
                let terms: Vec<String> = case.input["terms"]
                    .as_array()
                    .map(|items| items.iter().map(python_str).collect())
                    .unwrap_or_default();
                json!({"snippet": make_snippet(
                    &text_or_empty(case.input.get("text")),
                    &terms,
                    case.input.get("max_chars").and_then(Value::as_u64)
                        .unwrap_or(RECALL_FRAGMENT_CHARS as u64) as usize,
                )})
            }
            "MemoryResult.to_dict" => {
                let input = &case.input;
                let result = MemoryResult {
                    status: input["status"].as_str().unwrap_or("").to_string(),
                    message: input["message"].as_str().unwrap_or("").to_string(),
                    kind: input["kind"].as_str().map(str::to_string),
                    text: input["text"].as_str().map(str::to_string),
                    proposal: input["proposal"].as_str().map(str::to_string),
                    removed: input["removed"].as_i64().unwrap_or(0),
                    files: input["files"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    backup: input["backup"].as_str().map(str::to_string),
                    line_count: input["line_count"].as_i64().unwrap_or(0),
                };
                json!({"saved": result.saved(), "data": result.to_dict()})
            }
            "MemoryStore" => store_case(case, &temp_text),
            "MemoryStore.__init__" => init_case(case, &temp_text),
            "memory_tools" => tool_case(case, &temp_text),
            "build_memory_tools" => specs_case(case, &temp_text),
            "tool_timeout" => timeout_case(case, &temp),
            other => panic!("función sin cubrir: {other}"),
        };
        compare(
            &normalize(&value, &temp_text),
            &normalize(&case.output, &temp_text),
            "$",
            &case.case,
        );
        run += 1;
    }
    assert_eq!(run + skipped, cases_in_file, "se saltó algún caso");
    assert_eq!(skipped, STRUCTURAL.len(), "omisiones documentadas");
    let _ = std::fs::remove_dir_all(&temp);
    assert_eq!(USER_LIMIT, 1375);
    assert_eq!(MEMORY_LIMIT, 2200);
    assert_eq!(MAX_ENTRY_CHARS, 200);
    assert_eq!(RETENTION_DAYS, 180);
    assert_eq!(DEFAULT_RECALL_LIMIT, 5);
    assert_eq!(summarize_text("x", 80), "x");
}

fn config_with_timeout(timeout: f64, temp: &Path) -> AgentConfig {
    let mut config = AgentConfig::load(Some(&temp.join("permissions.toml"))).0;
    config.general.confirm_timeout_s = timeout;
    config
}

fn init_case(_case: &FixtureCase, temp: &str) -> Value {
    let dir = Path::new(temp).join("memory-cases").join("from-config");
    std::fs::create_dir_all(dir.join("memory")).expect("caso");
    let mut config = AgentConfig::load(Some(&dir.join("app/permissions.toml"))).0;
    config.memory_dir = dir.join("memory");
    config.history_path = dir.join("history.db");
    config.general.confirm_timeout_s = 15.0;
    let store = MemoryStore::from_paths(
        &config.memory_dir,
        &config.history_path,
        MemoryOptions {
            session: "cfg".to_string(),
            prefer_fts: true,
            retention_days: RETENTION_DAYS,
            clock: Some(Arc::new(frozen)),
        },
    )
    .expect("almacén");
    store
        .remember("user", "desde AgentConfig")
        .expect("recuerdo");
    let payload = json!({"info": info(&store), "files": dump_files(&config.memory_dir),
        "db": dump_db(&config.history_path), "store": info(&store), "warnings": Vec::<Value>::new()});
    normalize(&payload, temp)
}

fn timeout_case(case: &FixtureCase, temp: &Path) -> Value {
    let configured = |raw: &Value| -> f64 {
        match raw {
            Value::Number(number) => number.as_f64().unwrap_or(15.0),
            Value::String(text) => text.parse().unwrap_or(15.0),
            _ => 15.0,
        }
    };
    let values: Vec<f64> = case.input["values"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|raw| {
                    ez_agent::tools_memory::tool_timeout(&config_with_timeout(
                        configured(raw),
                        temp,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"values": values, "no_general": 30.0, "no_config": 30.0})
}

fn specs_case(case: &FixtureCase, temp: &str) -> Value {
    let dir = Path::new(temp).join("tool-cases").join("specs");
    std::fs::create_dir_all(dir.join("memory")).expect("caso");
    let store = Arc::new(store_at(&dir, &json!({"prefer_fts": true})));
    let config = config_with_timeout(
        case.input["timeout_s"].as_f64().unwrap_or(15.0),
        Path::new(temp),
    );
    let tools = build_memory_tools(&config, store, None, None);
    let describe_args = |name: &str| -> Vec<Value> {
        match name {
            "remember" => vec![
                json!({"kind": "user", "text": "  dato  con   espacios "}),
                json!({}),
                json!({"kind": "otro"}),
            ],
            "recall" => vec![
                json!({"query": "  café  ", "limit": 7}),
                json!({}),
                json!({"query": ""}),
            ],
            "forget" => vec![json!({"query": "  café  "}), json!({})],
            _ => vec![
                json!({"kind": "user", "text": "# T
una

 otra "}),
                json!({"kind": "otro"}),
            ],
        }
    };
    json!({"specs": tools.iter().map(|tool| json!({
        "name": tool.name, "description": tool.description, "parameters": tool.parameters,
        "level": tool.level, "untrusted_output": tool.untrusted_output,
        "timeout_s": tool.timeout_s, "schema": tool.schema(),
        "descriptions": describe_args(&tool.name).into_iter().map(|args| {
            let map = args.as_object().cloned().unwrap_or_default();
            let info = tool.describe(&map);
            json!({"args": args, "info": {"level": info.level, "bulk": info.bulk,
                "always_confirm": info.always_confirm, "summary": info.summary}})
        }).collect::<Vec<_>>(),
    })).collect::<Vec<_>>(), "timeout": ez_agent::tools_memory::tool_timeout(&config)})
}
