//! Paridad del dominio `wake` contra `parity/fixtures/wake.jsonl`.
//!
//! Cubre `normalize`, `wake_forms`, `is_wake_phrase`, `utterance_secs` y
//! `worth_transcribing` de `ezagent.voice.wake`.

use std::path::Path;

use ez_audio::wake::{is_wake_phrase, normalize, utterance_secs, wake_forms, worth_transcribing};
use ez_parity::fixtures::{self, FixtureCase};

const FIXTURES: &str = "../../../parity/fixtures/wake.jsonl";
const TOTAL_CASES: usize = 264;

#[test]
fn every_wake_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de wake legibles");
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "normalize" => compare_normalize(case),
            "wake_forms" => compare_wake_forms(case),
            "is_wake_phrase" => compare_is_wake(case),
            "utterance_secs" => compare_utterance(case),
            "worth_transcribing" => compare_worth(case),
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert!(
        cases_in_file >= TOTAL_CASES,
        "se esperaban al menos {TOTAL_CASES} casos, hay {cases_in_file}"
    );
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}

/// `str(text or "")`: los casos nulos llegan como texto vacío.
fn text_of(case: &FixtureCase) -> String {
    case.input["text"].as_str().unwrap_or("").to_string()
}

fn phrase_of(case: &FixtureCase) -> String {
    case.input["phrase"].as_str().unwrap_or("").to_string()
}

fn compare_normalize(case: &FixtureCase) {
    assert_eq!(
        normalize(&text_of(case)),
        case.output["text"].as_str().expect("text"),
        "caso {}",
        case.case
    );
}

fn compare_wake_forms(case: &FixtureCase) {
    let mut produced: Vec<String> = wake_forms(&phrase_of(case)).into_iter().collect();
    produced.sort();
    let expected: Vec<String> = case.output["forms"]
        .as_array()
        .expect("forms")
        .iter()
        .map(|value| value.as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(produced, expected, "caso {}", case.case);
}

fn compare_is_wake(case: &FixtureCase) {
    assert_eq!(
        is_wake_phrase(&text_of(case), &phrase_of(case)),
        case.output["wake"].as_bool().expect("wake"),
        "caso {}",
        case.case
    );
}

fn pcm_of(case: &FixtureCase) -> Vec<u8> {
    let spec = &case.input["pcm"];
    match spec["kind"].as_str().expect("kind del pcm") {
        "hex" => {
            let text = spec["hex"].as_str().unwrap_or("");
            text.as_bytes()
                .chunks(2)
                .filter(|pair| pair.len() == 2)
                .map(|pair| {
                    let high = (pair[0] as char).to_digit(16).unwrap_or(0);
                    let low = (pair[1] as char).to_digit(16).unwrap_or(0);
                    (high * 16 + low) as u8
                })
                .collect()
        }
        "pattern" => {
            let size = spec["bytes"].as_u64().unwrap_or(0) as usize;
            let seed = spec["seed"].as_u64().unwrap_or(0) as usize;
            (0..size)
                .map(|index| ((seed + index) % 256) as u8)
                .collect()
        }
        other => panic!("pcm desconocido: {other}"),
    }
}

/// DECISION: `rate` es `u32`; el Python devuelve 0.0 con tasas ≤ 0 y aquí se
/// pasa 0, que da el mismo resultado.
fn rate_of(case: &FixtureCase) -> u32 {
    case.input["rate"].as_i64().unwrap_or(0).max(0) as u32
}

fn compare_utterance(case: &FixtureCase) {
    let produced = utterance_secs(&pcm_of(case), rate_of(case));
    let expected = case.output["secs"].as_f64().expect("secs");
    // DECISION: sin `float_roundtrip`, `serde_json` puede desviarse 1 ulp al
    // leer el decimal del fixture; la tolerancia f64 cubre ese error.
    let tolerance = 1e-9 * expected.abs().max(1.0);
    assert!(
        (produced - expected).abs() <= tolerance,
        "caso {}: {produced} != {expected}",
        case.case
    );
}

fn compare_worth(case: &FixtureCase) {
    assert_eq!(
        worth_transcribing(&pcm_of(case), rate_of(case)),
        case.output["worth"].as_bool().expect("worth"),
        "caso {}",
        case.case
    );
}
