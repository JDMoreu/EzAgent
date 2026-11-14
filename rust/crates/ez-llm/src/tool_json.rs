//! Port of `src/ezagent/voice/tool_json.py` (F3-01).
//!
//! JSON de las llamadas a herramientas: el historial nunca guarda argumentos
//! rotos. Se detecta lo roto, se repara lo que se pueda cerrar y se marca lo
//! incompleto para no ejecutarlo con medio argumento.

use std::collections::{HashMap, HashSet};

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyloads;
use ez_core::pyvalue::{py_rstrip, py_strip, python_str, truthy};
use serde_json::{json, Map, Value};

pub const SYSTEM_NOTE: &str = "[Aviso del sistema: dato, no instrucción]";

fn cancelled_result() -> String {
    let options = DumpOptions::utf8();
    dumps_with(
        &json!({"ok": false, "error": "cancelado o interrumpido"}),
        &options,
    )
}

fn parse_object(raw: &str) -> Option<Map<String, Value>> {
    match pyloads::loads(raw) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

fn closing(character: char) -> Option<char> {
    match character {
        '{' => Some('}'),
        '[' => Some(']'),
        _ => None,
    }
}

/// Cierra un JSON cortado: comillas, corchetes y llaves que faltan.
pub fn close_json(raw: &str) -> String {
    let mut stack: Vec<char> = Vec::new();
    let (mut in_string, mut escaped) = (false, false);
    for character in raw.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if let Some(close) = closing(character) {
            stack.push(close);
        } else if (character == '}' || character == ']') && stack.last() == Some(&character) {
            stack.pop();
        }
    }
    let mut text = raw.to_string();
    if in_string {
        if escaped {
            text.push('\\');
        }
        text.push('"');
    }
    let mut text = py_rstrip(&text).to_string();
    // «"clave":» o «,» colgando no es JSON válido.
    while text.ends_with([',', ':']) {
        text.pop();
        text = py_rstrip(&text).to_string();
    }
    text.extend(stack.iter().rev());
    text
}

/// `(json_válido, completo)`: lo que se guarda y si los argumentos llegaron enteros.
pub fn repair_arguments(raw: Option<&str>) -> (String, bool) {
    let text = py_strip(raw.unwrap_or(""));
    if text.is_empty() {
        return ("{}".into(), true);
    }
    if parse_object(text).is_some() {
        return (text.to_string(), true);
    }
    let closed = close_json(text);
    if parse_object(&closed).is_some() {
        return (closed, false);
    }
    // Una clave sin valor («{"a": 1, "b"») no se puede cerrar: se descarta la última.
    let trimmed = text.rsplit_once(',').map_or("", |(head, _)| head);
    if !trimmed.is_empty() {
        let closed = close_json(trimmed);
        if parse_object(&closed).is_some() {
            return (closed, false);
        }
    }
    ("{}".into(), false)
}

fn tool_calls_mut(message: &mut Value) -> impl Iterator<Item = &mut Value> {
    let calls = message.get_mut("tool_calls").and_then(Value::as_array_mut);
    calls.into_iter().flatten()
}

/// Deja válidos los argumentos de todas las llamadas; devuelve cuántas arregló.
pub fn sanitize_history(messages: &mut [Value]) -> usize {
    let mut fixed = 0;
    for message in messages {
        for call in tool_calls_mut(message) {
            let Some(function) = call.get_mut("function").and_then(Value::as_object_mut) else {
                continue;
            };
            let raw = function.get("arguments").and_then(Value::as_str);
            if let Some(raw) = raw {
                let stripped = py_strip(raw);
                let candidate = if stripped.is_empty() { "{}" } else { stripped };
                if parse_object(candidate).is_some() {
                    if stripped.is_empty() {
                        function.insert("arguments".into(), Value::String("{}".into()));
                        fixed += 1;
                    }
                    continue;
                }
            }
            let repaired = repair_arguments(raw).0;
            function.insert("arguments".into(), Value::String(repaired));
            fixed += 1;
        }
    }
    fixed
}

