#![recursion_limit = "512"]
//! Paridad de `hud/menu.py` contra `parity/fixtures/hud_menu.jsonl` (P-HUD-B).
//!
//! Un solo `#[test]`: los casos pintan y comparten la paleta global de tokens,
//! así que se ejecutan en serie.

#[path = "hud_menu_parity/frames.rs"]
mod frames;
#[path = "hud_menu_parity/geometry.rs"]
mod geometry;
#[path = "hud_menu_parity/replay.rs"]
mod replay;
#[path = "hud_menu_parity/sequence.rs"]
mod sequence;
#[path = "hud_menu_parity/support.rs"]
mod support;

use std::path::Path;

use ez_parity::fixtures;

const FIXTURES: &str = "../../../parity/fixtures/hud_menu.jsonl";
const CASE_COUNT: usize = 323;

#[test]
fn every_menu_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de menú legibles");
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "Menu.draw" => frames::menu_frame(case),
            "settings_window" => frames::settings_frame(case),
            "Menu.input" => sequence::run(case),
            other => geometry::run(case, other),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "se saltó algún caso de {FIXTURES}"
    );
    assert!(cases_in_file >= CASE_COUNT, "faltan casos en {FIXTURES}");
    let failures = frames::take_failures();
    assert!(
        failures.is_empty(),
        "{} cuadros fuera de umbral:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
