//! Paridad de `tools_herdr` contra `parity/fixtures/tools_herdr.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: Herdr y los callbacks son dobles que registran cada
//! llamada; no se abre ninguna terminal ni se ejecuta ningún proceso.

use std::sync::{Arc, Mutex};

use ez_agent::registry::ToolResult;
use ez_agent::tools_herdr::args::{
    action, checked_action, last_address, optional_bool, optional_lines, optional_text,
    required_text, shown, user_text, wants_close, wants_deliver, wants_notify, wrapped_value,
};
use ez_agent::tools_herdr::builder::build_herdr_tools_with_port;
use ez_agent::tools_herdr::delegate::{handle_delegate, handle_run_with_ids, read_output};
use ez_agent::tools_herdr::port::HerdrPort;
use ez_agent::tools_herdr::summaries::{
    delegate_info, delegate_summary, labelled, voice_info, voice_summary, with_labels,
};
use ez_agent::tools_herdr::voice::handle_voice;

use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_herdr.jsonl";
const KNOWN: [&str; 7] = [
    "claude", "opencode", "codex", "mcode", "minimax", "pi", "qwen",
];

pub(crate) type Calls = Arc<Mutex<Vec<Value>>>;

#[path = "tools_herdr_parity/fake.rs"]
mod fake;
use fake::{callbacks, info_json, map, resolver, state, FakePort};

fn run_handler_case(case: &FixtureCase, kind: &str) {
    let input = &case.input;
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let port = FakePort::new(input.get("herdr").unwrap_or(&Value::Null), &calls);
    let callbacks = callbacks(input.get("callbacks").unwrap_or(&Value::Null), &calls);
    let session = state(input);
    let args = input["args"].as_object().cloned().unwrap_or_default();
    let result = match kind {
        "delegate" => handle_delegate(&port, &callbacks, &session, &args),
        "run" => {
            let ids = Arc::new(Mutex::new(0_u64));
            let next = move || {
                let mut index = ids.lock().expect("ids");
                *index += 1;
                match *index {
                    1 => "a1b2c3d4e5f60718".to_string(),
                    2 => "b2c3d4e5f6071829".to_string(),
                    _ => "c3d4e5f60718293a".to_string(),
                }
            };
            match handle_run_with_ids(&port, &callbacks, &session, &args, &next) {
                Ok(result) => result,
                Err(error) => {
                    // `_handle_run` no captura: el fixture guarda el texto en `error`.
                    assert_eq!(error.into_result().error, case.error, "caso {}", case.case);
                    assert_eq!(case.output, Value::Null, "caso {}", case.case);
                    return;
                }
            }
        }
        _ => handle_voice(&callbacks, &session, &args),
    };
    let value = json!({
        "result": result.to_dict(),
        "calls": Value::Array(calls.lock().expect("llamadas").clone()),
        "last_address": last_address(&session),
    });
    assert_eq!(value, case.output, "caso {}", case.case);
}

