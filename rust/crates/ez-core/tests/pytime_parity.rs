//! Paridad de fechas contra `parity/fixtures/pytime.jsonl` (F2-01).

use ez_core::pytime::{
    date_local, isoformat_local, isoformat_millis_utc, isoformat_utc, timestamp,
};
use ez_parity::fixtures::{self, FixtureCase};

fn seconds(case: &FixtureCase) -> f64 {
    case.input
        .get("t")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| panic!("caso {}: falta `t`", case.case))
}

fn call(function: &str, secs: f64) -> Result<String, String> {
    match function {
        "isoformat_local" => isoformat_local(secs),
        "isoformat_utc" => isoformat_utc(secs),
        "isoformat_millis_utc" => isoformat_millis_utc(secs),
        "date_local" => date_local(secs),
        other => panic!("función desconocida en el fixture: {other}"),
    }
}

#[test]
fn every_case_matches_python() {
    let path = std::path::Path::new("../../../parity/fixtures/pytime.jsonl");
    let cases = fixtures::load(path).expect("fixtures de pytime legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.function == "timestamp_shape" {
            let now = timestamp();
            assert!(now.is_finite(), "caso {}", case.case);
            let clock = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("reloj después de 1970")
                .as_secs_f64();
            assert!(
                (now - clock).abs() < 300.0,
                "caso {}: timestamp() no sigue al reloj",
                case.case
            );
            cases_run += 1;
            continue;
        }
        let produced = call(&case.function, seconds(case));
        match case.error.as_deref() {
            Some(expected) => {
                let error = produced.expect_err("el fixture espera un error");
                assert_eq!(error, expected, "caso {}", case.case);
            }
            None => {
                let text = produced.unwrap_or_else(|error| panic!("caso {}: {error}", case.case));
                assert_eq!(
                    Some(text.as_str()),
                    case.output.as_str(),
                    "caso {}",
                    case.case
                );
            }
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}
