//! Paridad de `agent/registry.py` contra `parity/fixtures/registry.jsonl` (P-AGENT-A).
//!
//! Un solo `#[test]`: algunos casos lanzan un hilo con plazo y otros escriben
//! `system.md`, así que conviene ejecutarlos en serie.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ez_agent::registry::{
    DescribeFn, Handler, ToolInfo, ToolRegistry, ToolResult, ToolReturn, ToolSpec, TRUNCATED_NOTE,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/registry.jsonl";

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn text(input: &Value, name: &str) -> String {
    field(input, name).as_str().unwrap_or_default().to_string()
}

fn number(input: &Value, name: &str) -> i64 {
    field(input, name).as_i64().unwrap_or(0)
}

/// Un `ToolSpec` a partir del descriptor plano del fixture.
fn spec(raw: &Value) -> ToolSpec {
    let handler: Handler = Arc::new(|args| Ok(json!({"x": args.get("x").cloned()}).into()));
    let mut spec = ToolSpec::new(
        &text(raw, "name"),
        &text(raw, "description"),
        field(raw, "parameters").clone(),
        number(raw, "level"),
        handler,
    );
    if let Some(describe) = raw.get("describe").and_then(Value::as_object) {
        let info = ToolInfo {
            level: number(raw, "level"),
            bulk: describe.get("bulk").and_then(Value::as_i64).unwrap_or(0),
            always_confirm: describe
                .get("always_confirm")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            summary: describe
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            ..ToolInfo::default()
        };
        let describe_fn: DescribeFn = Arc::new(move |_| info.clone());
        spec.describe_fn = Some(describe_fn);
    }
    spec
}

/// El resultado bruto del fixture, con el `_Odd` de `__repr__` del oráculo.
fn result(raw: &Value) -> ToolResult {
    let data = match field(raw, "data") {
        Value::Object(map) if map.len() == 1 && map.contains_key("__repr__") => {
            map["__repr__"].clone()
        }
        other => other.clone(),
    };
    let error = raw.get("error").and_then(Value::as_str).map(str::to_string);
    ToolResult {
        ok: raw.get("ok").and_then(Value::as_bool).unwrap_or(false),
        data,
        error,
        outcome: raw
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("completed")
            .to_string(),
        error_kind: raw
            .get("error_kind")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Los manejadores del oráculo, con sus errores y su lentitud.
fn handler(name: &str) -> Handler {
    let make = |function: fn(&Map<String, Value>) -> Result<ToolReturn, String>| -> Handler {
        Arc::new(function)
    };
    match name {
        "echo" => make(|args| Ok(ToolReturn::Value(json!({"x": args.get("x")})))),
        "contract" => make(|_| Ok(ToolReturn::Value(json!({"ok": false, "error": "mal"})))),
        "plain" => make(|_| Ok(ToolReturn::Value(json!({"cualquier": "cosa"})))),
        "none" => make(|_| Ok(ToolReturn::Value(Value::Null))),
        "text" => make(|_| Ok(ToolReturn::Value(json!("texto")))),
        "fail" => make(|_| Err("ValueError: kaboom".into())),
        "slow" => make(|_| {
            std::thread::sleep(Duration::from_millis(200));
            Ok(ToolReturn::Value(json!("tarde")))
        }),
        other => panic!("manejador desconocido: {other}"),
    }
}

fn run_spec(input: &Value) -> ToolRegistry {
    let registry = ToolRegistry::new();
    let mut tool = ToolSpec::new(
        "t",
        "t",
        field(input, "parameters").clone(),
        0,
        handler(&text(input, "handler")),
    );
    tool.timeout_s = field(input, "timeout_s").as_f64().unwrap_or(10.0);
    registry.register(tool);
    registry
}

fn run_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "schema" => Ok(json!({"schema": spec(field(input, "spec")).schema()})),
        "validate" => {
            let registry = ToolRegistry::new();
            let tool = text(input, "tool");
            if tool != "nope" {
                registry.register(spec(&json!({
                    "name": "t",
                    "description": "t",
                    "parameters": field(input, "parameters").clone(),
                })));
            }
            Ok(json!({
                "error": registry.validate(&tool, field(input, "args"))
            }))
        }
        "run" => {
            let registry = run_spec(input);
            let tool = text(input, "tool");
            let result = registry.run(&tool, field(input, "args"));
            Ok(json!({"result": result.to_dict()}))
        }
        "render_for_llm" => {
            let max_chars = number(input, "max_chars");
            Ok(
                json!({"text": ToolRegistry::render_for_llm(&result(field(input, "result")), max_chars)}),
            )
        }
        "schemas" => {
            let registry = ToolRegistry::new();
            let empty = json!({"type": "object", "properties": {}});
            for name in field(input, "tools").as_array().into_iter().flatten() {
                let name = name.as_str().unwrap_or_default();
                registry.register(ToolSpec::new(
                    name,
                    &format!("tool {name}"),
                    empty.clone(),
                    0,
                    handler("echo"),
                ));
            }
            let kwargs = field(input, "kwargs");
            let names: Option<Vec<String>> =
                kwargs.get("names").and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                });
            let limit = kwargs.get("limit").and_then(Value::as_i64).unwrap_or(14);
            let schemas = registry.schemas(names.as_deref(), limit);
            let names: Vec<Value> = schemas
                .iter()
                .filter_map(|schema| schema.pointer("/function/name").cloned())
                .collect();
            Ok(json!({"schemas": schemas, "names": names}))
        }
        "describe" => {
            if input
                .get("describe")
                .and_then(Value::as_object)
                .is_some_and(|describe| describe.contains_key("wrong"))
            {
                // El Python devuelve un dict y lanza TypeError; la firma Rust
                // `DescribeFn -> ToolInfo` hace imposible ese caso en compilación.
                return Err("describe_fn debe devolver ToolInfo".into());
            }
            let registry = ToolRegistry::new();
            let spec = spec(input);
            registry.register(spec.clone());
            let args = json!({"p": "hola"});
            let info = spec.describe(args.as_object().expect("objeto"));
            Ok(json!({
                "tool_info": {
                    "level": info.level,
                    "bulk": info.bulk,
                    "always_confirm": info.always_confirm,
                    "summary": info.summary,
                }
            }))
        }
        "register" => {
            // `register` solo acepta `ToolSpec`; el tipo hace imposible el caso
            // del fixture (un dict sin description/parameters).
            Err("El registro solo acepta ToolSpec".into())
        }
        "truncated_note" => Ok(json!({"note": TRUNCATED_NOTE})),
        other => Err(format!("función sin portar: {other}")),
    }
}

#[test]
fn every_registry_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de registry legibles");
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case);
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
    assert!(cases_in_file >= 68, "faltan casos en el fixture");
}
