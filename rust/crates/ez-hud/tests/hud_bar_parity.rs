//! Paridad de `hud_bar` contra `parity/fixtures/hud_bar.jsonl` y los PNG dorados.

mod common;

use std::path::{Path, PathBuf};

use common::fixtures;

const FIXTURES: &str = "../../../parity/fixtures/hud_bar.jsonl";
const GOLDEN: &str = "../../../parity/golden";
const THRESHOLDS: &str = "../../../parity/thresholds.toml";

#[test]
fn every_hud_bar_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures legibles");
    let thresholds =
        ez_parity::image::Thresholds::load(Path::new(THRESHOLDS)).expect("umbrales legibles");
    let golden = PathBuf::from(GOLDEN);
    // Cachés sensibles al primer uso (máscara de la forma, `round(escala·100)`):
    // se reproducen en el mismo orden que `parity/gen/hud_bar.py` — primero los
    // cuadros quietos, luego las transiciones en su orden de generación.
    let mut ordered: Vec<&common::FixtureCase> = cases.iter().collect();
    ordered.sort_by_key(|case| {
        let animated = !common::json::field(&case.input, "animation").is_null();
        if !animated {
            return (0_u8, 0_u8, 0_i64, String::new());
        }
        let theme = if case.case.contains("-light-") { 0 } else { 1 };
        let tag = if case.case.contains("-emerge-") {
            0
        } else if case.case.contains("-mute-") {
            1
        } else {
            2
        };
        let t_ms = case
            .case
            .rsplit_once('@')
            .and_then(|(_, tail)| tail.parse::<i64>().ok())
            .unwrap_or(0);
        (1_u8, theme, tag, format!("{t_ms:08}"))
    });
    let mut cases_run = 0;
    let mut failures: Vec<String> = Vec::new();
    for case in ordered {
        if let Err(message) = common::bar_cases::run(case, &golden, &thresholds) {
            failures.push(message);
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 361, "solo {cases_in_file} casos");
    assert_eq!(cases_in_file, 361);
}
