//! Paridad de `tools_swarm` contra `parity/fixtures/tools_swarm.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: cada caso monta un tablero SQLite temporal con el reloj
//! fijo del oráculo y vuelca las tablas normalizadas (ids, tokens y rutas).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ez_agent::registry::{ToolInfo, ToolResult};
use ez_agent::tools_swarm::build_swarm_tools;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_parity::fixtures::{self, FixtureCase};
use ez_swarm::db::Db;
use ez_swarm::server::SwarmServer;
use ez_swarm::service::SwarmService;
use ez_swarm::stack::SwarmStack;
use ez_swarm::supervisor::{LaunchRequest, Supervisor, SupervisorConfig, SwarmHost};
use rusqlite::types::ValueRef;
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_swarm.jsonl";
const COMMAND_DIR: &str = "C:/ezagent/bin";
const TABLES: [&str; 7] = [
    "cursor", "incident", "lease", "member", "message", "swarm", "task",
];

struct FakeHost {
    context: String,
    saved: Mutex<Vec<String>>,
}

impl FakeHost {
    fn new(context: &str) -> Self {
        Self {
            context: context.to_string(),
            saved: Mutex::new(Vec::new()),
        }
    }
}

impl SwarmHost for FakeHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(Vec::new())
    }

    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, String> {
        Ok(Map::new())
    }

    fn read_agent(&self, _address: &str, _lines: i64) -> Result<String, String> {
        Ok(String::new())
    }

    fn send_task(&self, _address: &str, _line: &str) -> Result<Map<String, Value>, String> {
        Ok(Map::new())
    }

    fn run_agent(
        &self,
        _agent: &str,
        _task: &str,
        _request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        Ok(Map::new())
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        let key = name.replace(' ', "").to_lowercase();
        ["claude", "opencode", "codex"]
            .contains(&key.as_str())
            .then_some(key)
    }

    fn swarm_args(&self, _agent: &str) -> Vec<String> {
        Vec::new()
    }

    fn save_context(&self, text: &str) -> Option<String> {
        self.saved.lock().expect("contextos").push(text.to_string());
        Some(self.context.clone())
    }

    fn default_cwd(&self) -> String {
        "C:/proj".to_string()
    }

    fn agent_names(&self) -> Vec<String> {
        vec!["claude".into(), "opencode".into(), "codex".into()]
    }
}

struct Clock(AtomicU64);

impl Clock {
    fn tick(&self) -> f64 {
        self.0.fetch_add(1, Ordering::SeqCst) as f64
    }
}

struct Rig {
    stack: Arc<SwarmStack>,
    host: Arc<FakeHost>,
    base: PathBuf,
}

fn rig(home: &Path, label: &str, command_dir: &str, context: &str) -> Rig {
    let base = home.join("swarm-tools-cases").join(label);
    std::fs::create_dir_all(&base).expect("caso");
    let clock = Arc::new(Clock(AtomicU64::new(0)));
    let store = Db::new(base.join("board.db"), {
        let clock = Arc::clone(&clock);
        Arc::new(move || clock.tick())
    })
    .expect("tablero");
    let service = Arc::new(SwarmService::new(store, "ezs", {
        let clock = Arc::clone(&clock);
        Arc::new(move || clock.tick())
    }));
    let announce: ez_swarm::supervisor::Announce = Arc::new(|_: &str| {});
    let mut config = SupervisorConfig::new(base.join("seats"), announce);
    config.command_dir = command_dir.to_string();
    config.clock = {
        let clock = Arc::clone(&clock);
        Arc::new(move || clock.tick())
    };
    config.wall = {
        let clock = Arc::clone(&clock);
        Arc::new(move || clock.tick())
    };
    let host = Arc::new(FakeHost::new(context));
    let supervisor = Arc::new(Supervisor::new(Arc::clone(&service), host.clone(), config));
    let server = SwarmServer::new(Arc::clone(&service), base.join("endpoint.json"));
    let stack = Arc::new(SwarmStack {
        base_dir: base.clone(),
        host: host.clone(),
        service,
        server,
        command_dir: command_dir.to_string(),
        supervisor,
        supervisor_thread: true,
    });
    Rig { stack, host, base }
}

fn seed(store: &Db, specs: &Value) -> Vec<String> {
    let mut created = Vec::new();
    let Some(specs) = specs.as_array() else {
        return created;
    };
    for spec in specs {
        let swarm = store
            .create(
                spec["goal"].as_str().unwrap_or("Objetivo de prueba"),
                spec["cwd"].as_str().unwrap_or(""),
                spec["status"].as_str().unwrap_or("active"),
            )
            .expect("enjambre");
        if let Some(ops) = spec["ops"].as_array() {
            for op in ops {
                apply(store, &swarm, op);
            }
        }
        created.push(swarm);
    }
    created
}

