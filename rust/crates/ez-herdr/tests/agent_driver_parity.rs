//! Paridad de `agent_driver.py`, `agent_session.py`, `agent_prelaunch.py` y
//! `watcher.py` contra `parity/fixtures/agent_driver.jsonl`.
//!
//! El oráculo sustituyó el reloj monotónico, `subprocess.run`, `shutil.which`,
//! los archivos de configuración y los hilos por dobles deterministas. Aquí se
//! inyectan los equivalentes; no se lanza ningún agente ni se toca la
//! configuración real del usuario. Un solo `#[test]` porque los casos tocan el
//! entorno del proceso y el directorio temporal.

#[path = "agent_driver_parity/cases.rs"]
mod cases;
#[path = "agent_driver_parity/common.rs"]
mod common;
#[path = "agent_driver_parity/driver.rs"]
mod driver;
#[path = "agent_driver_parity/fake.rs"]
mod fake;
#[path = "agent_driver_parity/prelaunch.rs"]
mod prelaunch;
#[path = "agent_driver_parity/session.rs"]
mod session;
#[path = "agent_driver_parity/watcher.rs"]
mod watcher;

use std::path::Path;

use common::{case_root, cleanup, first_diff, json_eq, set_env, unmark, ENV_NAMES};
use ez_parity::fixtures;

const FIXTURES: &str = "../../../parity/fixtures/agent_driver.jsonl";

#[test]
fn every_agent_driver_case_matches_python() {
    let fixtures_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    let cases = fixtures::load(&fixtures_path).expect("fixtures legibles");
    let home = std::env::temp_dir().join(format!("ez-agent-driver-parity-{}", std::process::id()));
    cleanup(&home);
    std::fs::create_dir_all(&home).expect("carpeta temporal");
    let home_text = home.display().to_string();
    let saved: Vec<_> = ENV_NAMES
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let root = case_root(&home, &case.case);
        std::fs::create_dir_all(&root).expect("carpeta del caso");
        let overrides = case.input.get("env").cloned();
        set_env(&root, &home, overrides.as_ref());
        let got = cases::run_case(case, &root);
        cases_run += 1;
        let expected = unmark(&case.output, &home_text);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => json_eq(value, &expected),
            (Err(message), Some(error)) => message == error,
            _ => false,
        };
        if !ok {
            let detail = match &got {
                Ok(value) => first_diff(value, &expected),
                Err(message) => format!("error inesperado: {message}"),
            };
            failures.push(format!("{}: {detail}", case.case));
        }
    }
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    cleanup(&home);
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(cases_run, cases.len());
    assert!(cases.len() >= 420, "casos: {}", cases.len());
}
