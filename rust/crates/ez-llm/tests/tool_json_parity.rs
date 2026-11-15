//! Paridad de `src/ezagent/voice/tool_json.py` contra `parity/fixtures/tool_json.jsonl`.

use std::path::Path;

use ez_llm::tool_json::{
    close_json_value, is_tool_json_error, normalize_roles, repair_arguments,
    repair_structure_checked, sanitize_history_checked,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/tool_json.jsonl";

/// Los mensajes del caso como arreglo; `None` si no vienen.
fn messages_of(case: &FixtureCase) -> Vec<Value> {
    case.input
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn raw_of(case: &FixtureCase) -> Option<&str> {
    case.input.get("raw").and_then(Value::as_str)
}

fn produced(case: &FixtureCase) -> Result<Value, String> {
    match case.function.as_str() {
        "close_json" => {
            let raw = case.input.get("raw").unwrap_or(&Value::Null);
            close_json_value(raw).map(|text| json!({"text": text}))
        }
        "repair_arguments" => {
            let (fixed, complete) = repair_arguments(raw_of(case));
            Ok(json!({"json": fixed, "complete": complete}))
        }
        "sanitize_history" => {
            let mut messages = messages_of(case);
            let fixed = sanitize_history_checked(&mut messages)?;
            Ok(json!({"fixed": fixed, "messages": messages}))
        }
        "is_tool_json_error" => {
            let message = case
                .input
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(json!({"result": is_tool_json_error(message)}))
        }
        "repair_structure" => {
            let mut messages = messages_of(case);
            let changes = repair_structure_checked(&mut messages)?;
            Ok(json!({"changes": changes, "messages": messages}))
        }
        "normalize_roles" => {
            let strict = case
                .input
                .get("strict")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let messages = normalize_roles(&messages_of(case), strict);
            Ok(json!({"messages": messages}))
        }
        other => panic!("función desconocida: {other}"),
    }
}

#[test]
fn every_tool_json_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tool_json legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        match (produced(case), &case.error) {
            (Ok(value), None) => assert_eq!(value, case.output, "caso {}", case.case),
            (Err(error), Some(expected)) => {
                assert_eq!(&error, expected, "caso {}", case.case);
            }
            (Ok(value), Some(expected)) => panic!(
                "caso {}: Python falla con {expected:?} y Rust devolvió {value}",
                case.case
            ),
            (Err(error), None) => panic!("caso {}: fallo inesperado: {error}", case.case),
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 169, "muy pocos casos: {cases_run}");
}