fn apply(store: &Db, swarm: &str, op: &Value) {
    let text = |key: &str| op[key].as_str().unwrap_or("").to_string();
    let text_or = |key: &str, fallback: &str| {
        op[key]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback)
            .to_string()
    };
    let id = |key: &str| op[key].as_i64().unwrap_or(0);
    let paths = || -> Option<Vec<String>> {
        op["paths"].as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
    };
    match text("op").as_str() {
        "join" => {
            store
                .join(
                    swarm,
                    &text("name"),
                    &text("role"),
                    &text("agent"),
                    &text("address"),
                    &text("model"),
                    &text("effort"),
                )
                .expect("join");
        }
        "task" => {
            store
                .add_task(
                    swarm,
                    &text("title"),
                    &text("role"),
                    &text("owner"),
                    &[],
                    id("parent"),
                )
                .expect("task");
        }
        "claim" => {
            store
                .claim(swarm, &text("member"), None, false)
                .expect("claim");
        }
        "finish" => {
            store
                .finish(
                    swarm,
                    &text("member"),
                    id("task"),
                    &text("result"),
                    op["ok"] != json!(false),
                    &text("ref"),
                )
                .expect("finish");
        }
        "incident" => {
            store
                .open_incident(swarm, &text("member"), &text("kind"), &text("detail"), 0.0)
                .expect("incident");
        }
        "post" => {
            store
                .post(
                    swarm,
                    &text_or("sender", "ez"),
                    &text("body"),
                    &text_or("to", "*"),
                    &text_or("kind", "info"),
                    &text("ref"),
                )
                .expect("post");
        }
        "reserve" => {
            let paths = paths().unwrap_or_default();
            store
                .reserve(swarm, &text("member"), &paths, 900.0)
                .expect("reserve");
        }
        "release" => {
            store
                .release(swarm, &text("member"), paths().as_deref())
                .expect("release");
        }
        "incident_state" => {
            store
                .set_incident(id("id"), &text("state"), &text("action"))
                .expect("incident_state");
        }
        "member_state" => {
            store
                .set_member_state(swarm, &text("member"), &text("state"))
                .expect("member_state");
        }
        "address" => {
            store
                .set_address(swarm, &text("member"), &text("address"))
                .expect("address");
        }
        "status" => {
            store.set_status(swarm, &text("status")).expect("status");
        }
        other => panic!("operación sin cubrir: {other}"),
    }
}

fn normalize_text(value: &str, swarm: &str, home: &Path) -> String {
    let mut text = if swarm.is_empty() {
        value.to_string()
    } else {
        value.replace(swarm, "<swarm>")
    };
    for prefix in [
        home.display().to_string(),
        home.display().to_string().replace('\\', "/"),
    ] {
        text = text.replace(&prefix, "<tmp>");
    }
    text.replace('\\', "/")
}

fn cell(value: ValueRef<'_>, column: &str, table: &str, swarm: &str, home: &Path) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => {
            if ["created", "updated", "ts", "seen", "expires", "resets"].contains(&column)
                && number != 0.0
            {
                json!((number * 1000.0).round() / 1000.0)
            } else {
                json!(number)
            }
        }
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            let text = normalize_text(&text, swarm, home);
            let text = if column == "token" && !text.is_empty() {
                "<token>".to_string()
            } else if column == "swarm" || (table == "swarm" && column == "id") {
                "<swarm>".to_string()
            } else {
                text
            };
            Value::String(text)
        }
        ValueRef::Blob(bytes) => json!(bytes),
    }
}

fn dump_board(path: &Path, swarm: &str, home: &Path) -> Value {
    let connection = rusqlite::Connection::open(path).expect("base del tablero");
    let mut tables = Map::new();
    for table in TABLES {
        let mut statement = connection
            .prepare(&format!("SELECT * FROM {table}"))
            .expect("consulta");
        let columns: Vec<String> = statement.column_names().map(str::to_string).collect();
        let mut rows: Vec<(String, Value)> = statement
            .query_map([], |row| {
                let mut data = Map::new();
                for (index, column) in columns.iter().enumerate() {
                    data.insert(
                        column.clone(),
                        cell(row.get_ref(index)?, column, table, swarm, home),
                    );
                }
                Ok(Value::Object(data))
            })
            .expect("filas")
            .filter_map(Result::ok)
            .map(|row| {
                let sorted = DumpOptions {
                    sort_keys: true,
                    ..DumpOptions::default()
                };
                let key = dumps_with(&row, &sorted);
                (key, row)
            })
            .collect();
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        tables.insert(
            table.to_string(),
            Value::Array(rows.into_iter().map(|(_, row)| row).collect()),
        );
    }
    Value::Object(tables)
}

