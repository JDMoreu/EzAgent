//! Paridad de plegado de caja en `stt_dictionary` contra `parity/fixtures/stt_fold.jsonl` (F2-06).

use ez_parity::fixtures::{self, FixtureCase};
use ez_stt::dictionary::{correct_transcription, parse_rules, transcription_hint, DictionaryError};
use serde_json::{json, Value};

fn text_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn agents_of(value: &Value) -> Vec<String> {
    value
        .get("agents")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn rules_to_json(rules: &[(String, String)]) -> Value {
    let pairs: Vec<Value> = rules
        .iter()
        .map(|(source, target)| json!([source, target]))
        .collect();
    json!({ "rules": pairs })
}

fn produced(case: &FixtureCase) -> Result<Value, DictionaryError> {
    let input = &case.input;
    match case.function.as_str() {
        "parse_rules" => parse_rules(text_field(input, "value")).map(|rules| rules_to_json(&rules)),
        "transcription_hint" => transcription_hint(
            text_field(input, "prompt"),
            text_field(input, "dictionary"),
            &agents_of(input),
        )
        .map(|text| json!({ "text": text })),
        "correct_transcription" => {
            correct_transcription(text_field(input, "text"), text_field(input, "dictionary"))
                .map(|text| json!({ "text": text }))
        }
        other => panic!("función desconocida: {other}"),
    }
}

#[test]
fn every_fold_case_matches_python() {
    let path = std::path::Path::new("../../../parity/fixtures/stt_fold.jsonl");
    let cases = fixtures::load(path).expect("fixtures de stt_fold legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        match (produced(case), &case.error) {
            (Ok(value), None) => assert_eq!(value, case.output, "caso {}", case.case),
            (Err(error), Some(expected)) => {
                assert_eq!(&error.to_string(), expected, "caso {}", case.case);
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
    assert!(cases_run >= 27, "muy pocos casos: {cases_run}");
}
