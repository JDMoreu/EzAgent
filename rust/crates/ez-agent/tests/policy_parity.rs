//! Paridad de `decide` contra `parity/fixtures/policy.jsonl` (F2-04).

use std::path::Path;

use ez_agent::policy::decide_with_threshold;
use ez_parity::fixtures;
use serde_json::Value;

fn number(input: &Value, key: &str) -> i64 {
    input
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("falta el entero {key}"))
}

fn flag(input: &Value, key: &str) -> bool {
    input
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("falta el booleano {key}"))
}

fn text<'value>(input: &'value Value, key: &str) -> &'value str {
    input
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

#[test]
fn every_decide_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/policy.jsonl");
    let cases = fixtures::load(path).expect("fixtures de policy legibles");
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(
            case.function, "decide",
            "función inesperada en {}",
            case.case
        );
        let decision = decide_with_threshold(
            number(&case.input, "tool_level"),
            flag(&case.input, "tainted"),
            number(&case.input, "bulk"),
            flag(&case.input, "always_confirm"),
            flag(&case.input, "confirm_nondestructive"),
            number(&case.input, "bulk_threshold"),
        );
        assert_eq!(
            decision.action.as_str(),
            text(&case.output, "action"),
            "caso {}",
            case.case
        );
        assert_eq!(
            decision.level,
            number(&case.output, "level"),
            "caso {}",
            case.case
        );
        assert_eq!(
            decision.reason,
            text(&case.output, "reason"),
            "caso {}",
            case.case
        );
        assert_eq!(case.error, None, "caso {} con error", case.case);
        cases_run += 1;
    }
    let cases_in_file = cases
        .iter()
        .filter(|case| case.function == "decide")
        .count();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}
