//! Paridad de `resolve_safe` contra `parity/fixtures/agent_paths.jsonl` (F2-04).
//!
//! Los fixtures marcan la carpeta temporal del oráculo como `<tmp>` y la raíz
//! de la unidad como `<anchor>`; aquí se sustituyen por la carpeta temporal
//! real, con el mismo entorno que fija `parity/gen/_common.py`.

use std::path::{Component, Path, PathBuf};

use ez_core::paths::{resolve_safe_with, Env};
use ez_parity::fixtures;
use serde_json::Value;

fn text<'value>(value: &'value Value, key: &str) -> &'value str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

fn mark(value: &str, tmp: &str, anchor: &str) -> String {
    value.replace("<tmp>", tmp).replace("<anchor>", anchor)
}

fn collapse(value: &str, tmp: &str, anchor: &str) -> String {
    value.replace(tmp, "<tmp>").replace(anchor, "<anchor>")
}

fn list(value: &Value, key: &str, tmp: &str, anchor: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| mark(item, tmp, anchor))
                .collect()
        })
        .unwrap_or_default()
}

fn drive_anchor(root: &Path) -> String {
    match root.components().next() {
        Some(Component::Prefix(prefix)) => {
            format!("{}{}", prefix.as_os_str().to_string_lossy(), '\\')
        }
        _ => "/".to_string(),
    }
}

#[test]
fn every_resolve_safe_case_matches_python() {
    let path = Path::new("../../../parity/fixtures/agent_paths.jsonl");
    let cases = fixtures::load(path).expect("fixtures de agent_paths legibles");
    // El oráculo pone HOME, USERPROFILE y APPDATA en la misma carpeta temporal.
    let tmp_root = std::env::temp_dir().join("ezagent-paths-parity");
    let tmp = tmp_root.display().to_string();
    let anchor = drive_anchor(&tmp_root);
    let env = Env::new(&[
        ("HOME", tmp.as_str()),
        ("USERPROFILE", tmp.as_str()),
        ("APPDATA", tmp.as_str()),
        ("LOCALAPPDATA", tmp.as_str()),
    ]);
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(
            case.function, "resolve_safe",
            "función inesperada en {}",
            case.case
        );
        let raw = mark(text(&case.input, "raw"), &tmp, &anchor);
        let base = PathBuf::from(mark(text(&case.input, "base"), &tmp, &anchor));
        let roots = list(&case.input, "roots", &tmp, &anchor);
        let deny = list(&case.input, "deny", &tmp, &anchor);
        let globs = list(&case.input, "deny_globs", &tmp, &anchor);
        let produced = resolve_safe_with(&env, &raw, &base, &roots, &deny, &globs);
        match (&case.error, produced) {
            (Some(expected), Err(error)) => {
                assert_eq!(error.message(), expected.as_str(), "caso {}", case.case);
            }
            (None, Ok(resolved)) => {
                let mut actual = collapse(&resolved.display().to_string(), &tmp, &anchor);
                let mut expected = text(&case.output, "path").to_string();
                if !cfg!(windows) {
                    actual = actual.replace('\\', "/");
                    expected = expected.replace('\\', "/");
                }
                assert_eq!(actual, expected, "caso {}", case.case);
            }
            (None, Err(error)) => {
                panic!("caso {}: error inesperado «{}»", case.case, error.message())
            }
            (Some(expected), Ok(resolved)) => panic!(
                "caso {}: se esperaba «{}» y se resolvió {}",
                case.case,
                expected,
                resolved.display()
            ),
        }
        cases_run += 1;
    }
    let cases_in_file = cases
        .iter()
        .filter(|case| case.function == "resolve_safe")
        .count();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}
