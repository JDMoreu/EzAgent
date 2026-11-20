//! The fake parent terminal, its harmless watcher placeholder and the seed of
//! the previous `developer`.
//!
//! The real `hc` creates the terminal. It uses no model and no window focus.
//!
//! DECISION: the name of the product watcher is occupied beforehand with a
//! fake agent. `hc run` does not pass the environment of the client, so a real
//! watcher would read the person's `voice.json`; the watcher is exercised
//! explicitly (and with the temporary environment) in step 6.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::checks::Report;
use super::fixtures;
use super::proc;
use super::run::{self, text};
use super::setup::{self, Sandbox};

/// The fake parent of the test.
pub struct Parent {
    /// The address of its `hc` terminal.
    pub address: String,
}

/// Opens the fake parent and prepares what depends on it.
pub fn open(sandbox: &Sandbox, report: &mut Report) -> Option<Parent> {
    let name = format!("{}-p", sandbox.id);
    let args = vec![
        "run".to_string(),
        "--json".to_string(),
        "--bg".to_string(),
        "--no-parent".to_string(),
        "--no-notify".to_string(),
        "--cwd".to_string(),
        setup::path_text(&sandbox.work),
        "--name".to_string(),
        name.clone(),
        "--label".to_string(),
        format!("ez.ir4={}", sandbox.id),
        "--".to_string(),
        setup::path_text(&sandbox.fake),
        "--echo".to_string(),
        setup::path_text(&sandbox.registros),
    ];
    let output = run::hc(sandbox, &args);
    let data = proc::json(&output).unwrap_or(Value::Null);
    let address = text(&data, "address");
    if !output.ok() || address.is_empty() {
        report.fail(
            "el padre falso se abre en hc",
            format!("código {}: {}", output.code, output.message()),
        );
        return None;
    }
    if let Err(message) = wait_ready(sandbox, &address) {
        report.fail("el padre falso queda vivo", message);
        return None;
    }
    report.pass(
        "el padre falso se abre en hc",
        format!("{address} ({name})"),
    );
    occupy_watcher(sandbox, &address, report);
    match fixtures::previous_developer(sandbox, &address) {
        Ok(()) => report.pass(
            "el registro del developer anterior está sembrado",
            format!("dev0-opencode con opencode-go/{}", setup::OPENCODE_GO_MODEL),
        ),
        Err(message) => report.fail("el registro del developer anterior", message),
    }
    Some(Parent { address })
}

/// Waits until `hc get` says live and the fake agent wrote its record.
fn wait_ready(sandbox: &Sandbox, address: &str) -> Result<(), String> {
    let record = sandbox.registros.join(format!("{address}.json"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let live = run::terminal(sandbox, address)
            .map(|data| text(&data, "state") == "live")
            .unwrap_or(false);
        if live && record.is_file() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "la terminal {address} no quedó viva con su registro ({})",
                record.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Occupies the watcher name so the product does not start a real one.
fn occupy_watcher(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let name = ez_herdr::subagents::watcher_name(parent);
    let args = vec![
        "run".to_string(),
        "--json".to_string(),
        "--bg".to_string(),
        "--no-parent".to_string(),
        "--no-notify".to_string(),
        "--cwd".to_string(),
        setup::path_text(&sandbox.work),
        "--name".to_string(),
        name.clone(),
        "--label".to_string(),
        format!("ez.watch={parent}"),
        "--".to_string(),
        setup::path_text(&sandbox.fake),
        "--echo".to_string(),
        setup::path_text(&sandbox.registros),
    ];
    let output = run::hc(sandbox, &args);
    let data = proc::json(&output).unwrap_or(Value::Null);
    let address = text(&data, "address");
    report.check(
        "el nombre del vigilante queda ocupado por la prueba",
        output.ok() && !address.is_empty(),
        if address.is_empty() {
            format!("código {}: {}", output.code, output.message())
        } else {
            format!("{address} ({name})")
        },
    );
}

/// The data of the parent for the result JSON.
pub fn to_value(parent: &Parent) -> Value {
    json!({"address": parent.address})
}
