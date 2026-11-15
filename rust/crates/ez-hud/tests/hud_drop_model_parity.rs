//! Paridad de `hud_drop_model` contra `parity/fixtures/hud_drop_model.jsonl`.

mod common;

use std::path::Path;

use common::fixtures;
use common::json::mark;

const FIXTURES: &str = "../../../parity/fixtures/hud_drop_model.jsonl";

#[test]
fn every_hud_drop_model_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures legibles");
    let mut cases_run = 0;
    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let got = common::drop::run(case);
        if !mark(&got, &case.output, case.error.as_ref(), 1e-9) {
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
    assert!(cases_in_file >= 253, "solo {cases_in_file} casos");
    assert_eq!(cases_in_file, 253);
}
