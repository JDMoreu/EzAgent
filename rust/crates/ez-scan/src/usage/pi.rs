//! Lector de Pi (§8.6 de IR-0): `message.usage` de sus sesiones.
//!
//! Pi no guarda porcentaje ni límite: solo tokens y costo por mensaje.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use serde_json::Value;

use super::jsonl;
use super::{ModelUsage, Reading, TokenUsage, UsageDirs, UsageState, Window};

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
    let mut by_model: BTreeMap<String, Totals> = BTreeMap::new();
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
            let model = message
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("desconocido")
                .to_string();
            let totals = by_model.entry(model).or_default();
            let input_tokens = number(usage, "input");
            let output_tokens = number(usage, "output");
            let read_tokens = number(usage, "cacheRead");
            let write_tokens = number(usage, "cacheWrite");
            input = input.saturating_add(input_tokens);
            output = output.saturating_add(output_tokens);
            cache_read = cache_read.saturating_add(read_tokens);
            cache_write = cache_write.saturating_add(write_tokens);
            totals.input = totals.input.saturating_add(input_tokens);
            totals.output = totals.output.saturating_add(output_tokens);
            totals.cache_read = totals.cache_read.saturating_add(read_tokens);
            totals.cache_write = totals.cache_write.saturating_add(write_tokens);
            totals.messages += 1;
            if let Some(total) = usage
                .get("cost")
                .and_then(|cost| cost.get("total"))
                .and_then(Value::as_f64)
            {
                cost = Some(cost.unwrap_or(0.0) + total);
                totals.cost = Some(totals.cost.unwrap_or(0.0) + total);
            }
        });
    }
    reading.tokens = Some(TokenUsage::new(input, output, cache_read, cache_write));
    reading.messages = Some(messages);
    reading.cost_usd = cost;
    reading.models = by_model
        .into_iter()
        .map(|(model, totals)| ModelUsage {
            model,
            tokens: Some(TokenUsage::new(
                totals.input,
                totals.output,
                totals.cache_read,
                totals.cache_write,
            )),
            messages: Some(totals.messages),
            cost_usd: totals.cost,
            state: UsageState::Desconocido,
        })
        .collect();
    reading
        .models
        .sort_by_key(|model| Reverse(model.tokens.map_or(0, |tokens| tokens.total)));
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

#[derive(Default)]
struct Totals {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    messages: u64,
    cost: Option<f64>,
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests;
