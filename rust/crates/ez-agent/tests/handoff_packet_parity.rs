//! Paridad de `handoff_packet.py` contra `parity/fixtures/handoff_packet.jsonl` (P-AGENT-B).
//!
//! Funciones puras: los dobles de `select` y `save` salen del propio fixture.

use std::cell::RefCell;
use std::path::Path;

use ez_agent::handoff_packet::{build_packet, has_work, outline, packet_text, Packet};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

/// Los casos con `select='raise'` esperan el mismo resultado que sin modelo:
/// el `except` del Python convierte la excepción en `None`, que es lo que
/// devuelve este doble (la firma Rust no puede lanzar).
type SelectDouble = Box<dyn Fn(&[String]) -> Option<i64>>;
type SaveDouble<'a> = Box<dyn Fn(&str) -> Option<String> + 'a>;

fn select_for(spec: &Value) -> SelectDouble {
    match spec {
        Value::Number(number) => {
            let start = number.as_i64().unwrap_or(0);
            Box::new(move |_| Some(start))
        }
        Value::String(name) => match name.as_str() {
            "zero" => Box::new(|_| Some(0)),
            "negative" => Box::new(|_| Some(-1)),
            "out_of_range" => Box::new(|_| Some(i64::MAX)),
            "len" => Box::new(|lines: &[String]| Some(lines.len() as i64)),
            // float, str, none_value y raise: `isinstance(start, int)` falla.
            _ => Box::new(|_| None),
        },
        _ => Box::new(|_| None),
    }
}

/// `save`: "path" guarda y devuelve la ruta del oráculo; "raise" propaga el
/// error como un pánico (el `catch_unwind` de la prueba lo comprueba).
fn save_for<'a>(spec: &Value, calls: &'a RefCell<Vec<String>>) -> SaveDouble<'a> {
    match spec.as_str() {
        Some("path") => Box::new(move |text| {
            calls.borrow_mut().push(text.to_string());
            Some("C:/tmp/registro.md".to_string())
        }),
        Some("raise") => Box::new(move |text| {
            calls.borrow_mut().push(text.to_string());
            panic!("disco lleno")
        }),
        _ => Box::new(|_| None),
    }
}

fn string<'value>(value: &'value Value, key: &str) -> &'value str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

fn number(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("falta el entero {key}")) as usize
}

fn compare_build(case: &FixtureCase) {
    let answer = string(&case.input, "answer");
    let calls = RefCell::new(Vec::new());
    let select_closure = select_for(&case.input["select"]);
    let save_closure = save_for(&case.input["save"], &calls);
    let select_active = !matches!(case.input["select"].as_str(), Some("none"));
    let save_active = !matches!(case.input["save"].as_str(), Some("none"));
    let select = select_active.then_some(&select_closure as &dyn Fn(&[String]) -> Option<i64>);
    let save = save_active.then_some(&save_closure as &dyn Fn(&str) -> Option<String>);
    let produced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_packet(answer, select, save)
    }));
    if case.error.is_some() {
        assert!(
            produced.is_err(),
            "caso {}: el `save` que lanza no se propagó",
            case.case
        );
        return;
    }
    let packet = produced.unwrap_or_else(|_| panic!("caso {}: pánico inesperado", case.case));
    let expected = &case.output;
    assert_eq!(packet.kind, string(expected, "kind"), "caso {}", case.case);
    assert_eq!(packet.text, string(expected, "text"), "caso {}", case.case);
    assert_eq!(
        packet.spoken,
        string(expected, "spoken"),
        "caso {}",
        case.case
    );
    assert_eq!(packet.full, string(expected, "full"), "caso {}", case.case);
    assert_eq!(
        packet.saved_path,
        string(expected, "saved_path"),
        "caso {}",
        case.case
    );
    let recorded: Vec<Value> = calls.borrow().iter().map(|text| json!(text)).collect();
    assert_eq!(
        Value::Array(recorded),
        expected["saved_calls"],
        "caso {}: llamadas a save",
        case.case
    );
}

fn compare_outline(case: &FixtureCase) {
    if case.error.is_some() {
        // `text.splitlines()` de `None` no tiene equivalente en la firma Rust.
        assert!(
            !case.input["text"].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    let produced = outline(string(&case.input, "text"), number(&case.input, "limit"));
    assert_eq!(produced, string(&case.output, "text"), "caso {}", case.case);
}

fn compare_packet_text(case: &FixtureCase) {
    if case.error.is_some() {
        assert!(
            case.input.get("kind").is_none(),
            "caso {}: error inesperado con un paquete",
            case.case
        );
        return;
    }
    let packet = Packet {
        kind: string(&case.input, "kind").to_string(),
        text: string(&case.input, "text").to_string(),
        spoken: string(&case.input, "spoken").to_string(),
        full: string(&case.input, "full").to_string(),
        saved_path: string(&case.input, "saved_path").to_string(),
    };
    assert_eq!(
        packet_text(&packet),
        string(&case.output, "text"),
        "caso {}",
        case.case
    );
}

fn compare_has_work(case: &FixtureCase) {
    if case.error.is_some() {
        assert!(
            !case.input["text"].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    assert_eq!(
        has_work(string(&case.input, "text")),
        case.output.as_bool().expect("booleano esperado"),
        "caso {}",
        case.case
    );
}

#[test]
fn every_handoff_packet_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/handoff_packet.jsonl");
    let cases = fixtures::load(path).expect("fixtures de handoff_packet legibles");
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "build_packet" => compare_build(case),
            "outline" => compare_outline(case),
            "packet_text" => compare_packet_text(case),
            "has_work" => compare_has_work(case),
            other => panic!("función desconocida: {other}"),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 129, "muy pocos casos: {cases_in_file}");
}
