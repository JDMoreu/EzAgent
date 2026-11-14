//! Paridad de `speech_text` contra `parity/fixtures/speech_text.jsonl` (F2-06).

use ez_core::speech_text::{normalize_for_kokoro, normalize_for_speech};
use ez_parity::fixtures;
use serde_json::Value;

fn text_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[test]
fn every_speech_text_case_matches_python() {
    let path = std::path::Path::new("../../../parity/fixtures/speech_text.jsonl");
    let cases = fixtures::load(path).expect("fixtures de speech_text legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        if case.error.is_some() {
            // Python falla con `None`; la firma Rust no admite texto ausente.
            assert!(
                text_field(&case.input, "text").is_none(),
                "caso {}: error inesperado sin texto nulo",
                case.case
            );
            continue;
        }
        let text = text_field(&case.input, "text").expect("texto de entrada");
        let produced = match case.function.as_str() {
            "normalize_for_speech" => normalize_for_speech(text),
            "normalize_for_kokoro" => normalize_for_kokoro(text),
            other => panic!("función desconocida: {other}"),
        };
        let expected = text_field(&case.output, "text").expect("texto esperado");
        assert_eq!(produced, expected, "caso {}", case.case);
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 452, "muy pocos casos: {cases_run}");
}
