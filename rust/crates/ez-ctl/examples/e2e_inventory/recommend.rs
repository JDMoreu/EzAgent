//! Step 3: `ez recommend` for `developer`, `qa_validator` and `planner`.

use serde_json::Value;

use super::checks::Report;
use super::run::{ez_json, strings, text};
use super::setup::Sandbox;

/// Runs the three recommendations of the brief.
pub fn run(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    check_developer(sandbox, parent, report);
    check_validator(sandbox, parent, report);
    check_planner(sandbox, parent, report);
}

/// The recommendation object of a role.
fn recommendation(sandbox: &Sandbox, parent: &str, role: &str) -> Result<Value, String> {
    let args = vec![
        "recommend".to_string(),
        role.to_string(),
        "--json".to_string(),
    ];
    let data = ez_json(sandbox, Some(parent), &args)?;
    data.get("recommendation")
        .cloned()
        .ok_or_else(|| "la respuesta no trae «recommendation»".to_string())
}

/// `developer`: the cheapest usable medium model of the test table.
fn check_developer(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let recommendation = match recommendation(sandbox, parent, "developer") {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez recommend developer`", message);
            return;
        }
    };
    report.check(
        "developer: elige el modelo medio de OpenCode",
        text(&recommendation, "agent") == "opencode"
            && text(&recommendation, "model") == "opencode-go/deepseek-v4.1-flash",
        format!(
            "{} · {}",
            text(&recommendation, "agent"),
            text(&recommendation, "model")
        ),
    );
    report.check(
        "developer: nivel medio, costo plan y esfuerzo high",
        text(&recommendation, "level") == "medio"
            && text(&recommendation, "cost") == "plan"
            && text(&recommendation, "effort") == "high",
        format!(
            "{} · {} · {}",
            text(&recommendation, "level"),
            text(&recommendation, "cost"),
            text(&recommendation, "effort")
        ),
    );
    let warnings = strings(&recommendation, "warnings");
    report.check(
        "developer: la recomendación no deja avisos",
        warnings.is_empty(),
        format!("{warnings:?}"),
    );
}

/// `qa_validator`: a family different from the previous developer.
fn check_validator(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let recommendation = match recommendation(sandbox, parent, "qa_validator") {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez recommend qa_validator`", message);
            return;
        }
    };
    report.check(
        "qa_validator: elige Claude Code · opus",
        text(&recommendation, "agent") == "claude" && text(&recommendation, "model") == "opus",
        format!(
            "{} · {}",
            text(&recommendation, "agent"),
            text(&recommendation, "model")
        ),
    );
    report.check(
        "qa_validator: nivel alto y esfuerzo high",
        text(&recommendation, "level") == "alto" && text(&recommendation, "effort") == "high",
        format!(
            "{} · {}",
            text(&recommendation, "level"),
            text(&recommendation, "effort")
        ),
    );
    let family_reason = reasons(&recommendation)
        .into_iter()
        .find(|reason| reason.contains("distinta a la del último developer del padre (deepseek)"))
        .unwrap_or_else(|| "sin razón de familia".to_string());
    report.check(
        "qa_validator: la familia es distinta a la del último developer",
        text(&recommendation, "family") == "anthropic"
            && family_reason.contains("distinta a la del último developer"),
        format!(
            "familia {} · {family_reason}",
            text(&recommendation, "family")
        ),
    );
}

/// `planner`: the first high-level plan model.
fn check_planner(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let recommendation = match recommendation(sandbox, parent, "planner") {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez recommend planner`", message);
            return;
        }
    };
    report.check(
        "planner: elige OpenCode · deepseek-v4-pro",
        text(&recommendation, "agent") == "opencode"
            && text(&recommendation, "model") == "opencode-go/deepseek-v4-pro",
        format!(
            "{} · {}",
            text(&recommendation, "agent"),
            text(&recommendation, "model")
        ),
    );
    report.check(
        "planner: nivel alto y esfuerzo high",
        text(&recommendation, "level") == "alto" && text(&recommendation, "effort") == "high",
        format!(
            "{} · {}",
            text(&recommendation, "level"),
            text(&recommendation, "effort")
        ),
    );
    let reasons = reasons(&recommendation);
    let explains_role = reasons
        .iter()
        .any(|reason| reason.contains("El papel «planner» pide como mínimo nivel alto"));
    let explains_cost = reasons.iter().any(|reason| reason.contains("costo plan"));
    report.check(
        "planner: explica el papel y el costo",
        explains_role && explains_cost,
        format!("{reasons:?}"),
    );
}

/// The reasons of a recommendation.
fn reasons(recommendation: &Value) -> Vec<String> {
    recommendation
        .get("reasons")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
