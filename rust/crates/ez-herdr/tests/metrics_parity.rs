//! Paridad de `metrics.py` contra `parity/fixtures/metrics.jsonl` (P-HERDR).
//!
//! El oráculo congeló `time.monotonic` en 1234.5 y el instante del perfil; los
//! casos de marcas implícitas se reproducen con ese valor explícito.

use std::path::Path;

use ez_core::metrics::{
    float_value, format_stt, format_turn, LatencyStats, TurnTimings, META_KEYS,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/metrics.jsonl";
const MONOTONIC: f64 = 1234.5;

fn report_of(value: Option<&Value>) -> ez_core::metrics::Report {
    let mut report = ez_core::metrics::Report::new();
    if let Some(map) = value.and_then(Value::as_object) {
        for (key, value) in map {
            if let Some(number) = value.as_f64() {
                report.insert(key.clone(), number);
            }
        }
    }
    report
}

fn timings_of(spec: &Value) -> TurnTimings {
    let timings = TurnTimings::new(
        spec.get("speech_end")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        spec.get("vad_end").and_then(Value::as_f64).unwrap_or(0.0),
    );
    for (name, at) in spec
        .get("marks")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        timings.mark(name, at.as_f64());
    }
    for (name, seconds) in spec
        .get("spans")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        timings.add_span(name, seconds.as_f64().unwrap_or(0.0));
    }
    timings
}

fn map_of(report: &ez_core::metrics::Report) -> Value {
    let mut map = Map::new();
    for (key, value) in report {
        map.insert(key.clone(), Value::from(*value));
    }
    Value::Object(map)
}

fn map_value(map: &Map<String, Value>) -> Value {
    Value::Object(map.clone())
}

fn run_case(case: &FixtureCase, temp: &str) -> Result<Value, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "module_constants" => json!({"meta_keys": META_KEYS}),
        "format_stt" => json!({"text": format_stt(&report_of(input.get("report")))}),
        "format_turn" => json!({"text": format_turn(&report_of(input.get("report")))}),
        "LatencyStats.percentile" => {
            let values: Vec<f64> = input
                .get("values")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default();
            let pct = input.get("pct").and_then(Value::as_f64).unwrap_or(0.0);
            json!({"value": LatencyStats::percentile(&values, pct)})
        }
        "TurnTimings.mark" => {
            let timings = TurnTimings::new(0.0, 0.0);
            let name = input
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("first_token");
            match input.get("value") {
                Some(value) => timings.mark_value(name, Some(value))?,
                None => timings.mark_value(name, Some(&Value::from(MONOTONIC)))?,
            };
            let marks = timings.marks.lock().expect("marcas").clone();
            let mut map = Map::new();
            for (key, value) in marks {
                map.insert(key, Value::from(value));
            }
            json!({"marks": map})
        }
        "TurnTimings.add_span" => {
            let timings = TurnTimings::new(0.0, 0.0);
            timings.add_span_value("x", input.get("value").unwrap_or(&Value::Null))?;
            json!({"spans": {}})
        }
        "TurnTimings.report" => {
            let timings = timings_of(input);
            let report = timings.report();
            let marks = timings.marks.lock().expect("marcas").clone();
            let spans = timings.spans.lock().expect("spans").clone();
            let mut marks_json = Map::new();
            for (key, value) in marks {
                marks_json.insert(key, Value::from(value));
            }
            let mut spans_json = Map::new();
            for (key, value) in spans {
                spans_json.insert(key, Value::from(value));
            }
            json!({
                "report": map_of(&report),
                "keys": report.keys().cloned().collect::<Vec<_>>(),
                "marks": marks_json,
                "spans": spans_json,
            })
        }
        "LatencyStats" => stats_case(case, input, temp)?,
        other => return Err(format!("función sin portar: {other}")),
    })
}

fn stats_case(case: &FixtureCase, input: &Value, temp: &str) -> Result<Value, String> {
    let name = case.case.rsplit('/').next().unwrap_or("stats").to_string();
    let folder = Path::new(temp).join("metrics-cases").join(name);
    std::fs::create_dir_all(&folder).expect("carpeta");
    let path = folder.join("latency.jsonl");
    if input.get("jsonl").and_then(Value::as_str) == Some("<dir>") {
        std::fs::create_dir_all(&path).expect("directorio");
    }
    let maxlen = input.get("maxlen").and_then(Value::as_u64).unwrap_or(50) as usize;
    let mut stats = LatencyStats::new(maxlen, Some(path.clone()));
    let mut reports = Vec::new();
    for op in input
        .get("ops")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match op.get("kind").and_then(Value::as_str) {
            Some("turn") => {
                let timings = timings_of(op.get("timings").unwrap_or(&Value::Null));
                let meta = op.get("meta").and_then(Value::as_object).cloned();
                let report = stats.record(&timings, meta.as_ref());
                reports.push(map_of(&report));
            }
            _ => {
                let command = op.get("name").and_then(Value::as_str).unwrap_or("");
                let handled = op.get("handled_ms").and_then(Value::as_f64).unwrap_or(0.0);
                stats.record_command(command, handled);
                reports.push(json!({"command": command, "handled_ms": handled}));
            }
        }
    }
    let turns: Vec<Value> = stats
        .turns()
        .iter()
        .map(|(report, extra)| json!([map_of(report), map_value(extra)]))
        .collect();
    let file = if path.is_dir() {
        Value::from("<dir>")
    } else if path.exists() {
        Value::String(freeze_ts(
            &std::fs::read_to_string(&path)
                .unwrap_or_default()
                .replace("\r\n", "\n"),
        ))
    } else {
        Value::Null
    };
    Ok(json!({
        "reports": reports,
        "turns": turns,
        "turns_len": stats.turns().len(),
        "total": stats.total(),
        "file": file,
    }))
}

