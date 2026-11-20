//! Step 2: `ez usage --json` with one fixture per local reader.

use chrono::{Duration, Utc};
use serde_json::Value;

use super::checks::Report;
use super::fixtures::{
    self, CLAUDE_OPUS_TOKENS, CLAUDE_SONNET_TOKENS, MCODE_TOKENS, OPENCODE_FREE_TOKENS,
    OPENCODE_GO_TOKENS, PI_TOKENS,
};
use super::run::{ez_json, flag, number, text};
use super::setup::{Sandbox, MCODE_MODEL, OPENCODE_FREE_MODEL, OPENCODE_GO_MODEL};

/// Tokens of every Claude Code message together.
const CLAUDE_TOKENS: u64 = CLAUDE_SONNET_TOKENS + CLAUDE_OPUS_TOKENS;

/// Runs `ez usage` with the invented data of every reader.
pub fn run(sandbox: &Sandbox, report: &mut Report) {
    if let Err(message) = fixtures::readers(sandbox) {
        report.fail("preparar los datos de uso", message);
        return;
    }
    let data = match usage_data(sandbox) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez usage --json`", message);
            return;
        }
    };
    report.check("`ez usage --json` termina bien", flag(&data, "ok"), "ok");
    check_codex_current(sandbox, report);
    let usage = match expired_codex(sandbox) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez usage` con el dato vencido de Codex", message);
            return;
        }
    };
    check_codex_expired(&usage, report);
    check_claude(&usage, report);
    check_opencode(&usage, report);
    check_mcode(&usage, report);
    check_pi(&usage, report);
    check_qwen(&usage, report);
}

/// The whole `ez usage --json` response.
fn usage_data(sandbox: &Sandbox) -> Result<Value, String> {
    let args = vec!["usage".to_string(), "--json".to_string()];
    ez_json(sandbox, None, &args)
}

/// The `usage` object of a fresh report.
fn usage_report(sandbox: &Sandbox) -> Result<Value, String> {
    let data = usage_data(sandbox)?;
    Ok(data.get("usage").cloned().unwrap_or(Value::Null))
}