#[test]
fn every_tools_herdr_case_matches_python() {
    let cases = fixtures::load(std::path::Path::new(FIXTURES)).expect("fixtures");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 393, "faltan casos: {cases_in_file}");
    let mut run = 0;
    for case in &cases {
        let input = &case.input;
        match case.function.as_str() {
            "_action" => {
                let args = input["args"].as_object().cloned().unwrap_or_default();
                assert_eq!(
                    json!({"action": action(&args)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_checked_action" => {
                let args = input["args"].as_object().cloned().unwrap_or_default();
                let valid: Vec<&str> = input["valid"]
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let outcome = checked_action(&args, &valid, input["tool"].as_str().unwrap_or(""));
                match outcome {
                    Ok(value) => assert_eq!(json!({"action": value}), case.output, "{}", case.case),
                    Err(error) => assert_eq!(Some(&error.0), case.error.as_ref(), "{}", case.case),
                }
            }
            "_required_text" => {
                let args = map(input["args"].clone());
                let outcome = required_text(
                    &args,
                    input["key"].as_str().unwrap(),
                    input["action"].as_str().unwrap(),
                );
                match outcome {
                    Ok(value) => assert_eq!(json!({"text": value}), case.output, "{}", case.case),
                    Err(error) => assert_eq!(Some(&error.0), case.error.as_ref(), "{}", case.case),
                }
            }
            "_optional_text" => {
                let args = map(input["args"].clone());
                match optional_text(
                    &args,
                    input["key"].as_str().unwrap(),
                    input["action"].as_str().unwrap(),
                ) {
                    Ok(value) => assert_eq!(json!({"text": value}), case.output, "{}", case.case),
                    Err(error) => assert_eq!(Some(&error.0), case.error.as_ref(), "{}", case.case),
                }
            }
            "_optional_bool" => {
                let args = map(input["args"].clone());
                match optional_bool(
                    &args,
                    input["key"].as_str().unwrap(),
                    input["action"].as_str().unwrap(),
                ) {
                    Ok(value) => assert_eq!(json!({"value": value}), case.output, "{}", case.case),
                    Err(error) => assert_eq!(Some(&error.0), case.error.as_ref(), "{}", case.case),
                }
            }
            "_optional_lines" => {
                let args = map(input["args"].clone());
                match optional_lines(&args, "read") {
                    Ok(value) => assert_eq!(json!({"lines": value}), case.output, "{}", case.case),
                    Err(error) => assert_eq!(Some(&error.0), case.error.as_ref(), "{}", case.case),
                }
            }
            "_wrapped" => {
                let value = json!({"result": wrapped_value(&input["value"]).to_dict()});
                assert_eq!(value, case.output, "{}", case.case);
            }
            "_user_text" => {
                let calls: Calls = Arc::new(Mutex::new(Vec::new()));
                let fake = callbacks(&input["callbacks"], &calls);
                assert_eq!(
                    json!({"text": user_text(&fake)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_wants_notify" => {
                let text = input["text"].as_str().unwrap_or("");
                assert_eq!(
                    json!({"notify": wants_notify(text), "deliver": wants_deliver(text),
                    "close": wants_close(text)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_delegate_summary" => {
                let args = map(input["args"].clone());
                let resolve = resolver(input);
                assert_eq!(
                    json!({"summary": delegate_summary(&args, &resolve)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_voice_summary" => {
                let args = map(input["args"].clone());
                let resolve = resolver(input);
                assert_eq!(
                    json!({"summary": voice_summary(&args, &resolve)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_delegate_info" | "_voice_info" => {
                let args = map(input["args"].clone());
                let mapping = input.get("label_map").cloned().unwrap_or(Value::Null);
                let resolve =
                    move |address: &str| mapping[address].as_str().unwrap_or("").to_string();
                let info = if case.function == "_delegate_info" {
                    delegate_info(&args, &resolve)
                } else {
                    voice_info(&args, &resolve)
                };
                assert_eq!(info_json(&info), case.output, "{}", case.case);
            }
            "_shown" => {
                let mapping = input.get("label_map").cloned().unwrap_or(Value::Null);
                let name = input["resolver"].as_str().unwrap_or("");
                let resolve: Box<dyn Fn(&str) -> String> = match name {
                    "known" | "unknown" => {
                        Box::new(move |address| mapping[address].as_str().unwrap_or("").to_string())
                    }
                    "blank" => Box::new(|_| "   ".to_string()),
                    "raise" => Box::new(|_| panic!("x")),
                    _ => Box::new(|_| String::new()),
                };
                assert_eq!(
                    json!({"shown": shown(input["address"].as_str().unwrap_or(""), &resolve)}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_labelled" => {
                let mapping = input.get("labels").cloned().unwrap_or(json!({}));
                let resolve =
                    move |address: &str| mapping[address].as_str().unwrap_or("").to_string();
                let mut item = map(input["item"].clone());
                labelled(&resolve, &mut item);
                assert_eq!(json!({"item": item}), case.output, "{}", case.case);
            }
            "_with_labels" => {
                let mapping = input.get("labels").cloned().unwrap_or(json!({}));
                let resolve =
                    move |address: &str| mapping[address].as_str().unwrap_or("").to_string();
                let result = with_labels(&resolve, ToolResult::ok(input["data"].clone()));
                assert_eq!(
                    json!({"result": result.to_dict()}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "_handle_delegate" => run_handler_case(case, "delegate"),
            "_handle_run" => run_handler_case(case, "run"),
            "_handle_voice" => run_handler_case(case, "voice"),
            "_read_output" => {
                let calls: Calls = Arc::new(Mutex::new(Vec::new()));
                let behavior = input["behavior"].as_str().unwrap_or("");
                let spec = if behavior == "error" {
                    json!({"read_full_error": true})
                } else {
                    json!({"read_full": input["fallback_text"]})
                };
                let port = FakePort::new(&spec, &calls);
                assert_eq!(
                    json!({"text": read_output(&port, "t1")}),
                    case.output,
                    "{}",
                    case.case
                );
            }
            "build_herdr_tools" => run_builder_case(case),
            other => panic!("función sin cubrir: {other}"),
        }
        run += 1;
    }
    assert_eq!(run, cases_in_file, "se saltó algún caso");
}

fn run_builder_case(case: &FixtureCase) {
    let input = &case.input;
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let labels = input.get("labels").cloned().unwrap_or(json!({}));
    let mapping = if labels.is_object() {
        labels
    } else if labels == json!("map") {
        json!({"t1": "Claude de EzAgent"})
    } else {
        json!({})
    };
    let listed = if case.case.ends_with("004-handler-labels") {
        json!([{"address": "t1", "name": "Crudo"}])
    } else {
        json!([{"address": "t1"}])
    };
    let spec = json!({
        "labels": mapping,
        "label_raises": input["label_raises"] == json!(true),
        "list": listed,
    });
    let port: Arc<dyn HerdrPort> = Arc::new(FakePort::new(&spec, &calls));
    let callbacks = callbacks(&Value::Null, &calls);
    let tools = build_herdr_tools_with_port(port, callbacks);
    if case.case.ends_with("004-handler-labels") || case.case.ends_with("005-handler-error") {
        let args = if case.case.ends_with("004-handler-labels") {
            json!({"action": "list"})
        } else {
            json!({"action": "nada"})
        };
        let result = match (tools[0].handler)(args.as_object().unwrap()) {
            Ok(ez_agent::registry::ToolReturn::Result(result)) => result,
            Ok(ez_agent::registry::ToolReturn::Value(value)) => ToolResult::ok(value),
            Err(error) => ToolResult::failed(error),
        };
        assert_eq!(
            json!({"result": result.to_dict()}),
            case.output,
            "{}",
            case.case
        );
        return;
    }
    let described: Vec<Value> = tools
        .iter()
        .flat_map(|tool| {
            [
                json!({"action": "run", "agent": "claude", "task": "x"}),
                json!({"action": "kill", "address": "t1"}),
                json!({"action": "close_and_watch", "address": "t1"}),
            ]
            .into_iter()
            .map(move |args| {
                let info = tool.describe(args.as_object().unwrap());
                json!({"name": tool.name, "args": args, "info": info_json(&info)})
            })
        })
        .collect();
    let handler = if input["handler"] == json!("list") {
        let result = match (tools[0].handler)(json!({"action": "list"}).as_object().unwrap()) {
            Ok(ez_agent::registry::ToolReturn::Result(result)) => result,
            Ok(ez_agent::registry::ToolReturn::Value(value)) => ToolResult::ok(value),
            Err(error) => ToolResult::failed(error),
        };
        result.to_dict()
    } else {
        Value::Null
    };
    let value = json!({
        "specs": tools.iter().map(|tool| json!({"name": tool.name, "description": tool.description,
            "parameters": tool.parameters, "level": tool.level,
            "untrusted_output": tool.untrusted_output, "timeout_s": tool.timeout_s,
            "schema": tool.schema()})).collect::<Vec<_>>(),
        "described": described,
        "handler": handler,
    });
    assert_eq!(value, case.output, "{}", case.case);
}
