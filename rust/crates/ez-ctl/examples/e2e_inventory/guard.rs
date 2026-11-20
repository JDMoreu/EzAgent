//! Step 5: the resource guard of `ez delegate` (code 4) and `--force`.

use serde_json::Value;

use super::checks::Report;
use super::run::{self, ez_json, flag, strings, text};
use super::setup::{self, Sandbox};

/// The absurd thresholds of the test (the machine never has this much RAM).
const ABSURD_MB: u64 = 999_999_999;

/// Runs the rejected delegation and the forced one.
pub fn run(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    if let Err(message) = setup::set_resources(sandbox, ABSURD_MB, ABSURD_MB, 99) {
        report.fail("umbrales absurdos en voice.json", message);
        return;
    }
    let (brief, _) = match setup::write_brief(sandbox, 2) {
        Ok(value) => value,
        Err(message) => {
            report.fail("el encargo de la guarda", message);
            return;
        }
    };
    let args = delegate_args(sandbox, &brief, false);
    let label = format!("ez.parent={parent}");
    let before = run::terminals_with_label(sandbox, &label).len();
    let output = run::ez(sandbox, Some(parent), &args);
    report.check(
        "la guarda rechaza con código 4",
        output.code == 4,
        format!("código {}: {}", output.code, output.message()),
    );
    let message = output.message();
    report.check(
        "la guarda usa el texto de §4",
        message.starts_with("Recursos justos:")
            && message.contains("Espera a que termine alguno o usa --force."),
        message.clone(),
    );
    let after = run::terminals_with_label(sandbox, &label).len();
    report.check(
        "el rechazo no abre ninguna terminal",
        after <= before,
        format!("{before} antes, {after} después"),
    );
    let forced = delegate_args(sandbox, &brief, true);
    match ez_json(sandbox, Some(parent), &forced) {
        Ok(row) => check_forced(sandbox, &row, parent, report),
        Err(message) => report.fail("`--force` lanza con aviso", message),
    }
}

/// The delegation arguments with the absurd thresholds in place.
fn delegate_args(sandbox: &Sandbox, brief: &std::path::Path, force: bool) -> Vec<String> {
    let mut args = vec![
        "delegate".to_string(),
        "developer".to_string(),
        "--agent".to_string(),
        "qwen".to_string(),
        "--model".to_string(),
        "qwen3-coder-plus".to_string(),
        "--brief".to_string(),
        setup::path_text(brief),
        "--cwd".to_string(),
        setup::path_text(&sandbox.work),
        "--json".to_string(),
    ];
    if force {
        args.push("--force".to_string());
    }
    args
}

/// The forced launch says it skipped the guard and the child exists.
fn check_forced(sandbox: &Sandbox, row: &Value, parent: &str, report: &mut Report) {
    let warnings = strings(row, "warnings");
    let force = warnings
        .iter()
        .find(|warning| warning.contains("--force: me salté la guarda de recursos"))
        .cloned()
        .unwrap_or_else(|| "sin aviso de --force".to_string());
    report.check(
        "`--force` lanza con el aviso de la guarda",
        flag(row, "ok") && force.contains("--force"),
        force,
    );
    report.check(
        "`--force` abre el hijo dev2-qwen bajo el padre",
        text(row, "name") == "dev2-qwen"
            && text(row, "agent") == "qwen"
            && text(row, "parent") == parent,
        format!(
            "{} · {} · padre {}",
            text(row, "name"),
            text(row, "agent"),
            text(row, "parent")
        ),
    );
    let path = sandbox.subagentes.join(parent).join("dev2-qwen.json");
    let record = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let state = record
        .as_ref()
        .map(|record| text(record, "state"))
        .unwrap_or_default();
    report.check(
        "el registro de dev2-qwen queda en_proceso",
        state == "en_proceso",
        format!("{state} ({})", path.display()),
    );
}
