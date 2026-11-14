//! Paridad de `redact` contra `parity/fixtures/redact.jsonl` (F2-06).

use ez_core::redact::redact;
use ez_parity::fixtures;
use serde_json::Value;

#[test]
fn every_redact_case_matches_python() {
    let path = std::path::Path::new("../../../parity/fixtures/redact.jsonl");
    let cases = fixtures::load(path).expect("fixtures de redact legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        assert_eq!(
            case.function, "redact",
            "función inesperada en {}",
            case.case
        );
        let text = case
            .input
            .get("text")
            .and_then(Value::as_str)
            .expect("texto de entrada");
        let produced = redact(text);
        let expected = case
            .output
            .get("text")
            .and_then(Value::as_str)
            .expect("texto esperado");
        assert_eq!(produced, expected, "caso {}", case.case);
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 53, "muy pocos casos: {cases_run}");
}