fn tool_info(info: &ToolInfo) -> Value {
    json!({
        "level": info.level,
        "bulk": info.bulk,
        "always_confirm": info.always_confirm,
        "summary": info.summary,
    })
}

fn result_json(result: &ToolResult) -> Value {
    result.to_dict()
}

fn norm_strings(value: &Value, swarm: &str, home: &Path) -> Value {
    match value {
        Value::String(text) => Value::String(normalize_text(text, swarm, home)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| norm_strings(item, swarm, home))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), norm_strings(item, swarm, home)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn run_case(case: &FixtureCase, home: &Path) {
    let input = &case.input;
    let label = case.case.rsplit('/').next().unwrap_or_default();
    let command = input
        .get("command_dir")
        .and_then(Value::as_str)
        .unwrap_or(COMMAND_DIR);
    let context = input
        .get("context")
        .and_then(Value::as_str)
        .unwrap_or("C:/tmp/encargo.md");
    let rig = rig(home, label, command, context);
    let created = seed(
        rig.stack.store(),
        input.get("seeds").unwrap_or(&Value::Null),
    );
    let tools = build_swarm_tools(Some(Arc::clone(&rig.stack)));
    let tool = &tools[0];
    let args = input["args"].as_object().cloned().unwrap_or_default();
    let result = match (tool.handler)(&args) {
        Ok(ez_agent::registry::ToolReturn::Result(result)) => result,
        Ok(ez_agent::registry::ToolReturn::Value(value)) => ToolResult::ok(value),
        Err(error) => ToolResult::failed(error),
    };
    let info = tool.describe(&args);
    let swarm = if result.ok {
        result.data["swarm"].as_str().unwrap_or("").to_string()
    } else {
        String::new()
    };
    let swarm = if swarm.is_empty() {
        let latest = rig.stack.store().latest().unwrap_or_default();
        if latest.is_empty() {
            created.last().cloned().unwrap_or_default()
        } else {
            latest
        }
    } else {
        swarm
    };
    let board = if swarm.is_empty() {
        Value::Null
    } else {
        dump_board(&rig.base.join("board.db"), &swarm, home)
    };
    let saved = rig.host.saved.lock().expect("contextos").len();
    let value = json!({
        "result": norm_strings(&result_json(&result), &swarm, home),
        "info": norm_strings(&tool_info(&info), &swarm, home),
        "board": board,
        "saved_context": saved,
    });
    assert_eq!(value, case.output, "caso {}", case.case);
}

#[test]
fn every_tools_swarm_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tools_swarm");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 54, "faltan casos: {cases_in_file}");
    let home = std::env::temp_dir().join(format!("ez-tools-swarm-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("temporal");
    let mut run = 0;
    for case in &cases {
        match case.function.as_str() {
            "swarm" => {
                run_case(case, &home);
                run += 1;
            }
            "swarm.describe" => {
                let rig = rig(&home, "describe", COMMAND_DIR, "C:/tmp/encargo.md");
                let tool = &build_swarm_tools(Some(Arc::clone(&rig.stack)))[0];
                let cases_output = case.output["cases"].as_array().cloned().unwrap_or_default();
                let mut produced = Vec::new();
                for item in &cases_output {
                    let args = item["args"].as_object().cloned().unwrap_or_default();
                    produced.push(
                        json!({"args": item["args"], "info": tool_info(&tool.describe(&args))}),
                    );
                }
                assert_eq!(
                    Value::Array(produced),
                    case.output["cases"],
                    "caso {}",
                    case.case
                );
                run += 1;
            }
            "build_swarm_tools" => {
                let rig = rig(&home, "specs", COMMAND_DIR, "C:/tmp/encargo.md");
                let tools = build_swarm_tools(Some(Arc::clone(&rig.stack)));
                let without = build_swarm_tools(None);
                assert!(without.is_empty());
                let value = json!({
                    "tools": tools.iter().map(|tool| json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                        "level": tool.level,
                        "untrusted_output": tool.untrusted_output,
                        "timeout_s": tool.timeout_s,
                        "schema": tool.schema(),
                        "info": tool_info(&tool.describe(&Map::new())),
                    })).collect::<Vec<_>>(),
                    "without_stack": Value::Array(Vec::new()),
                });
                assert_eq!(value, case.output, "caso {}", case.case);
                run += 1;
            }
            other => panic!("función sin cubrir: {other}"),
        }
    }
    assert_eq!(run, cases_in_file, "se saltó algún caso");
    let _ = std::fs::remove_dir_all(&home);
}