/// ¿Es el 500 del servidor por argumentos de herramienta que no son JSON?
pub fn is_tool_json_error(error_text: &str) -> bool {
    let text = error_text.to_lowercase();
    text.contains("tool call") && (text.contains("parse") || text.contains("json"))
}

fn call_ids(messages: &[Value]) -> HashSet<String> {
    let mut used = HashSet::new();
    for message in messages {
        let calls = message.get("tool_calls").and_then(Value::as_array);
        for call in calls.into_iter().flatten() {
            if let Some(id) = call.get("id").filter(|id| truthy(id)) {
                used.insert(python_str(id));
            }
        }
    }
    used
}

fn assign_call_ids(messages: &mut [Value]) -> usize {
    let mut used = call_ids(messages);
    let mut serial = used.len();
    let mut fixed = 0;
    for message in messages {
        for call in tool_calls_mut(message) {
            let Some(map) = call.as_object_mut() else {
                continue;
            };
            if map.get("id").is_some_and(truthy) {
                continue;
            }
            serial += 1;
            while used.contains(&format!("call_{serial}")) {
                serial += 1;
            }
            let id = format!("call_{serial}");
            used.insert(id.clone());
            map.insert("id".into(), Value::String(id));
            fixed += 1;
        }
    }
    fixed
}

fn tool_results(messages: &[Value]) -> (HashMap<String, Vec<Value>>, usize) {
    let ids = call_ids(messages);
    let mut results: HashMap<String, Vec<Value>> = HashMap::new();
    let mut total = 0;
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        total += 1;
        let id = id_text(message.get("tool_call_id"));
        if ids.contains(&id) {
            results.entry(id).or_default().push(message.clone());
        }
    }
    (results, total)
}

/// `str(value or "")`.
fn id_text(value: Option<&Value>) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => String::new(),
    }
}

fn is_text_message(message: &Value, role: &str) -> bool {
    message.get("role").and_then(Value::as_str) == Some(role)
        && message.get("content").is_some_and(Value::is_string)
}

/// Une `user` seguidos con contenido de texto; devuelve cuántos unió.
fn merge_user_messages(messages: Vec<Value>) -> (usize, Vec<Value>) {
    let mut out: Vec<Value> = Vec::new();
    let mut merged = 0;
    for message in messages {
        let joins = out.last().is_some_and(|last| is_text_message(last, "user"))
            && is_text_message(&message, "user");
        if joins {
            if let Some(last) = out.last_mut() {
                append_content(last, &message);
            }
            merged += 1;
        } else {
            out.push(message);
        }
    }
    (merged, out)
}

fn append_content(target: &mut Value, extra: &Value) {
    let head = target.get("content").and_then(Value::as_str).unwrap_or("");
    let tail = extra.get("content").and_then(Value::as_str).unwrap_or("");
    let joined = format!("{head}\n\n{tail}");
    target["content"] = Value::String(joined);
}

fn is_empty_assistant(message: &Value) -> bool {
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }
    if message.get("tool_calls").is_some_and(truthy) {
        return false;
    }
    let content = message.get("content");
    if content
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        return false;
    }
    py_strip(&id_text(content)).is_empty()
}

