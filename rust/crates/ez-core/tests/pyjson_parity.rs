//! Paridad de `json.dumps` contra `parity/fixtures/pyjson.jsonl` (F2-01).

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_parity::fixtures;
use serde_json::Value;

fn options_from(value: &Value) -> DumpOptions {
    let mut options = DumpOptions::default();
    let Some(object) = value.as_object() else {
        return options;
    };
    if let Some(indent) = object.get("indent").and_then(Value::as_u64) {
        options = options.with_indent(indent as usize);
    }
    options.sort_keys = object
        .get("sort_keys")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    options.ensure_ascii = object
        .get("ensure_ascii")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if let Some(Value::Array(separators)) = object.get("separators") {
        if separators.len() == 2 {
            if let Some(item) = separators[0].as_str() {
                options.item_separator = item.to_string();
            }
            if let Some(key) = separators[1].as_str() {
                options.key_separator = key.to_string();
            }
        }
    }
    options
}

#[test]
fn every_dumps_case_matches_python() {
    let path = std::path::Path::new("../../../parity/fixtures/pyjson.jsonl");
    let cases = fixtures::load(path).expect("fixtures de pyjson legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.function != "dumps" {
            continue;
        }
        let value = case.input.get("value").cloned().unwrap_or(Value::Null);
        let options = options_from(case.input.get("options").unwrap_or(&Value::Null));
        let produced = dumps_with(&value, &options);
        let expected = case
            .output
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(produced, expected, "caso {}", case.case);
        cases_run += 1;
    }
    let in_file = cases.iter().filter(|case| case.function == "dumps").count();
    assert_eq!(cases_run, in_file, "se saltó algún caso");
    assert!(cases_run >= 30, "muy pocos casos: {cases_run}");
}

#[test]
fn round_trip_keeps_values() {
    let path = std::path::Path::new("../../../parity/fixtures/pyjson.jsonl");
    let cases = fixtures::load(path).expect("fixtures de pyjson legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.function != "round_trip" {
            continue;
        }
        let value = case.input.get("value").cloned().unwrap_or(Value::Null);
        let text = dumps_with(&value, &DumpOptions::default());
        let expected = case
            .output
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(text, expected, "caso {}", case.case);
        let back: Value = serde_json::from_str(&text).expect("json válido");
        assert_eq!(back, value, "round-trip {}", case.case);
        cases_run += 1;
    }
    let in_file = cases
        .iter()
        .filter(|case| case.function == "round_trip")
        .count();
    assert_eq!(cases_run, in_file);
}
