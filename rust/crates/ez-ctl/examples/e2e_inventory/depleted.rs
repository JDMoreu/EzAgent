//! Step 7: a `uso.json` entry for the free model does not exhaust OpenCode.

use serde_json::Value;

use super::checks::Report;
use super::fixtures;
use super::run::{ez_json, flag, text};
use super::setup::{Sandbox, OPENCODE_FREE_MODEL, OPENCODE_GO_MODEL};

/// Writes the watcher's `uso.json` and checks `ez usage` and `ez recommend`.
pub fn run(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    if let Err(message) = fixtures::depleted_free_model(sandbox) {
        report.fail("el uso.json del vigilante", message);
        return;
    }
    let usage = match usage_report(sandbox) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez usage` con uso.json", message);
            return;
        }
    };
    let Some(opencode) = agent(&usage, "opencode") else {
        report.fail("OpenCode aparece con uso.json", "no está");
        return;
    };
    let free = format!("opencode/{OPENCODE_FREE_MODEL}");
    let go = format!("opencode-go/{OPENCODE_GO_MODEL}");
    report.check(
        "uso.json agota el modelo gratuito",
        model_state(opencode, &free) == "agotado",
        model_state(opencode, &free).to_string(),
    );
    report.check(
        "uso.json no agota a todo OpenCode",
        text(opencode, "state") != "agotado",
        text(opencode, "state"),
    );
    report.check(
        "el modelo Go sigue disponible",
        model_state(opencode, &go) != "agotado",
        model_state(opencode, &go).to_string(),
    );
    check_recommendation(sandbox, parent, report);
}

/// `ez recommend developer` still picks the OpenCode Go model.
fn check_recommendation(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let args = vec![
        "recommend".to_string(),
        "developer".to_string(),
        "--json".to_string(),
    ];
    let data = match ez_json(sandbox, Some(parent), &args) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez recommend` con uso.json", message);
            return;
        }
    };
    let recommendation = data.get("recommendation").cloned().unwrap_or(Value::Null);
    report.check(
        "`ez recommend` no descarta OpenCode por el modelo gratuito",
        flag(&data, "ok")
            && text(&recommendation, "agent") == "opencode"
            && text(&recommendation, "model") == "opencode-go/deepseek-v4.1-flash",
        format!(
            "{} · {}",
            text(&recommendation, "agent"),
            text(&recommendation, "model")
        ),
    );
}

/// The `usage` object of a fresh report.
fn usage_report(sandbox: &Sandbox) -> Result<Value, String> {
    let args = vec!["usage".to_string(), "--json".to_string()];
    let data = ez_json(sandbox, None, &args)?;
    Ok(data.get("usage").cloned().unwrap_or(Value::Null))
}

/// The agent with that key in a usage report.
fn agent<'a>(usage: &'a Value, key: &str) -> Option<&'a Value> {
    usage
        .get("agents")?
        .as_array()?
        .iter()
        .find(|agent| text(agent, "key") == key)
}

/// The state of a model row.
fn model_state<'a>(agent: &'a Value, name: &str) -> &'a str {
    agent
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|model| text(model, "model") == name)
        .and_then(|model| model.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("desconocido")
}