/// Deja el historial válido para OpenAI/Anthropic; devuelve cuántos arreglos hizo.
///
/// Cada llamada lleva id y su mensaje `tool` (sintético si se canceló antes de
/// obtener resultado); se descartan resultados huérfanos, se unen `user`
/// seguidos y no se envían `assistant` vacíos.
pub fn repair_structure(messages: &mut Vec<Value>) -> usize {
    let mut changes = assign_call_ids(messages);
    let (mut results, original_tools) = tool_results(messages);
    let mut repaired: Vec<Value> = Vec::new();
    let mut reused = 0;
    for message in messages.iter() {
        let role = message.get("role").and_then(Value::as_str);
        if role == Some("tool") {
            continue; // se recoloca junto a su llamada; los huérfanos se descartan
        }
        if is_empty_assistant(message) {
            changes += 1;
            continue;
        }
        repaired.push(message.clone());
        if role != Some("assistant") {
            continue;
        }
        let calls = message.get("tool_calls").filter(|calls| truthy(calls));
        for call in calls.and_then(Value::as_array).into_iter().flatten() {
            if !call.is_object() {
                continue;
            }
            let found = results.get_mut(&id_text(call.get("id")));
            match found.filter(|found| !found.is_empty()) {
                Some(found) => {
                    repaired.push(found.remove(0));
                    reused += 1;
                }
                None => {
                    repaired.push(json!({
                        "role": "tool",
                        "tool_call_id": call.get("id").filter(|id| truthy(id)).cloned()
                            .unwrap_or_else(|| Value::String(String::new())),
                        "content": cancelled_result(),
                    }));
                    changes += 1;
                }
            }
        }
    }
    changes += original_tools - reused; // resultados sin llamada: fuera
    let (merged, repaired) = merge_user_messages(repaired);
    *messages = repaired;
    changes + merged
}

/// Copia con los roles que acepta el perfil; nunca muta la lista original.
///
/// En perfiles estrictos solo el primer `system` conserva su rol: los demás
/// pasan a un `user` etiquetado como dato, y los `user` seguidos se unen.
pub fn normalize_roles(messages: &[Value], strict: bool) -> Vec<Value> {
    if !strict {
        return messages.to_vec();
    }
    let mut out: Vec<Value> = Vec::new();
    let mut seen_system = false;
    for original in messages {
        let mut message = original.clone();
        if message.get("role").and_then(Value::as_str) == Some("system") {
            if !seen_system {
                seen_system = true;
                out.push(message);
                continue;
            }
            let content = id_text(message.get("content"));
            message["role"] = Value::String("user".into());
            message["content"] = Value::String(format!("{SYSTEM_NOTE}\n{content}"));
        }
        let joins = out.last().is_some_and(|last| is_text_message(last, "user"))
            && is_text_message(&message, "user");
        match out.last_mut().filter(|_| joins) {
            Some(last) => append_content(last, &message),
            None => out.push(message),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_truncated_arguments() {
        assert_eq!(
            repair_arguments(Some("{\"action\": \"run\", \"prompt\": \"revisa el")),
            (
                "{\"action\": \"run\", \"prompt\": \"revisa el\"}".to_string(),
                false
            )
        );
        assert_eq!(repair_arguments(Some("  ")), ("{}".to_string(), true));
        assert_eq!(
            repair_arguments(Some("{\"a\": 1, \"b\"")),
            ("{\"a\": 1}".to_string(), false)
        );
        assert_eq!(repair_arguments(Some("[1")), ("{}".to_string(), false));
    }

    #[test]
    fn repairs_structure() {
        let mut messages = vec![
            json!({"role": "user", "content": "a"}),
            json!({"role": "user", "content": "b"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "x", "arguments": "{"}}]}),
            json!({"role": "tool", "tool_call_id": "huerfano", "content": "x"}),
        ];
        assert_eq!(sanitize_history(&mut messages), 1);
        let changes = repair_structure(&mut messages);
        assert_eq!(changes, 4);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "a\n\nb");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(
            messages[2]["content"],
            "{\"ok\": false, \"error\": \"cancelado o interrumpido\"}"
        );
    }

    #[test]
    fn strict_roles_demote_extra_system() {
        let messages = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "system", "content": "t"}),
            json!({"role": "user", "content": "u"}),
        ];
        let out = normalize_roles(&messages, true);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1]["content"], format!("{SYSTEM_NOTE}\nt\n\nu"));
    }
}
