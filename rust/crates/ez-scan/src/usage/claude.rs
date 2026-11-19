//! Lector de Claude Code (§8.3 de IR-0): `message.usage` de sus sesiones.
//!
//! Claude Code no guarda el porcentaje ni el reinicio de la ventana: aquí solo
//! se pueden sumar tokens y detectar el aviso `fallback_credit`.

use serde_json::Value;

use super::jsonl;
use super::{Reading, TokenUsage, UsageDirs, Window};

pub(crate) fn read(dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    let root = dirs.home.join(".claude").join("projects");
    let mut reading = Reading::default();
    let files = jsonl::files_since(&root, window.start_ms);
    let (plan, warning) = plan(dirs);
    reading.plan = plan;
    if let Some(warning) = warning {
        reading.warnings.push(warning);
    }
    if reading.plan.is_some() {
        reading.sources.plan = Some("oauthAccount.organizationType de ~/.claude.json".to_string());
    }
    if !root.is_dir() {
        reading.warnings.push(format!(
            "Sin carpeta de sesiones de Claude Code en {}.",
            root.display()
        ));
        return reading;
    }
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut messages = 0u64;
    let mut fallback = 0u64;
    let mut observed = 0u64;
    for path in &files {
        jsonl::read_lines(path, |line| {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                return;
            };
            let Some(usage) = record
                .get("message")
                .and_then(|message| message.get("usage"))
                .filter(|usage| usage.is_object())
            else {
                return;
            };
            let Some(timestamp) = record
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(jsonl::parse_time_ms)
            else {
                return;
            };
            if timestamp < window.start_ms || timestamp > now_ms {
                return;
            }
            messages += 1;
            observed = observed.max(timestamp);
            input = input.saturating_add(number(usage, "input_tokens"));
            output = output.saturating_add(number(usage, "output_tokens"));
            cache_read = cache_read.saturating_add(number(usage, "cache_read_input_tokens"));
            cache_write = cache_write.saturating_add(number(usage, "cache_creation_input_tokens"));
            if usage
                .get("fallback_credit")
                .is_some_and(|credit| !credit.is_null())
            {
                fallback += 1;
            }
        });
    }
    // DECISION: Claude Code no guarda costo por token en disco; no se inventa.
    reading.tokens = Some(TokenUsage::new(input, output, cache_read, cache_write));
    reading.messages = Some(messages);
    reading.observed_ms = (observed > 0).then_some(observed);
    reading.sources.tokens = Some(format!(
        "message.usage de {} sesión(es) reciente(s) de Claude Code",
        files.len()
    ));
    reading.sources.messages = Some("registros con usage de Claude Code".to_string());
    if fallback > 0 {
        reading.depleted = Some(format!(
            "fallback_credit de Claude Code en {fallback} registro(s) de la ventana"
        ));
    }
    reading
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Solo `oauthAccount.organizationType`; el resto del archivo no se toca.
fn plan(dirs: &UsageDirs) -> (Option<String>, Option<String>) {
    let path = dirs.home.join(".claude.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (None, None);
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return (
            None,
            Some("No pude leer ~/.claude.json; el plan queda desconocido.".to_string()),
        );
    };
    let plan = value
        .get("oauthAccount")
        .and_then(|account| account.get("organizationType"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (plan, None)
}

#[cfg(test)]
mod tests;
