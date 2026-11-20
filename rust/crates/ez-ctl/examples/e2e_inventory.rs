//! End-to-end test of the inventory, usage, recommendation and resource guard
//! (`docs/designs/inventario-recursos.md`, brief IR-4).
//!
//! Everything runs with temporary user folders and fake agents. The real `hc`
//! only opens test terminals with unique names; they are closed even when a
//! check fails. No model is called and no real usage data is read.
//!
//! Build the binaries first and then run the example:
//!
//! ```text
//! cargo build -p ez-ctl --bin ez -p ez-fakes --bin fake-ws-agent
//! cargo run -p ez-ctl --example e2e_inventory
//! ```
//!
//! Exit codes: 0 when every check passed, 1 when one failed, 2 when a binary
//! or `hc` is missing.

// DECISION: the modules live in `examples/e2e_inventory/`; the root of the
// example is `examples/e2e_inventory.rs`, so each module says where it is.
#[path = "e2e_inventory/checks.rs"]
mod checks;
#[path = "e2e_inventory/cleanup.rs"]
mod cleanup;
#[path = "e2e_inventory/delegate.rs"]
mod delegate;
#[path = "e2e_inventory/depleted.rs"]
mod depleted;
#[path = "e2e_inventory/fixtures.rs"]
mod fixtures;
#[path = "e2e_inventory/guard.rs"]
mod guard;
#[path = "e2e_inventory/init.rs"]
mod init;
#[path = "e2e_inventory/out.rs"]
mod out;
#[path = "e2e_inventory/parent.rs"]
mod parent;
#[path = "e2e_inventory/proc.rs"]
mod proc;
#[path = "e2e_inventory/recommend.rs"]
mod recommend;
#[path = "e2e_inventory/run.rs"]
mod run;
#[path = "e2e_inventory/setup.rs"]
mod setup;
#[path = "e2e_inventory/usage.rs"]
mod usage;
#[path = "e2e_inventory/watch.rs"]
mod watch;

use std::process::ExitCode;

use checks::Report;
use parent::Parent;
use serde_json::json;

fn main() -> ExitCode {
    ExitCode::from(run_test() as u8)
}

/// The complete test.
fn run_test() -> i32 {
    let sandbox = match setup::prepare() {
        Ok(sandbox) => sandbox,
        Err(message) => {
            out::error(&format!("[EzAgent] la prueba no puede empezar: {message}"));
            return 2;
        }
    };
    out::line(&format!("[EzAgent] prueba {}", sandbox.id));
    let mut report = Report::default();
    init::run(&sandbox, &mut report);
    usage::run(&sandbox, &mut report);
    let parent = parent::open(&sandbox, &mut report);
    if let Some(terminal) = parent.as_ref() {
        recommend::run(&sandbox, &terminal.address, &mut report);
        delegate::run(&sandbox, &terminal.address, &mut report);
        guard::run(&sandbox, &terminal.address, &mut report);
        watch::run(&sandbox, &terminal.address, &mut report);
        depleted::run(&sandbox, &terminal.address, &mut report);
    }
    report.extra.insert("id".to_string(), json!(sandbox.id));
    report.extra.insert(
        "parent".to_string(),
        parent
            .as_ref()
            .map(parent::to_value)
            .unwrap_or(serde_json::Value::Null),
    );
    let parent_address = parent.as_ref().map(|Parent { address }| address.as_str());
    cleanup::run(&sandbox, parent_address, &mut report);
    let value = report.to_value();
    write_result(&sandbox, &value);
    out::line(&report.table());
    if report.ok() {
        0
    } else {
        1
    }
}

/// Writes the JSON with every check next to the temporary folder.
fn write_result(sandbox: &setup::Sandbox, value: &serde_json::Value) {
    let path = setup::result_path(sandbox);
    let body = match serde_json::to_string_pretty(value) {
        Ok(body) => body,
        Err(error) => {
            out::error(&format!("[EzAgent] no pude preparar el JSON: {error}"));
            return;
        }
    };
    match std::fs::write(&path, format!("{body}\n")) {
        Ok(()) => out::line(&format!("[EzAgent] resultado: {}", path.display())),
        Err(error) => out::error(&format!(
            "[EzAgent] no pude escribir {}: {error}",
            path.display()
        )),
    }
}
