//! Paridad de `voice/chat/trace.py` contra `parity/fixtures/chat_trace.jsonl`.
//!
//! El oráculo marcó la zona «UTC», pero en Windows `time.tzset()` no existe y
//! `time.localtime` siguió usando la zona de la máquina que generó los casos
//! (UTC−5 fija). El desfase se deduce del propio fixture y se comprueba que sea
//! el mismo en todos los casos; así la prueba no depende de esta máquina.

use std::path::Path;

use chrono::{NaiveDate, TimeZone, Utc};
use ez_chat::session::ChatMessage;
use ez_chat::store::{Role, Source};
use ez_chat::trace::{stamp_for_with_offset, stamp_with_offset};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::Value;

const FIXTURES: &str = "../../../parity/fixtures/chat_trace.jsonl";

/// La firma Rust usa `f64`; estos casos del oráculo pasan `None`/texto.
fn consumes_type_boundary(case: &FixtureCase) {
    let t = case.input.get("t");
    assert!(
        t.is_none() || t.is_some_and(Value::is_null) || t.is_some_and(Value::is_string),
        "caso {}: el error no viene de `t`",
        case.case
    );
}

/// Desfase de la máquina del oráculo: hora de pared del texto menos el instante.
fn oracle_offset(t: f64, stamp: &str) -> i32 {
    let (date, clock) = stamp.split_once('·').expect("texto con «·»");
    let fields: Vec<&str> = date.split_whitespace().collect();
    assert_eq!(fields.len(), 3, "fecha inesperada: {stamp}");
    let day = fields[0].parse::<u32>().expect("día");
    let month = match fields[1] {
        "ene" => 1,
        "feb" => 2,
        "mar" => 3,
        "abr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "ago" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dic" => 12,
        other => panic!("mes desconocido: {other}"),
    };
    let year = fields[2].parse::<i32>().expect("año");
    let parts: Vec<u32> = clock
        .trim()
        .split(':')
        .map(|part| part.parse::<u32>().expect("hora"))
        .collect();
    assert_eq!(parts.len(), 3, "hora inesperada: {stamp}");
    let wall = NaiveDate::from_ymd_opt(year, month, day)
        .expect("fecha válida")
        .and_hms_opt(parts[0], parts[1], parts[2])
        .expect("hora válida");
    (Utc.from_utc_datetime(&wall).timestamp() - t.trunc() as i64) as i32
}

fn role_of(text: &str) -> Role {
    match text {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => Role::System,
    }
}

fn message_of(case: &FixtureCase) -> ChatMessage {
    let object = case.input.as_object().expect("input del caso");
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map_or(Role::System, role_of);
    let streaming = object
        .get("streaming")
        .is_some_and(ez_core::pyvalue::truthy);
    ChatMessage {
        id: 1,
        role,
        text: String::new(),
        source: Source::Chat,
        t: object.get("t").and_then(Value::as_f64).expect("t del caso"),
        streaming,
        error: false,
    }
}

fn expected_stamp(case: &FixtureCase) -> &str {
    case.output
        .get("stamp")
        .and_then(Value::as_str)
        .expect("stamp esperado")
}

#[test]
fn every_chat_trace_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat_trace legibles");
    let mut cases_run = 0;
    let mut offset: Option<i32> = None;
    for case in &cases {
        if case.error.is_some() {
            consumes_type_boundary(case);
            cases_run += 1;
            continue;
        }
        let expected = expected_stamp(case);
        let t = case.input.get("t").and_then(Value::as_f64).expect("t");
        if !expected.is_empty() {
            let found = oracle_offset(t, expected);
            match offset {
                Some(known) => assert_eq!(found, known, "caso {}: desfase distinto", case.case),
                None => offset = Some(found),
            }
        }
        let zone = offset.expect("algún caso con hora");
        let produced = match case.function.as_str() {
            "stamp" => stamp_with_offset(t, zone),
            "stamp_for" => stamp_for_with_offset(&message_of(case), zone),
            other => panic!("función desconocida en {}: {other}", case.case),
        };
        assert_eq!(produced, expected, "caso {}", case.case);
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 54, "muy pocos casos: {cases_in_file}");
}
