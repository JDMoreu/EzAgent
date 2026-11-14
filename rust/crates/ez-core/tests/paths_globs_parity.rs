//! Paridad de `fnmatch` y `deny_globs` contra `parity/fixtures/paths_globs.jsonl` (F2-04).
//!
//! Los casos `fnmatch` comparan el booleano; los `resolve_safe` usan una
//! carpeta temporal como workspace, igual que `parity/gen/paths_globs.py`.

use std::path::Path;

use ez_core::paths::{fnmatch, resolve_safe};
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

fn boolean(value: &Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("falta el booleano {key}"))
}

#[test]
fn translated_classes_follow_fnmatch() {
    // Rangos invertidos y clases vacías no coinciden nunca en Python.
    assert!(!fnmatch("z", "[z-a]"));
    assert!(!fnmatch("a", "[!]"));
    // `]` como primer carácter de la clase sí es un literal.
    assert!(fnmatch("]", "[]a]"));
    assert!(fnmatch("a", "[]a]"));
    // `[!x]` es una clase negada, no un `^` fuera de la clase.
    assert!(fnmatch("a.env", "[!x]*.env"));
    assert!(!fnmatch("x.env", "[!x]*.env"));
}

#[test]
fn every_globs_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/paths_globs.jsonl");
    let cases = fixtures::load(path).expect("fixtures de paths_globs legibles");
    let workspace = std::env::temp_dir().join("ezagent-paths-globs").join("ws");
    std::fs::create_dir_all(workspace.join("a")).expect("carpeta de trabajo");
    let roots = vec![workspace.display().to_string()];
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "fnmatch" => {
                let produced = fnmatch(text(&case.input, "name"), text(&case.input, "pattern"));
                assert_eq!(
                    produced,
                    boolean(&case.output, "match"),
                    "caso {}",
                    case.case
                );
            }
            "resolve_safe" => {
                let globs = strings(&case.input, "deny_globs");
                let produced = resolve_safe(
                    text(&case.input, "raw"),
                    &workspace,
                    &roots,
                    &[] as &[&str],
                    &globs,
                );
                match (boolean(&case.output, "ok"), produced) {
                    (false, Err(error)) => {
                        assert_eq!(
                            error.message(),
                            text(&case.output, "error"),
                            "caso {}",
                            case.case
                        );
                    }
                    (true, Ok(resolved)) => {
                        let name = resolved
                            .file_name()
                            .map(|part| part.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        assert_eq!(name, text(&case.output, "name"), "caso {}", case.case);
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
            }
            other => panic!("función inesperada «{other}» en {}", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}
