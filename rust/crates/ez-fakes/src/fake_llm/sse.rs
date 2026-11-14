//! Cuerpos SSE y no-streaming de OpenAI Chat Completions y Anthropic Messages.

use serde_json::{json, Value};

use super::scenario::{ToolCallSpec, Turn};
use super::Route;

const CREATED: u64 = 1_700_000_000;

fn split_chars(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let size = size.max(1);
    chars
        .chunks(size)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn call_id(turn_index: u64, call_index: usize, call: &ToolCallSpec) -> String {
    if call.id.is_empty() {
        format!("call_fake_{turn_index}_{call_index}")
    } else {
        call.id.clone()
    }
}

fn openai_usage(turn: &Turn) -> Value {
    if let Some(usage) = &turn.usage {
        return usage.clone();
    }
    let completion = turn.output_tokens();
    json!({
        "prompt_tokens": 10,
        "completion_tokens": completion,
        "total_tokens": 10 + completion,
    })
}

fn openai_chunk(model: &str, delta: Value, finish: Option<&str>) -> Value {
    json!({
        "id": "chatcmpl-fake",
        "object": "chat.completion.chunk",
        "created": CREATED,
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    })
}

fn openai_tool_start(model: &str, index: usize, call_id: &str, name: &str) -> Value {
    openai_chunk(
        model,
        json!({"tool_calls": [{
            "index": index,
            "id": call_id,
            "type": "function",
            "function": {"name": name, "arguments": ""},
        }]}),
        None,
    )
}

fn openai_tool_arguments(model: &str, index: usize, arguments: &str) -> Value {
    openai_chunk(
        model,
        json!({"tool_calls": [{
            "index": index,
            "function": {"arguments": arguments},
        }]}),
        None,
    )
}

/// Eventos SSE de Chat Completions; `[DONE]` solo si no hay corte.
pub fn openai_sse(turn: &Turn, model: &str, chunk_chars: usize, turn_index: u64) -> String {
    let mut events = Vec::new();
    for text in split_chars(&turn.content, chunk_chars) {
        events.push(openai_chunk(model, json!({"content": text}), None));
    }
    for (index, call) in turn.tool_calls.iter().enumerate() {
        let id = call_id(turn_index, index, call);
        events.push(openai_tool_start(model, index, &id, &call.name));
        for piece in split_chars(&call.arguments, chunk_chars * 2) {
            events.push(openai_tool_arguments(model, index, &piece));
        }
    }
    let finish = turn.finish();
    let mut last = openai_chunk(model, json!({}), Some(&finish));
    if let (Some(object), Value::Object(usage)) = (last.as_object_mut(), openai_usage(turn)) {
        object.insert("usage".to_string(), Value::Object(usage));
    }
    events.push(last);
    let limit = turn.max_chunks.unwrap_or(usize::MAX);
    let cut = limit < events.len();
    let mut body: String = events
        .iter()
        .take(limit)
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    if !cut {
        body.push_str("data: [DONE]\n\n");
    }
    body
}

fn anthropic_stop(finish: &str) -> &'static str {
    match finish {
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        _ => "end_turn",
    }
}

fn anthropic_usage(turn: &Turn) -> Value {
    let output = turn
        .usage
        .as_ref()
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| turn.output_tokens());
    json!({"output_tokens": output})
}

fn event(name: &'static str, value: Value) -> (&'static str, Value) {
    (name, value)
}

fn anthropic_text(
    turn: &Turn,
    chunk_chars: usize,
    index: usize,
    out: &mut Vec<(&'static str, Value)>,
) {
    if turn.content.is_empty() {
        return;
    }
    out.push(event(
        "content_block_start",
        json!({"type": "content_block_start", "index": index,
               "content_block": {"type": "text", "text": ""}}),
    ));
    for text in split_chars(&turn.content, chunk_chars) {
        out.push(event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": index,
                   "delta": {"type": "text_delta", "text": text}}),
        ));
    }
    out.push(event(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": index}),
    ));
}

fn anthropic_tools(
    turn: &Turn,
    chunk_chars: usize,
    start: usize,
    turn_index: u64,
    out: &mut Vec<(&'static str, Value)>,
) {
    for (offset, call) in turn.tool_calls.iter().enumerate() {
        let index = start + offset;
        let id = call_id(turn_index, offset, call);
        out.push(event(
            "content_block_start",
            json!({"type": "content_block_start", "index": index,
                   "content_block": {"type": "tool_use", "id": id, "name": call.name, "input": {}}}),
        ));
        let arguments = if call.arguments.is_empty() {
            "{}".to_string()
        } else {
            call.arguments.clone()
        };
        for piece in split_chars(&arguments, chunk_chars * 2) {
            out.push(event(
                "content_block_delta",
                json!({"type": "content_block_delta", "index": index,
                       "delta": {"type": "input_json_delta", "partial_json": piece}}),
            ));
        }
        out.push(event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": index}),
        ));
    }
}

