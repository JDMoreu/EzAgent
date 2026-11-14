//! Paridad de junctions contra `parity/fixtures/paths_junction.jsonl` (F2-04).
//!
//! `C:\Users\All Users` es un junction a `C:\ProgramData`: `Path.resolve` lo
//! sigue y el port debe denegar la ruta. Si la máquina no tiene el junction, el
//! fixture solo trae el caso de presencia y aquí solo se comprueba ese.

use std::path::{Path, PathBuf};

use ez_core::paths::resolve_safe;
use ez_parity::fixtures;
use serde_json::Value;

fn text<'value>(value: &'value Value, key: &str) -> &'value str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn posix(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn every_junction_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/paths_junction.jsonl");
    let cases = fixtures::load(path).expect("fixtures de paths_junction legibles");
    let presence = cases
        .iter()
        .find(|case| case.function == "junction_present")
        .expect("caso junction_present");
    let junction = PathBuf::from(text(&presence.input, "path"));
    let present = presence
        .output
        .get("exists")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("falta exists en {}", presence.case));
    assert_eq!(junction.exists(), present, "caso {}", presence.case);
    let mut cases_run = 1;
    if !present {
        assert_eq!(cases_run, 1, "sin junction solo se comprueba ese caso");
        return;
    }
    let real = text(&presence.output, "real");
    let base = Path::new("C:/Users"); // parity/gen/paths_junction.py usa esta base
    let resolved = resolve_safe(
        text(&presence.input, "path"),
        base,
        &[text(&presence.input, "path")],
        &[] as &[&str],
        &[] as &[&str],
    )
    .expect("el propio junction está permitido");
    assert_eq!(posix(&resolved), real, "caso {}", presence.case);
    for case in &cases {
        if case.function != "resolve_safe" {
            continue;
        }
        let roots = strings(&case.input, "roots");
        let produced = resolve_safe(
            text(&case.input, "raw"),
            base,
            &roots,
            &[] as &[&str],
            &[] as &[&str],
        );
        match (
            case.output
                .get("ok")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            produced,
        ) {
            (false, Err(error)) => {
                assert_eq!(
                    error.message(),
                    text(&case.output, "error"),
                    "caso {}",
                    case.case
                );
            }
            (true, Ok(resolved)) => {
                assert_eq!(
                    posix(&resolved),
                    text(&case.output, "path"),
                    "caso {}",
                    case.case
                );
            }
            (true, Err(error)) => {
                panic!("caso {}: error inesperado «{}»", case.case, error.message())
            }
            (false, Ok(resolved)) => panic!(
                "caso {}: se esperaba «{}» y se resolvió {}",
                case.case,
                text(&case.output, "error"),
                resolved.display()
            ),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}
