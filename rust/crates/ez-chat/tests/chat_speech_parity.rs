//! Paridad de `voice/chat/speech.py` contra `parity/fixtures/chat_speech.jsonl`.

use std::path::Path;

use ez_chat::speech::spoken_version;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/chat_speech.jsonl";

/// La firma Rust pide `usize`; el oráculo incluye `None` y texto como error.
fn consumes_type_boundary(case: &FixtureCase) {
    assert!(
        case.input
            .get("max_chars")
            .is_some_and(|value| value.is_null() || value.is_string()),
        "caso {}: el error no viene de `max_chars`",
        case.case
    );
}

#[test]
fn every_chat_speech_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat_speech legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.error.is_some() {
            consumes_type_boundary(case);
            cases_run += 1;
            continue;
        }
        assert_eq!(case.function, "spoken_version", "caso {}", case.case);
        let markdown = case
            .input
            .get("markdown")
            .and_then(Value::as_str)
            .expect("markdown");
        let max_chars = case
            .input
            .get("max_chars")
            .and_then(Value::as_u64)
            .expect("max_chars") as usize;
        assert_eq!(
            json!({"text": spoken_version(markdown, max_chars)}),
            case.output,
            "caso {}",
            case.case
        );
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 45, "muy pocos casos: {cases_in_file}");
}