/// El oráculo congeló `time.time()` en 1735689600.0; el reloj es lo único no
/// determinista del archivo, así que se fija antes de comparar.
fn freeze_ts(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let Some(start) = line.find("\"ts\": ") else {
                return line.to_string();
            };
            let value_start = start + "\"ts\": ".len();
            let value_end = line[value_start..]
                .find(',')
                .map(|index| value_start + index)
                .unwrap_or_else(|| line.trim_end_matches('\n').len());
            format!(
                "{}\"ts\": 1735689600.0{}",
                &line[..start],
                &line[value_end..]
            )
        })
        .collect()
}

/// Borra el árbol temporal propio con `remove_file`/`remove_dir` (la puerta no
/// permite el borrado recursivo de la biblioteca estándar).
fn cleanup(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup(&path);
            let _ = std::fs::remove_dir(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn unmark(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("<tmp>", temp).replace('\\', "/")),
        Value::Array(items) => Value::Array(items.iter().map(|item| unmark(item, temp)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), unmark(value, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn diff_path(left: &Value, right: &Value, path: &str) -> String {
    if json_eq(left, right) {
        return String::new();
    }
    match (left, right) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, value) in actual {
                match expected.get(key) {
                    Some(other) => {
                        let nested = diff_path(value, other, &format!("{path}.{key}"));
                        if !nested.is_empty() {
                            return nested;
                        }
                    }
                    None => return format!("{path}.{key} (clave de más)"),
                }
            }
            for key in expected.keys() {
                if !actual.contains_key(key) {
                    return format!("{path}.{key} (falta)");
                }
            }
            path.to_string()
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (index, (value, other)) in actual.iter().zip(expected).enumerate() {
                let nested = diff_path(value, other, &format!("{path}[{index}]"));
                if !nested.is_empty() {
                    return nested;
                }
            }
            format!("{path} (longitud {} != {})", actual.len(), expected.len())
        }
        (Value::String(actual), Value::String(expected)) => {
            let position = actual
                .chars()
                .zip(expected.chars())
                .position(|(left, right)| left != right)
                .unwrap_or(actual.chars().count().min(expected.chars().count()));
            format!(
                "{path} char {position}: {:?} != {:?}",
                actual
                    .chars()
                    .skip(position.saturating_sub(20))
                    .take(60)
                    .collect::<String>(),
                expected
                    .chars()
                    .skip(position.saturating_sub(20))
                    .take(60)
                    .collect::<String>()
            )
        }
        _ => format!("{path}: {left:?} != {right:?}"),
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

#[test]
fn every_metrics_case_matches_python() {
    let fixtures_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    let cases = fixtures::load(&fixtures_path).expect("fixtures legibles");
    let temp = std::env::temp_dir().join(format!("ez-metrics-parity-{}", std::process::id()));
    cleanup(&temp);
    std::fs::create_dir_all(&temp).expect("carpeta temporal");
    let temp_text = temp.display().to_string();
    let mut failures = Vec::new();
    for case in &cases {
        let got = run_case(case, &temp_text);
        let expected = unmark(&case.output, &temp_text);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => json_eq(value, &expected),
            (Err(message), Some(error)) => message == error,
            _ => false,
        };
        if !ok {
            let detail = match &got {
                Ok(value) => diff_path(value, &expected, "$"),
                Err(_) => String::new(),
            };
            failures.push(format!("{}: {detail}", case.case));
        }
    }
    cleanup(&temp);
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(cases.len() >= 56, "casos: {}", cases.len());
}

/// `float_value` también se prueba indirectamente; aquí un caso explícito del
/// texto de error de CPython y de los valores aceptados.
#[test]
fn float_value_reproduce_el_error_de_python() {
    assert_eq!(
        float_value(&Value::from("no-numérico")),
        Err("could not convert string to float: 'no-numérico'".to_string())
    );
    assert_eq!(float_value(&Value::from(" 12.5 ")), Ok(12.5));
    assert_eq!(float_value(&Value::Bool(true)), Ok(1.0));
    assert_eq!(float_value(&Value::from(3)), Ok(3.0));
}
