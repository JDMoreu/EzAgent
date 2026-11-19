//! El estado de cada subagente y los avisos al padre (§13 del diseño).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::checks::Report;
use super::children::{ez_as, Child, Parent};
use super::proc;
use super::rejects::terminals_with_label;
use super::setup::Sandbox;

/// Cuánto se espera a que el vigilante avise (segundos).
const WAIT_S: f64 = 45.0;

/// Cuánto se espera al aviso de «fallido», que el brief exige en 15 s.
const WAIT_FAILED_S: f64 = 15.0;

/// El nombre del subagente de tipo Claude Code (el que declara su estado).
const PLANNER: &str = "plan1-claude";

/// El nombre del subagente de tipo Qwen Code (el que se mata).
const QWEN: &str = "dev2-qwen";

/// El detalle de un aviso que no llegó: lo que el padre sí recibió.
fn missing_notice(sandbox: &Sandbox, parent: &Parent, notice: &str) -> String {
    let lines = parent_lines(sandbox, parent);
    format!(
        "no llegó «{notice}»; el padre recibió {} líneas: {}",
        lines.len(),
        lines.join(" || ")
    )
}

/// Comprueba el estado de los subagentes y los avisos al padre.
pub fn run(sandbox: &Sandbox, parent: &Parent, children: &[Child], report: &mut Report) {
    check_finished(sandbox, parent, report);
    check_validated(sandbox, parent, children, report);
    check_failed(sandbox, parent, children, report);
    check_tasks(sandbox, parent, report);
    check_watcher(sandbox, parent, report);
}

/// El hijo de tipo Claude Code termina y el padre se entera.
fn check_finished(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let state = wait_for_state(sandbox, &parent.address, PLANNER, "terminado");
    report.check(
        "el registro de plan1-claude pasa a «terminado»",
        state.as_deref() == Some("terminado"),
        format!("{state:?}"),
    );
    let notice = format!("[EzAgent · aviso] {PLANNER} (Claude Code · sonnet) terminó");
    let lines = wait_for_line(sandbox, parent, &notice, WAIT_S);
    report.check(
        "el padre recibe el aviso de «terminado»",
        lines.is_some(),
        match &lines {
            Some(line) => line.clone(),
            None => missing_notice(sandbox, parent, &notice),
        },
    );
}

/// La validación desde un hermano: el registro pasa a «validado».
fn check_validated(sandbox: &Sandbox, parent: &Parent, children: &[Child], report: &mut Report) {
    let Some(sibling) = children
        .iter()
        .find(|child| child.agent == "opencode" && !child.terminal.is_empty())
    else {
        report.fail("validar desde un hermano", "no hay ningún hermano vivo");
        return;
    };
    let args = vec![
        "validate".to_string(),
        PLANNER.to_string(),
        "--pass".to_string(),
        "--summary".to_string(),
        "ok".to_string(),
        "--json".to_string(),
    ];
    let output = ez_as(sandbox, &sibling.terminal, &args);
    report.check(
        "validar plan1-claude desde un hermano",
        output.ok(),
        format!("código {}: {}", output.code, output.message()),
    );
    let state = wait_for_state(sandbox, &parent.address, PLANNER, "validado");
    report.check(
        "el registro de plan1-claude pasa a «validado»",
        state.as_deref() == Some("validado"),
        format!("{state:?}"),
    );
    let notice = format!("[EzAgent · aviso] {PLANNER} quedó validado por");
    let lines = wait_for_line(sandbox, parent, &notice, WAIT_S);
    report.check(
        "el padre recibe el aviso de «validado»",
        lines.is_some(),
        match &lines {
            Some(line) => line.clone(),
            None => missing_notice(sandbox, parent, &notice),
        },
    );
}

/// El hijo de tipo Qwen Code se mata: el registro pasa a «fallido».
fn check_failed(sandbox: &Sandbox, parent: &Parent, children: &[Child], report: &mut Report) {
    let Some(child) = children
        .iter()
        .find(|child| child.agent == "qwen" && !child.terminal.is_empty())
    else {
        report.fail("matar a dev2-qwen", "no se pudo delegar");
        return;
    };
    let output = proc::run(
        &sandbox.hc,
        &["kill".to_string(), child.terminal.clone()],
        &proc::EnvVars::default(),
        None,
    );
    report.check(
        "matar a dev2-qwen",
        output.ok(),
        format!("código {}: {}", output.code, output.message()),
    );
    let state = wait_for_state(sandbox, &parent.address, QWEN, "fallido");
    report.check(
        "el registro de dev2-qwen pasa a «fallido»",
        state.as_deref() == Some("fallido"),
        format!("{state:?}"),
    );
    let notice = format!("[EzAgent · aviso] {QWEN} (Qwen Code · qwen3-coder-plus) falló");
    let lines = wait_for_line(sandbox, parent, &notice, WAIT_FAILED_S);
    report.check(
        "el padre recibe el aviso de «fallido»",
        lines.is_some(),
        match &lines {
            Some(line) => line.clone(),
            None => missing_notice(sandbox, parent, &notice),
        },
    );
}

/// `ez tasks` muestra los cuatro estados.
fn check_tasks(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let args = vec!["tasks".to_string(), "--json".to_string()];
    let output = ez_as(sandbox, &parent.address, &args);
    if !output.ok() {
        report.fail(
            "`ez tasks --json`",
            format!("código {}: {}", output.code, output.message()),
        );
        return;
    }
    let Ok(data) = proc::json(&output) else {
        report.fail("`ez tasks --json`", "no devolvió JSON");
        return;
    };
    let states: Vec<String> = match &data {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("state").and_then(Value::as_str))
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    for wanted in ["validado", "fallido", "en_proceso"] {
        report.check(
            &format!("`ez tasks` muestra «{wanted}»"),
            states.iter().any(|state| state == wanted),
            format!("{states:?}"),
        );
    }
}

/// El vigilante del padre existe una sola vez.
fn check_watcher(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let label = format!("ez.watch={}", parent.address);
    let watchers = terminals_with_label(sandbox, &label);
    report.check(
        "el vigilante del padre existe una sola vez",
        watchers.len() == 1,
        format!("{watchers:?}"),
    );
}

/// El estado de un registro del padre.
pub fn record_path(sandbox: &Sandbox, parent: &str, name: &str) -> PathBuf {
    sandbox
        .root
        .join("subagentes")
        .join(parent.replace('/', "_"))
        .join(format!("{name}.json"))
}

/// El estado de un registro, si existe.
fn state_of(sandbox: &Sandbox, parent: &str, name: &str) -> Option<String> {
    let path = record_path(sandbox, parent, name);
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("state")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Espera a que un registro llegue a un estado.
fn wait_for_state(sandbox: &Sandbox, parent: &str, name: &str, wanted: &str) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs_f64(WAIT_S);
    loop {
        let state = state_of(sandbox, parent, name);
        if state.as_deref() == Some(wanted) {
            return state;
        }
        if Instant::now() >= deadline {
            return state;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Las líneas que recibió el padre falso.
pub fn parent_lines(sandbox: &Sandbox, parent: &Parent) -> Vec<String> {
    let path = sandbox.registros.join(format!("{}.json", parent.address));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    match value.get("lines") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().unwrap_or_default().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Espera a que el padre reciba una línea con ese texto.
fn wait_for_line(sandbox: &Sandbox, parent: &Parent, wanted: &str, seconds: f64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    loop {
        let found = parent_lines(sandbox, parent)
            .into_iter()
            .find(|line| line.contains(wanted));
        if found.is_some() {
            return found;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
