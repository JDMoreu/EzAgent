//! Lector de Pi (§8.6 de IR-0): `message.usage` de sus sesiones.
//!
//! Pi no guarda porcentaje ni límite: solo tokens y costo por mensaje.

use serde_json::Value;

use super::jsonl;
use super::{Reading, TokenUsage, UsageDirs, Window};

pub(crate) fn read(dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    let root = dirs.home.join(".pi").join("agent").join("sessions");
    let mut reading = Reading::default();
    if !root.is_dir() {
        reading.warnings.push(format!(
            "Sin carpeta de sesiones de Pi en {}.",
            root.display()
        ));
        return reading;
    }
    let files = jsonl::files_since(&root, window.start_ms);
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut messages = 0u64;
    let mut cost: Option<f64> = None;
    let mut observed = 0u64;
    for path in &files {
        jsonl::read_lines(path, |line| {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                return;
            };
            let Some(message) = record.get("message").filter(|message| message.is_object()) else {
                return;
            };
            let Some(usage) = message.get("usage").filter(|usage| usage.is_object()) else {
                return;
            };
            let Some(timestamp) = message.get("timestamp").and_then(Value::as_i64) else {
                return;
            };
            let Ok(timestamp) = u64::try_from(timestamp) else {
                return;
            };
            if timestamp < window.start_ms || timestamp > now_ms {
                return;
            }
            messages += 1;
            observed = observed.max(timestamp);
            input = input.saturating_add(number(usage, "input"));
            output = output.saturating_add(number(usage, "output"));
            cache_read = cache_read.saturating_add(number(usage, "cacheRead"));
            cache_write = cache_write.saturating_add(number(usage, "cacheWrite"));
            if let Some(total) = usage
                .get("cost")
                .and_then(|cost| cost.get("total"))
                .and_then(Value::as_f64)
            {
                cost = Some(cost.unwrap_or(0.0) + total);
            }
        });
    }
    reading.tokens = Some(TokenUsage::new(input, output, cache_read, cache_write));
    reading.messages = Some(messages);
    reading.cost_usd = cost;
    reading.observed_ms = (observed > 0).then_some(observed);
    reading.sources.tokens = Some(format!(
        "message.usage de {} sesión(es) reciente(s) de Pi",
        files.len()
    ));
    reading.sources.messages = Some("registros con usage de Pi".to_string());
    if cost.is_some() {
        reading.sources.cost = Some("message.usage.cost.total de Pi".to_string());
    }
    reading
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests;
