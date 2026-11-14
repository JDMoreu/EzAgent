//! Port of `src/ezagent/voice/context.py` (F3-01).
//!
//! Memoria de contexto: medir la conversación y compactar lo viejo en un
//! resumen. Todo es lógica pura; el `Messenger` pone la llamada al modelo.

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyloads;
use ez_core::pyvalue::{
    char_len, collapse_spaces, head_chars, int_value, py_strip, python_str, tail_chars, truthy,
};
use serde_json::{json, Value};

pub const CHARS_PER_TOKEN: f64 = 3.2;
pub const MESSAGE_OVERHEAD: i64 = 6;
pub const COMPACT_AT: f64 = 0.70;
pub const TARGET_AT: f64 = 0.50;
pub const CLIP_AT: f64 = 0.45;
pub const KEEP_TURNS: usize = 4;
pub const SUMMARY_TOKENS: i64 = 900;
pub const TOOL_RESULT_KEEP: usize = 1500;
pub const TRANSCRIPT_CHARS: usize = 48_000;
pub const MIN_BUDGET: i64 = 4_000;
pub const RESERVE_MARGIN: i64 = 500;

pub const SUMMARY_SYSTEM: &str = "Eres el archivista de una conversación entre un usuario y un asistente de voz y chat. Resume la conversación para que el asistente pueda continuarla sin releerla. Escribe en español, en puntos breves, sin saludos ni relleno. Conserva SIEMPRE: lo que el usuario quiere o pidió, hechos y cifras dichos, decisiones tomadas, nombres, rutas de archivos y direcciones exactas, tareas lanzadas o pendientes y su estado, y preferencias del usuario. Omite lo irrelevante y el contenido largo de herramientas (resúmelo en una línea). Si hay un resumen previo, intégralo en el nuevo sin perder nada importante. Máximo 350 palabras.";

