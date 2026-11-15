//! Paridad de `herdr.py` contra `parity/fixtures/herdr.jsonl`.
//!
//! El oráculo sustituyó `subprocess.run`, `shutil.which`, el reloj monotónico y
//! `uuid.uuid4` por dobles deterministas. Aquí se inyecta el equivalente con
//! [`HerdrRuntime`]; no se ejecuta `hc` ni `herdr-app` reales. Un solo `#[test]`
//! porque los casos tocan el entorno del proceso.

#[path = "herdr_parity/cases.rs"]
mod cases;
#[path = "herdr_parity/harness.rs"]
mod harness;
#[path = "herdr_parity/launch.rs"]
mod launch;

use std::path::Path;

use ez_parity::fixtures;
use harness::{cleanup, normalize_powershell};
use serde_json::Value;

const FIXTURES: &str = "../../../parity/fixtures/herdr.jsonl";

fn slice_chars(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn first_diff(left: &Value, right: &Value) -> String {
    if json_eq(left, right) {
        return String::new();
    }
    match (left, right) {
        (Value::String(actual), Value::String(expected)) => {
            let position = actual
                .chars()
                .zip(expected.chars())
                .position(|(left, right)| left != right)
                .unwrap_or(actual.chars().count().min(expected.chars().count()));
            let start = position.saturating_sub(60);
            format!(
                "texto@{position}: …{}… / …{}…",
                slice_chars(actual, start, position + 60),
                slice_chars(expected, start, position + 60)
            )
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (index, (left, right)) in actual.iter().zip(expected).enumerate() {
                if !json_eq(left, right) {
                    return format!("[{index}].{}", first_diff(left, right));
                }
            }
            format!("longitud {} != {}", actual.len(), expected.len())
        }
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, value) in actual {
                match expected.get(key) {
                    Some(other) if json_eq(value, other) => continue,
                    Some(other) => return format!("{key}.{}", first_diff(value, other)),
                    None => return format!("clave de más: {key}"),
                }
            }
            for key in expected.keys() {
                if !actual.contains_key(key) {
                    return format!("falta clave: {key}");
                }
            }
            "objetos distintos".to_string()
        }
        _ => format!("{left:?} != {right:?}"),
    }
}

fn unmark(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("<tmp>", temp)),
        Value::Array(items) => Value::Array(items.iter().map(|item| unmark(item, temp)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), unmark(value, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn json_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(actual), Value::Number(expected)) => {
            match (actual.as_f64(), expected.as_f64()) {
                (Some(actual), Some(expected)) => {
                    (actual - expected).abs() <= 1e-9 * actual.abs().max(expected.abs()).max(1.0)
                }
                _ => actual == expected,
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| json_eq(actual, expected))
        }
        (Value::Object(actual), Value::Object(expected)) => {
            actual.len() == expected.len()
                && actual.iter().all(|(key, value)| {
                    expected.get(key).is_some_and(|other| json_eq(value, other))
                })
        }
        _ => left == right,
    }
}

const ENV_NAMES: [&str; 8] = [
    "LOCALAPPDATA",
    "HOME",
    "HC_TERMINAL_ID",
    "hc_terminal_id",
    "Hc_Terminal_Id",
    "OTRA",
    "OPENCODE_CONFIG",
    "OPENCODE_CONFIG_DIR",
];

fn clear_env(temp: &str) {
    std::env::set_var("LOCALAPPDATA", temp);
    std::env::set_var("HOME", temp);
    for name in [
        "HC_TERMINAL_ID",
        "hc_terminal_id",
        "Hc_Terminal_Id",
        "OTRA",
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_DIR",
    ] {
        std::env::remove_var(name);
    }
}

#[test]
fn every_herdr_case_matches_python() {
    let fixtures_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    let cases = fixtures::load(&fixtures_path).expect("fixtures legibles");
    let base = std::env::temp_dir().join(format!("ez-herdr-parity-{}", std::process::id()));
    cleanup(&base);
    std::fs::create_dir_all(&base).expect("carpeta temporal");
    let saved: Vec<_> = ENV_NAMES
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for (index, case) in cases.iter().enumerate() {
        let temp = base.join(format!("c{index:03}"));
        std::fs::create_dir_all(&temp).expect("caso");
        let temp_text = temp.display().to_string();
        clear_env(&temp_text);
        let got = cases::run_case(case, &temp_text).map(|value| normalize_powershell(&value));
        cases_run += 1;
        let expected = unmark(&case.output, &temp_text);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => json_eq(value, &expected),
            (Err(message), Some(error)) => message == error,
            _ => false,
        };
        if !ok {
            let detail = match &got {
                Ok(value) => first_diff(value, &expected),
                Err(_) => String::new(),
            };
            failures.push(format!("{}: {detail}", case.case));
        }
    }
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    cleanup(&base);
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(cases_run, cases.len());
    assert!(cases.len() >= 177, "casos: {}", cases.len());
}
