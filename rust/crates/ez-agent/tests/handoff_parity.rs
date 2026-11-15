//! Paridad de `handoff.py` contra `parity/fixtures/handoff.jsonl` (P-AGENT-B).
//!
//! Funciones puras: no tocan el entorno ni el directorio actual.

use std::path::Path;

use ez_agent::handoff::{answer_or_tail, clean_tail, compose_handoff, extract_answer};
use ez_parity::fixtures;
use serde_json::Value;

fn text<'value>(value: &'value Value, key: &str) -> &'value str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

/// `str(x or "")` del Python: `None` y la ausencia llegan como texto vacío.
fn argument<'value>(value: &'value Value, key: &str) -> &'value str {
    match value.get(key) {
        None | Some(Value::Null) => "",
        Some(Value::String(text)) => text,
        Some(other) => panic!("{key} no es texto: {other}"),
    }
}

#[test]
fn every_handoff_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/handoff.jsonl");
    let cases = fixtures::load(path).expect("fixtures de handoff legibles");
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(case.error, None, "caso {} con error", case.case);
        let produced = match case.function.as_str() {
            "extract_answer" => extract_answer(
                argument(&case.input, "screen"),
                argument(&case.input, "sent_task"),
            ),
            "clean_tail" => clean_tail(argument(&case.input, "screen")),
            "answer_or_tail" => answer_or_tail(
                argument(&case.input, "screen"),
                argument(&case.input, "sent_task"),
            ),
            "compose_handoff" => compose_handoff(
                argument(&case.input, "task"),
                argument(&case.input, "answer"),
            ),
            other => panic!("función desconocida: {other}"),
        };
        assert_eq!(produced, text(&case.output, "text"), "caso {}", case.case);
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 123, "muy pocos casos: {cases_in_file}");
}
