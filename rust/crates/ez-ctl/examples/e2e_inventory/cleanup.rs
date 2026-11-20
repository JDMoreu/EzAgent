//! Cleanup: every test terminal is killed and removed and the sandbox deleted.

use serde_json::Value;

use super::checks::Report;
use super::run::{self, text};
use super::setup::{self, Sandbox};

/// Closes the test terminals, removes the sandbox and confirms nothing is left.
pub fn run(sandbox: &Sandbox, parent: Option<&str>, report: &mut Report) {
    if let Some(parent) = parent {
        kill_label(sandbox, &format!("ez.parent={parent}"));
        kill_label(sandbox, &format!("ez.watch={parent}"));
    }
    kill_label(sandbox, &format!("ez.ir4={}", sandbox.id));
    let left = leftovers(sandbox, parent);
    report.check(
        "no quedan terminales de la prueba en hc",
        left.is_empty(),
        format!("{left:?}"),
    );
    setup::remove_root(sandbox);
    report.check(
        "la carpeta de la prueba se borra",
        !sandbox.root.exists(),
        setup::path_text(&sandbox.root),
    );
}

/// Kills and removes every terminal with a label.
fn kill_label(sandbox: &Sandbox, label: &str) {
    for terminal in run::terminals_with_label(sandbox, label) {
        let address = text(&terminal, "address");
        if address.is_empty() {
            continue;
        }
        let _ = run::hc(sandbox, &["kill".to_string(), address.clone()]);
        let _ = run::hc(sandbox, &["rm".to_string(), address]);
    }
}

/// The terminals that still belong to the test.
fn leftovers(sandbox: &Sandbox, parent: Option<&str>) -> Vec<String> {
    let mut labels = vec![format!("ez.ir4={}", sandbox.id)];
    if let Some(parent) = parent {
        labels.push(format!("ez.watch={parent}"));
        labels.push(format!("ez.parent={parent}"));
    }
    let mut left = Vec::new();
    for terminal in run::all_terminals(sandbox) {
        let terminal_labels = terminal.get("labels").cloned().unwrap_or(Value::Null);
        let matches = labels.iter().any(|label| {
            let (key, value) = label.split_once('=').unwrap_or((label, ""));
            text(&terminal_labels, key) == value
        });
        if matches {
            left.push(text(&terminal, "address"));
        }
    }
    left
}
