//! Formato de la salida del eval: primer paso del modelo y herramientas
//! disponibles (`_first_step`, `_parse_calls` de `provider.py`).

use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{json, Value};

/// `_first_step`: primer mensaje del asistente después de la frase del usuario.
pub fn first_step(messages: &[Value], user: &str) -> Value {
    for index in (0..messages.len()).rev() {
        let message = &messages[index];
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        if message.get("content").and_then(Value::as_str) != Some(user) {
            continue;
        }
        for later in &messages[index + 1..] {
            if later.get("role").and_then(Value::as_str) == Some("assistant") {
                return later.clone();
            }
        }
        break;
    }
    json!({"content": "", "tool_calls": []})
}

/// `(step.get("content") or "").strip()`.
pub fn text(step: &Value) -> String {
    let raw = step
        .get("content")
        .filter(|value| truthy(value))
        .map_or_else(String::new, python_str);
    py_strip(&raw).to_string()
}

/// `_parse_calls`: nombre, argumentos y si llegaron como objeto JSON.
pub fn parse_calls(step: &Value) -> Vec<Value> {
    let Some(calls) = step.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    calls
        .iter()
        .map(|call| {
            let function = call
                .get("function")
                .filter(|value| truthy(value))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let name = function.get("name").cloned().unwrap_or_else(|| json!(""));
            let raw = function
                .get("arguments")
                .filter(|value| truthy(value))
                .map_or_else(String::new, python_str);
            let source = if raw.is_empty() { "{}" } else { raw.as_str() };
            let (arguments, valid) = match pyloads::loads(source) {
                Ok(parsed) => {
                    let valid = parsed.is_object();
                    (parsed, valid)
                }
                Err(_) => (Value::String(raw), false),
            };
            json!({
                "name": name,
                "arguments": arguments,
                "arguments_valid": valid,
            })
        })
        .collect()
}

/// `[t["function"]["name"] for t in tools]`.
pub fn tool_names(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            tool.get("function")
                .and_then(|function| function.get("name"))
                .cloned()
                .unwrap_or_else(|| json!(""))
        })
        .collect()
}
