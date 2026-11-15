//! Paridad de `hud_geometry` contra `parity/fixtures/hud_geometry.jsonl` (P-HUD-A).
//!
//! Un solo `#[test]`: `tokens.set_themed` cambia la paleta global del proceso.

mod common;

use std::path::Path;

use common::fixtures;
use common::json::mark;
use ez_hud::tokens;

const FIXTURES: &str = "../../../parity/fixtures/hud_geometry.jsonl";

fn tolerance(function: &str) -> f64 {
    if function.contains("rounded_coverage") || function.contains("rounded_shadow") {
        1e-5
    } else {
        1e-9
    }
}

#[test]
fn every_hud_geometry_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures legibles");
    tokens::set_themed(None);
    let mut cases_run = 0;
    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let got = common::geometry::run(case);
        if !mark(
            &got,
            &case.output,
            case.error.as_ref(),
            tolerance(&case.function),
        ) {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
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
    assert!(cases_in_file >= 438, "solo {cases_in_file} casos");
    assert_eq!(cases_in_file, 438);
}