/// Eventos SSE de la API Messages con la forma que espera `anthropic_client`.
pub fn anthropic_sse(turn: &Turn, model: &str, chunk_chars: usize, turn_index: u64) -> String {
    let mut events = vec![event(
        "message_start",
        json!({"type": "message_start", "message": {
            "id": format!("msg_fake_{turn_index}"),
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 0},
        }}),
    )];
    let text_blocks = usize::from(!turn.content.is_empty());
    anthropic_text(turn, chunk_chars, 0, &mut events);
    anthropic_tools(turn, chunk_chars, text_blocks, turn_index, &mut events);
    let finish = turn.finish();
    events.push(event(
        "message_delta",
        json!({"type": "message_delta",
               "delta": {"stop_reason": anthropic_stop(&finish), "stop_sequence": null},
               "usage": anthropic_usage(turn)}),
    ));
    events.push(event("message_stop", json!({"type": "message_stop"})));
    let limit = turn.max_chunks.unwrap_or(usize::MAX);
    events
        .iter()
        .take(limit)
        .map(|(name, value)| format!("event: {name}\ndata: {value}\n\n"))
        .collect()
}

/// Cuerpo JSON de Chat Completions (sin `stream`).
pub fn openai_completion(turn: &Turn, model: &str, turn_index: u64) -> Value {
    let tool_calls: Vec<Value> = turn
        .tool_calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            json!({
                "id": call_id(turn_index, index, call),
                "type": "function",
                "function": {"name": call.name, "arguments": call.arguments},
            })
        })
        .collect();
    let content = if turn.content.is_empty() {
        Value::Null
    } else {
        json!(turn.content)
    };
    let mut message = json!({"role": "assistant", "content": content});
    if !tool_calls.is_empty() {
        if let Some(object) = message.as_object_mut() {
            object.insert("tool_calls".to_string(), Value::Array(tool_calls));
        }
    }
    json!({
        "id": format!("chatcmpl-fake-{turn_index}"),
        "object": "chat.completion",
        "created": CREATED,
        "model": model,
        "choices": [{"index": 0, "message": message, "finish_reason": turn.finish()}],
        "usage": openai_usage(turn),
    })
}

/// Cuerpo JSON de Messages (sin `stream`).
pub fn anthropic_message(turn: &Turn, model: &str, turn_index: u64) -> Value {
    let mut content = Vec::new();
    if !turn.content.is_empty() {
        content.push(json!({"type": "text", "text": turn.content}));
    }
    for (index, call) in turn.tool_calls.iter().enumerate() {
        let input = serde_json::from_str::<Value>(&call.arguments)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        content.push(json!({
            "type": "tool_use",
            "id": call_id(turn_index, index, call),
            "name": call.name,
            "input": input,
        }));
    }
    json!({
        "id": format!("msg_fake_{turn_index}"),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": anthropic_stop(&turn.finish()),
        "stop_sequence": null,
        "usage": {"input_tokens": 10, "output_tokens": turn.output_tokens()},
    })
}

fn default_error(status: u16) -> (&'static str, &'static str) {
    match status {
        401 => ("authentication_error", "Invalid API key."),
        403 => ("permission_error", "The API key does not have access."),
        404 => ("not_found_error", "The requested model does not exist."),
        413 => ("request_too_large", "Request too large."),
        429 => ("rate_limit_error", "Rate limit reached."),
        500 => (
            "api_error",
            "The server had an error while processing your request.",
        ),
        _ => (
            "invalid_request_error",
            "The request could not be processed.",
        ),
    }
}

