//! Secuencias de `Menu.input`.

use ez_parity::fixtures::FixtureCase;

use super::geometry;
use super::replay::run_sequence;
use super::support::*;

pub(super) fn run(case: &FixtureCase) {
    activate(case);
    let settings = settings_of(case);
    let recorder = Recorder::new(case, settings.saved_snapshot());
    let mut menu = build_menu(case, &recorder);
    let steps = run_sequence(case, &mut menu, &recorder);
    compare(
        case,
        o(&[("steps", a(&steps)), ("rects", geometry::geometry(&menu))]),
    );
}
