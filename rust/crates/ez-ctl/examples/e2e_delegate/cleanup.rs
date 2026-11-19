//! La limpieza: las terminales de la prueba, su espacio y el foco de la ventana.

use serde_json::Value;

use super::checks::Report;
use super::children::{app_json, text, Parent};
use super::proc::{self, EnvVars};
use super::rejects::terminals_with_label;
use super::setup::{self, Sandbox};

/// Deja la ventana como estaba y borra la carpeta de la prueba.
pub fn run(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    kill_children(sandbox, parent, report);
    kill_watcher(sandbox, parent, report);
    close_space(sandbox, parent, report);
    kill_parent(sandbox, parent, report);
    restore_focus(sandbox, parent, report);
    setup::remove_root(sandbox);
    let gone = !sandbox.root.exists();
    report.check(
        "la carpeta de la prueba se borra",
        gone,
        sandbox.root.to_string_lossy().into_owned(),
    );
    check_gone(sandbox, parent, report);
}

/// Mata y borra los hijos de la prueba.
fn kill_children(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let label = format!("ez.parent={}", parent.address);
    let children = terminals_with_label(sandbox, &label);
    for terminal in &children {
        kill(sandbox, terminal);
    }
    report.check(
        "los hijos de la prueba se borran",
        children.is_empty() || terminals_with_label(sandbox, &label).is_empty(),
        format!("{children:?}"),
    );
}

/// Mata y borra el vigilante del padre.
fn kill_watcher(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let label = format!("ez.watch={}", parent.address);
    let watchers = terminals_with_label(sandbox, &label);
    for terminal in &watchers {
        kill(sandbox, terminal);
    }
    report.check(
        "el vigilante de la prueba se borra",
        watchers.is_empty() || terminals_with_label(sandbox, &label).is_empty(),
        format!("{watchers:?}"),
    );
}

/// Cierra el espacio de la prueba.
fn close_space(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    if parent.space_id.is_empty() {
        report.pass("el espacio de la prueba se cierra", "no hubo espacio");
        return;
    }
    let args = vec![
        "do".to_string(),
        "space.close".to_string(),
        "--id".to_string(),
        parent.space_id.clone(),
        "--confirmed".to_string(),
        "true".to_string(),
    ];
    let output = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
    report.check(
        "el espacio de la prueba se cierra",
        output.ok(),
        format!("código {}: {}", output.code, output.message()),
    );
}

/// Mata y borra la terminal del padre.
fn kill_parent(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    kill(sandbox, &parent.address);
    let left = terminals_with_label(sandbox, &format!("ez.parent={}", parent.address));
    report.check(
        "las terminales de la prueba se borran",
        left.is_empty(),
        format!("{left:?}"),
    );
}

/// Devuelve el foco a donde estaba.
fn restore_focus(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let pane = text(&parent.focus, "focused_pane");
    let agent = text(&parent.focus, "agent");
    let mut done = false;
    if !pane.is_empty() {
        let args = vec![
            "do".to_string(),
            "pane.focus".to_string(),
            "--pane".to_string(),
            pane.clone(),
        ];
        let output = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
        done = output.ok();
    }
    if !done && !agent.is_empty() {
        let args = vec![
            "do".to_string(),
            "pane.focus".to_string(),
            "--terminal".to_string(),
            agent.clone(),
        ];
        let output = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
        done = output.ok();
    }
    let now = app_json(sandbox, &["status".to_string()]).unwrap_or(Value::Null);
    let back = text(&now, "agent") == agent || text(&now, "focused_pane") == pane;
    report.check(
        "el foco vuelve a donde estaba",
        done && back,
        format!(
            "pane {pane} / agent {agent}; ahora pane {} / agent {}",
            text(&now, "focused_pane"),
            text(&now, "agent")
        ),
    );
}

/// Comprueba que en Herdr no queda nada de la prueba.
fn check_gone(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let listing = match app_json(sandbox, &["ls".to_string()]) {
        Ok(value) => value,
        Err(message) => {
            report.fail("la ventana de Herdr", message);
            return;
        }
    };
    let spaces = listing
        .get("spaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut left = Vec::new();
    for space in &spaces {
        if text(space, "id") == parent.space_id || text(space, "name") == sandbox.id {
            left.push(text(space, "name"));
            continue;
        }
        let tabs = space
            .get("tabs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for tab in tabs {
            let terminals = tab
                .get("terminals")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for terminal in terminals {
                if text(&terminal, "terminal") == parent.address {
                    left.push(text(&terminal, "terminal"));
                }
            }
        }
    }
    report.check(
        "no queda nada de la prueba en Herdr",
        left.is_empty(),
        format!("{left:?}"),
    );
}

/// Mata y borra una terminal.
fn kill(sandbox: &Sandbox, terminal: &str) {
    let env = EnvVars::default();
    let _ = proc::run(
        &sandbox.hc,
        &["kill".to_string(), terminal.to_string()],
        &env,
        None,
    );
    let _ = proc::run(
        &sandbox.hc,
        &["rm".to_string(), terminal.to_string()],
        &env,
        None,
    );
}
