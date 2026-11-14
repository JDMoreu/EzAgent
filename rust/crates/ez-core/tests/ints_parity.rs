//! Paridad de `VoiceConfig._int` contra `parity/fixtures/ints.jsonl` (F2-02).

use std::path::Path;

use ez_core::config::int_value;
use ez_parity::fixtures;
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/ints.jsonl";

#[test]
fn every_int_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de ints legibles");
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(case.function, "_int", "caso {}", case.case);
        assert_eq!(case.error, None, "caso {}", case.case);
        let value = case.input.get("value").cloned().unwrap_or(Value::Null);
        let default = case
            .input
            .get("default")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| panic!("default entero en {}", case.case));
        let produced = json!(int_value(&value, default));
        let expected = case.output.get("int").cloned().unwrap_or(Value::Null);
        assert_eq!(produced, expected, "caso {}", case.case);
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "se saltó algún caso de {FIXTURES}"
    );
}