/// Cuerpo de error con la forma del proveedor elegido por la ruta.
pub fn error_body(route: Route, turn: &Turn) -> Value {
    let status = turn.status_code();
    let (kind, default_message) = default_error(status);
    let message = if turn.error.trim().is_empty() {
        default_message
    } else {
        turn.error.trim()
    };
    let kind = if turn.error_type.trim().is_empty() {
        kind
    } else {
        turn.error_type.trim()
    };
    match route {
        Route::OpenAi => json!({
            "error": {
                "message": message,
                "type": kind,
                "param": null,
                "code": null,
            }
        }),
        Route::Anthropic => json!({
            "type": "error",
            "error": {"type": kind, "message": message},
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> Turn {
        Turn {
            content: "Hola mundo".to_string(),
            ..Turn::default()
        }
    }

    fn text_from_body(body: &str) -> String {
        body.lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|data| serde_json::from_str::<Value>(data).ok())
            .filter_map(|value| {
                value["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect()
    }

    #[test]
    fn openai_text_is_chunked_and_done() {
        let body = openai_sse(&turn(), "m", 4, 1);
        assert_eq!(text_from_body(&body), "Hola mundo");
        assert!(body.contains("data: [DONE]\n\n"));
        assert!(body.contains("\"finish_reason\":\"stop\""));
        assert!(!body.contains("\"role\""));
    }

    #[test]
    fn openai_tool_calls_accumulate_arguments() {
        let turn = Turn {
            tool_calls: vec![ToolCallSpec {
                name: "web_search".to_string(),
                arguments: "{\"query\":\"ezagent\"}".to_string(),
                ..ToolCallSpec::default()
            }],
            ..Turn::default()
        };
        let body = openai_sse(&turn, "m", 2, 7);
        let deltas: Vec<Value> = body
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|data| serde_json::from_str::<Value>(data).ok())
            .collect();
        let mut arguments = String::new();
        let mut id = String::new();
        let mut name = String::new();
        for value in &deltas {
            for call in value["choices"][0]["delta"]["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                if let Some(value) = call["id"].as_str() {
                    id = value.to_string();
                }
                if let Some(value) = call["function"]["name"].as_str() {
                    name = value.to_string();
                }
                if let Some(value) = call["function"]["arguments"].as_str() {
                    arguments.push_str(value);
                }
            }
        }
        assert_eq!(id, "call_fake_7_0");
        assert_eq!(name, "web_search");
        assert_eq!(arguments, "{\"query\":\"ezagent\"}");
        assert!(body.contains("\"finish_reason\":\"tool_calls\""));
    }

    #[test]
    fn max_chunks_cuts_before_done() {
        let turn = Turn {
            content: "abcdefghijkl".to_string(),
            max_chunks: Some(1),
            ..Turn::default()
        };
        let body = openai_sse(&turn, "m", 2, 1);
        assert_eq!(body.lines().count(), 2);
        assert!(!body.contains("[DONE]"));
        assert!(!body.contains("finish_reason\":\"stop"));
    }

    #[test]
    fn anthropic_text_and_tool_events() {
        let turn = Turn {
            content: "Hola".to_string(),
            tool_calls: vec![ToolCallSpec {
                name: "delegate".to_string(),
                arguments: "{\"action\":\"run\"}".to_string(),
                ..ToolCallSpec::default()
            }],
            ..Turn::default()
        };
        let body = anthropic_sse(&turn, "claude", 2, 3);
        assert!(body.starts_with("event: message_start"));
        assert!(body.contains("\"type\":\"text_delta\",\"text\":\"Ho\""));
        assert!(body.contains("\"type\":\"tool_use\""));
        assert!(body.contains("\"type\":\"input_json_delta\""));
        assert!(body.contains("\"stop_reason\":\"tool_use\""));
        assert!(body
            .trim_end()
            .ends_with("data: {\"type\":\"message_stop\"}"));
    }

    #[test]
    fn anthropic_cut_has_no_message_stop() {
        let turn = Turn {
            content: "abcdef".to_string(),
            max_chunks: Some(2),
            ..Turn::default()
        };
        let body = anthropic_sse(&turn, "claude", 1, 0);
        assert!(!body.contains("message_stop"));
        assert!(!body.contains("message_delta"));
    }

    #[test]
    fn non_stream_shapes() {
        let openai = openai_completion(&turn(), "m", 1);
        assert_eq!(openai["choices"][0]["message"]["content"], "Hola mundo");
        assert_eq!(openai["choices"][0]["finish_reason"], "stop");
        let anthropic = anthropic_message(&turn(), "claude", 1);
        assert_eq!(anthropic["content"][0]["text"], "Hola mundo");
        assert_eq!(anthropic["stop_reason"], "end_turn");
        assert_eq!(anthropic["type"], "message");
    }

    #[test]
    fn error_bodies_match_each_provider() {
        let turn = Turn {
            status: 429,
            retry_after: Some(3),
            ..Turn::default()
        };
        assert_eq!(
            error_body(Route::OpenAi, &turn)["error"]["type"],
            "rate_limit_error"
        );
        let anthropic = error_body(Route::Anthropic, &turn);
        assert_eq!(anthropic["type"], "error");
        assert_eq!(anthropic["error"]["type"], "rate_limit_error");
        assert!(anthropic["error"]["message"]
            .as_str()
            .expect("mensaje")
            .contains("Rate limit"));
    }
}