/// A Codex session whose window is still open.
fn check_codex_current(sandbox: &Sandbox, report: &mut Report) {
    let now = Utc::now();
    let resets = (now + Duration::hours(1)).timestamp();
    let written = fixtures::codex_session(
        sandbox,
        "vigente.jsonl",
        now - Duration::minutes(5),
        42,
        resets,
    );
    if let Err(message) = written {
        report.fail("preparar Codex vigente", message);
        return;
    }
    let usage = match usage_report(sandbox) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez usage` con Codex vigente", message);
            return;
        }
    };
    let Some(codex) = agent(&usage, "codex") else {
        report.fail("Codex aparece en `ez usage`", "no está");
        return;
    };
    check_codex_window(codex, report);
    check_codex_age(codex, report);
}

/// The current Codex window: percentage, limit, reset and window length.
fn check_codex_window(codex: &Value, report: &mut Report) {
    report.check(
        "Codex vigente: 42 % y estado libre",
        text(codex, "state") == "libre" && (number(codex, "used_percent") - 42.0).abs() < 0.01,
        format!(
            "{} · {} %",
            text(codex, "state"),
            number(codex, "used_percent")
        ),
    );
    let unit = text(&codex.get("limit").cloned().unwrap_or(Value::Null), "unit");
    report.check(
        "Codex vigente: el límite es el porcentaje observado",
        unit == "percent",
        format!("unidad «{unit}»"),
    );
    let reset = codex.get("resets_ms").and_then(Value::as_u64).unwrap_or(0);
    report.check(
        "Codex vigente: el reinicio está en el futuro",
        reset > (Utc::now().timestamp_millis().max(0) as u64),
        format!("resets_ms {reset}"),
    );
    report.check(
        "Codex vigente: la ventana observada es de 5 h",
        (number(codex, "observed_window_hours") - 5.0).abs() < 0.01,
        format!("{} h", number(codex, "observed_window_hours")),
    );
}

/// Codex always says how old its last quota datum is.
fn check_codex_age(codex: &Value, report: &mut Report) {
    let warnings = strings_of(codex, "warnings");
    let age = warnings
        .iter()
        .find(|line| line.contains("Último dato de cuota"))
        .cloned()
        .unwrap_or_else(|| "no avisa de la antigüedad".to_string());
    report.check(
        "Codex vigente: avisa de la antigüedad del dato",
        warnings
            .iter()
            .any(|line| line.contains("Último dato de cuota")),
        age,
    );
}

/// A newer Codex session whose window already reset.
fn expired_codex(sandbox: &Sandbox) -> Result<Value, String> {
    let now = Utc::now();
    let resets = (now - Duration::hours(1)).timestamp();
    fixtures::codex_session(
        sandbox,
        "vencido.jsonl",
        now - Duration::minutes(5),
        30,
        resets,
    )?;
    usage_report(sandbox)
}

/// The expired record is not presented as current usage.
fn check_codex_expired(usage: &Value, report: &mut Report) {
    let Some(codex) = agent(usage, "codex") else {
        report.fail("Codex aparece con el dato vencido", "no está");
        return;
    };
    report.check(
        "Codex vencido: el estado queda desconocido",
        text(codex, "state") == "desconocido",
        text(codex, "state"),
    );
    let source = text(
        &codex.get("sources").cloned().unwrap_or(Value::Null),
        "state",
    );
    report.check(
        "Codex vencido: explica que la ventana reinició",
        source.contains("ya reinició"),
        source,
    );
}

/// Claude Code: tokens and messages per model, with the detected plan.
fn check_claude(usage: &Value, report: &mut Report) {
    let Some(claude) = agent(usage, "claude") else {
        report.fail("Claude Code aparece en `ez usage`", "no está");
        return;
    };
    report.check(
        "Claude Code: 540 tokens en 2 mensajes",
        tokens_total(claude) == CLAUDE_TOKENS
            && claude.get("messages").and_then(Value::as_u64) == Some(2),
        format!(
            "{} tokens · {} mensajes",
            tokens_total(claude),
            messages(claude)
        ),
    );
    report.check(
        "Claude Code: el plan sale de ~/.claude.json",
        text(claude, "plan") == "claude_pro",
        format!("plan «{}»", text(claude, "plan")),
    );
    report.check(
        "Claude Code: los tokens de sonnet",
        model_tokens(claude, "sonnet") == CLAUDE_SONNET_TOKENS,
        format!(
            "{} (se esperaban {CLAUDE_SONNET_TOKENS})",
            model_tokens(claude, "sonnet")
        ),
    );
    report.check(
        "Claude Code: los tokens de opus",
        model_tokens(claude, "opus") == CLAUDE_OPUS_TOKENS,
        format!(
            "{} (se esperaban {CLAUDE_OPUS_TOKENS})",
            model_tokens(claude, "opus")
        ),
    );
    report.check(
        "Claude Code: estado desconocido sin límite declarado",
        text(claude, "state") == "desconocido",
        text(claude, "state"),
    );
}

/// OpenCode: `session_message`, the Free notice and later Go usage.
fn check_opencode(usage: &Value, report: &mut Report) {
    let Some(opencode) = agent(usage, "opencode") else {
        report.fail("OpenCode aparece en `ez usage`", "no está");
        return;
    };
    report.check(
        "OpenCode: 280 tokens y 2 mensajes de session_message",
        tokens_total(opencode) == OPENCODE_GO_TOKENS + OPENCODE_FREE_TOKENS
            && opencode.get("messages").and_then(Value::as_u64) == Some(2),
        format!(
            "{} tokens · {} mensajes",
            tokens_total(opencode),
            messages(opencode)
        ),
    );
    let go = format!("opencode-go/{OPENCODE_GO_MODEL}");
    let free = format!("opencode/{OPENCODE_FREE_MODEL}");
    report.check(
        "OpenCode: el modelo Go no está agotado",
        model_state(opencode, &go) != "agotado"
            && model_tokens(opencode, &go) == OPENCODE_GO_TOKENS,
        format!(
            "{} · {} tokens",
            model_state(opencode, &go),
            model_tokens(opencode, &go)
        ),
    );
    report.check(
        "OpenCode: el modelo gratuito sí está agotado",
        model_state(opencode, &free) == "agotado"
            && model_tokens(opencode, &free) == OPENCODE_FREE_TOKENS,
        format!(
            "{} · {} tokens",
            model_state(opencode, &free),
            model_tokens(opencode, &free)
        ),
    );
    check_opencode_state(opencode, report);
}

/// The agent-level state and the explanation of the Free notice.
fn check_opencode_state(opencode: &Value, report: &mut Report) {
    report.check(
        "OpenCode: el agente no queda agotado por el aviso Free",
        text(opencode, "state") != "agotado",
        text(opencode, "state"),
    );
    report.check(
        "OpenCode: el plan detectado es OpenCode Go",
        text(opencode, "plan") == "OpenCode Go",
        text(opencode, "plan"),
    );
    let warnings = strings_of(opencode, "warnings");
    let free_notice = warnings
        .iter()
        .find(|line| line.contains("Aviso vigente de Free"))
        .cloned()
        .unwrap_or_else(|| "sin aviso de Free".to_string());
    report.check(
        "OpenCode: avisa del aviso Free vigente",
        warnings
            .iter()
            .any(|line| line.contains("Aviso vigente de Free")),
        free_notice,
    );
    let partial = warnings
        .iter()
        .find(|line| line.contains("está agotado; el agente no"))
        .cloned()
        .unwrap_or_else(|| "no distingue modelo y agente".to_string());
    report.check(
        "OpenCode: dice que solo el modelo está agotado",
        warnings
            .iter()
            .any(|line| line.contains("está agotado; el agente no")),
        partial,
    );
    report.check(
        "OpenCode: costo de la ventana",
        (number(opencode, "cost_usd") - 0.5).abs() < 0.001,
        format!("{} USD", number(opencode, "cost_usd")),
    );
}

/// mcode: the SQLite of `local_runtime_token_usage`.
fn check_mcode(usage: &Value, report: &mut Report) {
    let Some(mcode) = agent(usage, "mcode") else {
        report.fail("mcode aparece en `ez usage`", "no está");
        return;
    };
    report.check(
        "mcode: 280 tokens en 2 filas",
        tokens_total(mcode) == MCODE_TOKENS
            && mcode.get("messages").and_then(Value::as_u64) == Some(2),
        format!("{} tokens · {} filas", tokens_total(mcode), messages(mcode)),
    );
    report.check(
        "mcode: los tokens por modelo",
        model_tokens(mcode, MCODE_MODEL) == MCODE_TOKENS,
        format!("{}", model_tokens(mcode, MCODE_MODEL)),
    );
    report.check(
        "mcode: el costo de la ventana",
        (number(mcode, "cost_usd") - 0.75).abs() < 0.001,
        format!("{} USD", number(mcode, "cost_usd")),
    );
    report.check(
        "mcode: estado desconocido (no hay límite local)",
        text(mcode, "state") == "desconocido",
        text(mcode, "state"),
    );
}

/// Pi: tokens and cost from `message.usage`.
fn check_pi(usage: &Value, report: &mut Report) {
    let Some(pi) = agent(usage, "pi") else {
        report.fail("Pi aparece en `ez usage`", "no está");
        return;
    };
    report.check(
        "Pi: 280 tokens en 2 mensajes",
        tokens_total(pi) == PI_TOKENS && pi.get("messages").and_then(Value::as_u64) == Some(2),
        format!("{} tokens · {} mensajes", tokens_total(pi), messages(pi)),
    );
    report.check(
        "Pi: los tokens por modelo",
        model_tokens(pi, "proveedor/libre") == PI_TOKENS,
        format!("{}", model_tokens(pi, "proveedor/libre")),
    );
    report.check(
        "Pi: el costo de la ventana",
        (number(pi, "cost_usd") - 0.6).abs() < 0.001,
        format!("{} USD", number(pi, "cost_usd")),
    );
}

/// Qwen Code has no reader yet; it must still appear as installed.
fn check_qwen(usage: &Value, report: &mut Report) {
    let Some(qwen) = agent(usage, "qwen") else {
        report.fail("Qwen Code aparece en `ez usage`", "no está");
        return;
    };
    report.check(
        "Qwen Code: instalado y sin lector de uso",
        flag(qwen, "installed")
            && text(qwen, "state") == "desconocido"
            && qwen.get("tokens").is_none_or(Value::is_null),
        format!(
            "instalado={} · {}",
            flag(qwen, "installed"),
            text(qwen, "state")
        ),
    );
}

/// The agent with that key in a usage report.
fn agent<'a>(usage: &'a Value, key: &str) -> Option<&'a Value> {
    usage
        .get("agents")?
        .as_array()?
        .iter()
        .find(|agent| text(agent, "key") == key)
}

/// Total tokens of an agent (0 if unknown).
fn tokens_total(agent: &Value) -> u64 {
    agent
        .get("tokens")
        .and_then(|tokens| tokens.get("total"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// The message count of an agent.
fn messages(agent: &Value) -> u64 {
    agent.get("messages").and_then(Value::as_u64).unwrap_or(0)
}

/// The usage row of a model.
fn model<'a>(agent: &'a Value, name: &str) -> Option<&'a Value> {
    agent
        .get("models")?
        .as_array()?
        .iter()
        .find(|model| text(model, "model") == name)
}

/// Tokens of a model (0 if it is not in the report).
fn model_tokens(agent: &Value, name: &str) -> u64 {
    model(agent, name)
        .map(|model| {
            model
                .get("tokens")
                .and_then(|tokens| tokens.get("total"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// State of a model (`desconocido` if it is not in the report).
fn model_state<'a>(agent: &'a Value, name: &str) -> &'a str {
    model(agent, name)
        .and_then(|model| model.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("desconocido")
}

/// A list of texts of a JSON value.
fn strings_of(data: &Value, key: &str) -> Vec<String> {
    data.get(key)
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