fn text_of(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.is_object())
            .map(|part| part.get("text").map_or_else(String::new, python_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn calls_of(message: &Value) -> &[Value] {
    match message.get("tool_calls") {
        Some(Value::Array(calls)) => calls,
        _ => &[],
    }
}

/// `str(function.get(key, ""))`.
fn function_field(function: &Value, key: &str) -> String {
    function.get(key).map_or_else(String::new, python_str)
}

/// Estimación de tokens de una lista de mensajes (barata, sin tokenizador).
pub fn estimate_tokens(messages: &[Value]) -> i64 {
    let mut chars = 0_usize;
    for message in messages {
        chars += char_len(&text_of(message));
        for call in calls_of(message) {
            let empty = json!({});
            let function = call.get("function").filter(|f| truthy(f)).unwrap_or(&empty);
            chars += char_len(&function_field(function, "name"));
            chars += char_len(&function_field(function, "arguments"));
        }
    }
    (chars as f64 / CHARS_PER_TOKEN) as i64 + MESSAGE_OVERHEAD * messages.len() as i64
}

/// Tokens estimados de los esquemas de herramientas tal como se envían.
pub fn estimate_tools_tokens(schemas: Option<&[Value]>) -> i64 {
    let Some(schemas) = schemas.filter(|schemas| !schemas.is_empty()) else {
        return 0;
    };
    let mut compact = DumpOptions::default();
    compact.ensure_ascii = false;
    compact.item_separator = ",".into();
    compact.key_separator = ":".into();
    let mut chars = 0_usize;
    for schema in schemas {
        chars += if schema.is_object() {
            char_len(&dumps_with(schema, &compact))
        } else {
            char_len(&python_str(schema))
        };
    }
    (chars as f64 / CHARS_PER_TOKEN) as i64 + MESSAGE_OVERHEAD * schemas.len() as i64
}

/// Tokens que hay que reservar: esquemas reales + salida + margen de formato.
pub fn reserve_tokens(tools_schemas: Option<&[Value]>, max_tokens: &Value) -> i64 {
    let output = int_value(max_tokens, 0).max(0);
    estimate_tools_tokens(tools_schemas) + output + RESERVE_MARGIN
}

/// Límite de contexto configurado, con un mínimo razonable.
pub fn budget_tokens(configured: &Value) -> i64 {
    int_value(configured, 32_000).max(MIN_BUDGET)
}

/// ¿Hay que resumir? `reserve` son los tokens de las herramientas y la respuesta.
pub fn needs_compaction(messages: &[Value], budget: i64, reserve: i64) -> bool {
    (estimate_tokens(messages) + reserve) as f64 > budget as f64 * COMPACT_AT
}

pub fn needs_clipping(messages: &[Value], budget: i64) -> bool {
    estimate_tokens(messages) as f64 > budget as f64 * CLIP_AT
}

fn role_is(message: &Value, role: &str) -> bool {
    message.get("role").and_then(Value::as_str) == Some(role)
}

/// `(viejo, reciente)` del historial sin el mensaje de sistema.
///
/// `reciente` empieza siempre en un mensaje del usuario (nunca parte una
/// llamada a herramienta de su resultado) y tiene como mucho `keep_turns` vueltas.
pub fn split_history(messages: &[Value], keep_turns: usize) -> (Vec<Value>, Vec<Value>) {
    let history = messages.get(1..).unwrap_or(&[]);
    let starts: Vec<usize> = history
        .iter()
        .enumerate()
        .filter(|(_, message)| role_is(message, "user"))
        .map(|(index, _)| index)
        .collect();
    if starts.len() <= keep_turns {
        return (Vec::new(), history.to_vec());
    }
    // `starts[-0]` es `starts[0]` en Python.
    let cut = if keep_turns == 0 {
        starts[0]
    } else {
        starts[starts.len() - keep_turns]
    };
    (history[..cut].to_vec(), history[cut..].to_vec())
}

/// `(viejo, reciente)` con las vueltas recientes que caben en el 50 % del límite.
pub fn fit_recent(messages: &[Value], budget: i64, reserve: i64) -> (Vec<Value>, Vec<Value>) {
    let system = &messages[..messages.len().min(1)];
    for keep in (1..=KEEP_TURNS).rev() {
        let (old, recent) = split_history(messages, keep);
        if old.is_empty() {
            return (old, recent);
        }
        let joined: Vec<Value> = system.iter().chain(recent.iter()).cloned().collect();
        let fits = (estimate_tokens(&joined) + reserve) as f64 <= budget as f64 * TARGET_AT;
        if fits || keep == 1 {
            return (old, recent);
        }
    }
    split_history(messages, 1)
}

/// Texto plano de lo viejo para que el modelo lo resuma.
pub fn render_transcript(old: &[Value], previous_summary: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut header = String::new();
    if !previous_summary.is_empty() {
        header = format!(
            "RESUMEN PREVIO:\n{}\n\nCONVERSACIÓN NUEVA:\n",
            py_strip(previous_summary)
        );
    }
    for message in old {
        let text = collapse_spaces(&text_of(message));
        if role_is(message, "user") {
            lines.push(format!("Usuario: {text}"));
        } else if role_is(message, "assistant") {
            for call in calls_of(message) {
                let empty = json!({});
                let function = call.get("function").filter(|f| truthy(f)).unwrap_or(&empty);
                let name = function.get("name").map_or_else(|| "?".into(), python_str);
                let args = function_field(function, "arguments");
                lines.push(format!("[herramienta {name} {}]", head_chars(&args, 180)));
            }
            if !text.is_empty() {
                lines.push(format!("Asistente: {text}"));
            }
        } else if role_is(message, "tool") {
            lines.push(format!("[resultado] {}", head_chars(&text, 300)));
        }
    }
    let mut body = lines.join("\n");
    if char_len(&body) > TRANSCRIPT_CHARS {
        // Lo más antiguo es lo menos útil: se conserva el final.
        body = format!("…\n{}", tail_chars(&body, TRANSCRIPT_CHARS));
    }
    // El resumen previo nunca se recorta: es lo que no se puede perder.
    header + &body
}

/// Mensajes para pedirle al modelo el resumen.
pub fn summary_messages(old: &[Value], previous_summary: &str) -> Vec<Value> {
    vec![
        json!({"role": "system", "content": SUMMARY_SYSTEM}),
        json!({"role": "user", "content": render_transcript(old, previous_summary)}),
    ]
}

/// El prompt de sistema con el resumen de lo anterior al final.
pub fn with_summary(system_prompt: &str, summary: &str) -> String {
    if py_strip(summary).is_empty() {
        return system_prompt.to_string();
    }
    format!(
        "{system_prompt}\n\n# Resumen de la conversación hasta ahora\n{}\n(Es lo hablado antes; lo reciente sigue a continuación.)",
        py_strip(summary)
    )
}

/// Recorta un resultado largo; si era JSON, el resultado sigue siendo JSON válido.
fn clip_tool_content(content: &str) -> String {
    if pyloads::loads(content).is_err() {
        let omitted = char_len(content) - TOOL_RESULT_KEEP;
        return format!(
            "{}… [recortado: {omitted} caracteres]",
            head_chars(content, TOOL_RESULT_KEEP)
        );
    }
    // DECISION (del Python): `data` guarda el prefijo literal en vez de
    // reserializar la estructura; la mitad deja sitio al envoltorio.
    let payload = json!({
        "truncated": true,
        "original_chars": char_len(content),
        "data": head_chars(content, TOOL_RESULT_KEEP / 2),
    });
    let mut options = DumpOptions::default();
    options.ensure_ascii = false;
    dumps_with(&payload, &options)
}

/// Recorta los resultados de herramientas de vueltas antiguas; devuelve cuántos.
///
/// `protect_last_turns` vueltas finales se respetan; con 0 se recorta todo.
pub fn clip_old_tool_results(messages: &mut [Value], protect_last_turns: i64) -> usize {
    let starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| role_is(message, "user"))
        .map(|(index, _)| index)
        .collect();
    let boundary = if protect_last_turns <= 0 {
        messages.len()
    } else if starts.len() as i64 >= protect_last_turns {
        starts[starts.len() - protect_last_turns as usize]
    } else {
        0
    };
    let mut clipped = 0;
    for message in &mut messages[..boundary] {
        if !role_is(message, "tool") {
            continue;
        }
        let Some(content) = message.get("content").and_then(Value::as_str) else {
            continue;
        };
        let already = tail_chars(content, 80).contains("… [recortado:");
        if char_len(content) > TOOL_RESULT_KEEP && !already {
            let replacement = clip_tool_content(content);
            message["content"] = Value::String(replacement);
            clipped += 1;
        }
    }
    clipped
}

/// «12 k de 32 k tokens» para registros y avisos.
pub fn describe(tokens: i64, budget: i64) -> String {
    format!(
        "{:.0} k de {:.0} k tokens",
        tokens as f64 / 1000.0,
        budget as f64 / 1000.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation(turns: usize) -> Vec<Value> {
        let mut messages = vec![json!({"role": "system", "content": "s"})];
        for turn in 0..turns {
            messages.push(json!({"role": "user", "content": format!("pregunta {turn}")}));
            messages.push(json!({"role": "assistant", "content": format!("respuesta {turn}")}));
        }
        messages
    }

    #[test]
    fn splits_at_user_turns() {
        let (old, recent) = split_history(&conversation(6), 4);
        assert_eq!(old.len(), 4);
        assert_eq!(recent.len(), 8);
        assert_eq!(recent[0]["content"], "pregunta 2");
        let (old, recent) = split_history(&conversation(3), 4);
        assert!(old.is_empty());
        assert_eq!(recent.len(), 6);
    }

    #[test]
    fn estimates_and_budgets() {
        let messages = conversation(1);
        assert_eq!(estimate_tokens(&messages), 6 * 3 + (1 + 10 + 11) * 10 / 32);
        assert_eq!(budget_tokens(&json!("abc")), 32_000);
        assert_eq!(budget_tokens(&json!(100)), MIN_BUDGET);
        assert_eq!(reserve_tokens(None, &json!(-5)), RESERVE_MARGIN);
        assert_eq!(describe(12_400, 32_000), "12 k de 32 k tokens");
    }

    #[test]
    fn clips_old_tool_results_but_keeps_json_valid() {
        let long = "x".repeat(2000);
        let json_long = format!("{{\"a\": \"{long}\"}}");
        let mut messages = vec![
            json!({"role": "user", "content": "a"}),
            json!({"role": "tool", "content": long}),
            json!({"role": "tool", "content": json_long}),
            json!({"role": "user", "content": "b"}),
            json!({"role": "tool", "content": "y".repeat(2000)}),
        ];
        assert_eq!(clip_old_tool_results(&mut messages, 1), 2);
        assert!(messages[1]["content"]
            .as_str()
            .unwrap_or("")
            .ends_with("… [recortado: 500 caracteres]"));
        let clipped: Value = serde_json::from_str(messages[2]["content"].as_str().unwrap_or(""))
            .unwrap_or(Value::Null);
        assert_eq!(clipped["truncated"], true);
        assert_eq!(messages[4]["content"].as_str().map(str::len), Some(2000));
        assert_eq!(clip_old_tool_results(&mut messages, 1), 0);
    }

    #[test]
    fn transcript_keeps_previous_summary() {
        let old = conversation(1);
        let text = render_transcript(&old[1..], "  antes  ");
        assert!(text.starts_with("RESUMEN PREVIO:\nantes\n\nCONVERSACIÓN NUEVA:\n"));
        assert!(text.ends_with("Usuario: pregunta 0\nAsistente: respuesta 0"));
        assert_eq!(with_summary("p", " "), "p");
    }
}
