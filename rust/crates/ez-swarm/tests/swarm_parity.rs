//! Paridad de `ez-swarm` contra `parity/fixtures/swarm.jsonl`.
//!
//! Un solo `#[test]`: los casos del CLI tocan el entorno del proceso y varios
//! casos comparten directorio temporal. Los casos se ejecutan en orden y se
//! recogen todos los fallos antes de fallar, para que el informe tenga números.

#[path = "swarm_parity/board.rs"]
mod board;
#[path = "swarm_parity/flow.rs"]
mod flow;
#[path = "swarm_parity/simple.rs"]
mod simple;
#[path = "swarm_parity/support.rs"]
mod support;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use ez_parity::fixtures::{self, FixtureCase};
use serde_json::Value;

use support::TempHome;

const FIXTURES: &str = "../../../parity/fixtures/swarm.jsonl";
const CASES_IN_FILE: usize = 644;

fn run_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    match case.function.as_str() {
        function if function.starts_with("protocol.") => simple::protocol_case(case),
        function if function.starts_with("models.") => simple::models_case(case),
        "progress.check" => simple::progress_case(case),
        function if function.starts_with("incidents.") => simple::incidents_case(case),
        function if function.starts_with("store.") => board::store_case(case, home),
        function if function.starts_with("view.") => board::view_case(case, home),
        "service.handle" => flow::service_case(case, home),
        "recovery.resolve" => flow::recovery_case(case, home),
        "cli.endpoint_path" | "cli._read_port" => {
            flow::with_frozen_profile(home, || flow::cli_case(case, home))
        }
        "cli.main" => flow::with_frozen_profile(home, || flow::cli_case(case, home)),
        other => Err(format!("caso sin arnés: {other}")),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
        })
        .unwrap_or_else(|| "pánico sin mensaje".to_string())
}

fn show(value: &Value) -> String {
    ez_core::pyjson::dumps_with(
        value,
        &ez_core::pyjson::DumpOptions {
            sort_keys: true,
            ..ez_core::pyjson::DumpOptions::default().with_indent(2)
        },
    )
}

#[test]
fn every_swarm_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures del enjambre legibles");
    let home = TempHome::new();
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        let produced = catch_unwind(AssertUnwindSafe(|| run_case(case, &home)))
            .unwrap_or_else(|payload| Err(format!("pánico: {}", panic_message(&*payload))));
        match (&case.error, produced) {
            (Some(expected), Err(actual)) if *expected == actual => {}
            (Some(expected), Err(actual)) => failures.push(format!(
                "{}: error distinto\n  esperado: {expected}\n  obtenido: {actual}",
                case.case
            )),
            (Some(expected), Ok(value)) => failures.push(format!(
                "{}: el oráculo espera el error {expected:?} y el Rust devolvió {}",
                case.case,
                show(&value)
            )),
            (None, Err(actual)) => {
                failures.push(format!("{}: error inesperado: {actual}", case.case));
            }
            (None, Ok(value)) if value == case.output => {}
            (None, Ok(value)) => {
                let differs = support::diff_json(&case.output, &value, "salida");
                failures.push(format!(
                    "{}: salida distinta\n  {}",
                    case.case,
                    differs.join("\n  ")
                ));
            }
        }
    }
    assert!(cases.len() >= CASES_IN_FILE, "faltan casos en el fixture");
    assert_eq!(cases_run, cases.len(), "no se consumieron todos los casos");
    if !failures.is_empty() {
        panic!(
            "{} de {} casos fallan:\n\n{}",
            failures.len(),
            cases_run,
            failures.join("\n\n")
        );
    }
}
