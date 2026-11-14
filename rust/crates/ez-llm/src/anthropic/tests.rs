//! Pruebas de `anthropic` (F4-06).

use std::io::Cursor;

use serde_json::json;

use super::*;
use crate::openai::Chunk;
use crate::sse::testing::FakeServer;

fn translated(events: Vec<Value>) -> Vec<Chunk> {
    translate_events(events.into_iter().map(Ok))
        .collect::<Result<Vec<_>, _>>()
        .expect("eventos válidos")
}

#[test]
fn builds_the_messages_url() {
    assert_eq!(
        messages_url("https://api.anthropic.com/v1/"),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(
        messages_url("https://proxy.local"),
        "https://proxy.local/v1/messages"
    );
    assert_eq!(
        messages_url("https://x.io/anthropic"),
        "https://x.io/anthropic/v1/messages"
    );
    assert_eq!(
        messages_url("  https://x.io/v1  "),
        "https://x.io/v1/messages"
    );
    assert_eq!(messages_url(""), "/v1/messages");
}

#[test]
fn converts_system_turns_and_merges_roles() {
    let messages = vec![
        json!({"role": "system", "content": "base"}),
        json!({"role": "system", "content": "extra"}),
        json!({"role": "user", "content": "hola"}),
        json!({"role": "user", "content": "otra"}),
        json!({"role": "assistant", "content": "", "tool_calls": [
            {"id": "c1", "function": {"name": "web", "arguments": "{\"q\": \"x\"}"}}
        ]}),
        json!({"role": "tool", "tool_call_id": "c1", "content": "ok"}),
        json!({"role": "system", "content": "aviso"}),
    ];
    let (system, out) = convert_messages(&messages);
    assert_eq!(system, "base\n\nextra");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0]["role"], json!("user"));
    assert_eq!(out[0]["content"].as_array().expect("bloques").len(), 2);
    assert_eq!(
        out[1]["content"][0],
        json!({"type": "tool_use", "id": "c1", "name": "web", "input": {"q": "x"}})
    );
    assert_eq!(
        out[2]["content"][0],
        json!({"type": "tool_result", "tool_use_id": "c1", "content": "ok"})
    );
    assert_eq!(
        out[2]["content"][1]["text"],
        json!("[Aviso del sistema]\naviso")
    );
}

#[test]
fn converts_tools_and_skips_invalid_ones() {
    let tools = json!([
        {"type": "function", "function": {"name": "f", "description": "d", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": ""}},
        {"name": "g"},
        {"type": "function", "function": "nope"},
    ]);
    let converted = convert_tools(Some(&tools));
    assert_eq!(converted.len(), 2);
    assert_eq!(converted[0]["name"], json!("f"));
    assert_eq!(converted[1]["name"], json!("g"));
    assert_eq!(
        converted[1]["input_schema"],
        json!({"type": "object", "properties": {}})
    );
}

#[test]
fn builds_the_default_body_and_never_sends_stream_false() {
    let client =
        AnthropicChatClient::new("clave", "https://api.anthropic.com/v1", &[], 60.0, false);
    let request = json!({"model": "claude", "messages": [], "stream": false});
    let body = client.build_body(&request);
    assert_eq!(body["model"], json!("claude"));
    assert_eq!(body["max_tokens"], json!(1024));
    assert_eq!(body["stream"], json!(true));
    assert!(body.get("system").is_none());
    assert!(body.get("tools").is_none());
}

#[test]
fn prompt_cache_marks_the_system_and_the_last_tool() {
    let client = AnthropicChatClient::new("clave", "https://x", &[], 60.0, true);
    let request = json!({
        "model": "claude",
        "max_tokens": 100,
        "temperature": 0.3,
        "messages": [{"role": "system", "content": "eres"}, {"role": "user", "content": "hola"}],
        "tools": [{"type": "function", "function": {"name": "f"}}],
    });
    let body = client.build_body(&request);
    assert_eq!(
        body["system"][0]["cache_control"]["type"],
        json!("ephemeral")
    );
    assert_eq!(
        body["tools"][0]["cache_control"]["type"],
        json!("ephemeral")
    );
    assert_eq!(body["tool_choice"], json!({"type": "auto"}));
    assert_eq!(body["temperature"], json!(0.3));
}

#[test]
fn invalid_tool_arguments_become_an_empty_object() {
    let arguments = [
        json!("{no json"),
        json!("[1,2]"),
        json!("\"texto\""),
        json!("null"),
        Value::Null,
        json!(5),
    ];
    for arguments in arguments {
        let (_, messages) = convert_messages(&[json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{"id": "c", "function": {"name": "f", "arguments": arguments}}],
        })]);
        assert_eq!(
            messages[0]["content"],
            json!([{"type": "tool_use", "id": "c", "name": "f", "input": {}}])
        );
    }
}

#[test]
fn ignores_noise_in_one_sse_block() {
    let lines = [
        "event: message_start",
        "data: {\"type\": \"a\"}",
        "",
        ": ping",
        "data:",
        "data: [DONE]",
        "data: {roto",
        "data: [1]",
        "data:{\"type\":\"b\"}",
        "  data: {\"type\": \"c\"}  ",
    ];
    let body = lines.join("\n");
    let kinds: Vec<String> = iter_sse(Cursor::new(body.as_bytes()))
        .filter_map(Result::ok)
        .filter_map(|event| {
            event
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    assert_eq!(kinds, ["a", "b", "c"]);
}

#[test]
fn translates_text_tool_calls_and_the_stop_event() {
    let chunks = translated(vec![
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "Hola"}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": " mundo"}}),
        json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "t1", "name": "web"}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"q\":1}"}}),
        json!({"type": "content_block_stop", "index": 1}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 2}}),
        json!({"type": "message_stop"}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "ignorado"}}),
    ]);
    assert_eq!(chunks.len(), 5);
    assert_eq!(chunks[0].content.as_deref(), Some("Hola"));
    assert_eq!(chunks[1].content.as_deref(), Some(" mundo"));
    assert_eq!(chunks[2].tool_calls[0].id.as_deref(), Some("t1"));
    assert_eq!(chunks[2].tool_calls[0].name.as_deref(), Some("web"));
    assert_eq!(chunks[3].tool_calls[0].arguments, "{\"q\":1}");
    assert_eq!(chunks[4].finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(chunks[4].usage.as_ref().expect("uso")["output_tokens"], 2);
}

#[test]
fn tool_without_arguments_receives_an_empty_object() {
    let chunks = translated(vec![
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "t", "name": "f"}}),
        json!({"type": "content_block_stop", "index": 0}),
    ]);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[1].tool_calls[0].arguments, "{}");
}

#[test]
fn error_events_become_errors_and_stop_the_stream() {
    let mut events = translate_events(
        vec![json!({"type": "error", "error": {"type": "overloaded", "message": "busy"}})]
            .into_iter()
            .map(Ok),
    );
    let error = events
        .next()
        .expect("un evento")
        .expect_err("el evento es un error");
    assert_eq!(error.message, "Anthropic: overloaded: busy");
    assert!(events.next().is_none());
}

#[test]
fn streams_from_a_real_http_server() {
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hola\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let server = FakeServer::serve("200 OK", &[("content-type", "text/event-stream")], body);
    let client = AnthropicChatClient::new("clave", &server.base_url, &[], 5.0, false);
    let request = json!({
        "model": "claude",
        "max_tokens": 64,
        "messages": [{"role": "user", "content": "hola"}],
    });
    let chunks = match client.stream(&request) {
        Ok(stream) => stream.collect::<Result<Vec<_>, _>>().expect("flujo válido"),
        Err(error) => panic!("no debía fallar: {error}"),
    };
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].content.as_deref(), Some("Hola"));
    assert_eq!(chunks[1].finish_reason.as_deref(), Some("stop"));
}

#[test]
fn http_errors_use_the_python_text() {
    let server = FakeServer::serve("401 Unauthorized", &[], "nope");
    let client = AnthropicChatClient::new("clave", &server.base_url, &[], 5.0, false);
    let request = json!({"model": "m", "messages": []});
    let error = match client.stream(&request) {
        Ok(_) => panic!("el servidor debía rechazar la petición"),
        Err(error) => error,
    };
    assert_eq!(error.status, Some(401));
    assert_eq!(
        error.message,
        "La API compatible con Anthropic respondió 401: nope"
    );
    assert!(crate::errors::friendly_llm_error(&error, "Anthropic", "m")
        .contains("La clave de Anthropic"));
}

#[test]
fn condenser_reads_a_short_streamed_reply() {
    let body = concat!(
        "data: {\"type\":\"content_block_start\",\"index\":0,",
        "\"content_block\":{\"type\":\"text\",\"text\":\"3\"}}\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let server = FakeServer::serve("200 OK", &[], body);
    let client = AnthropicChatClient::new("clave", &server.base_url, &[], 5.0, false);
    let messages = [json!({"role": "user", "content": "línea"})];
    assert_eq!(
        client
            .complete_text("anthropic", "claude", &messages, 8)
            .expect("respuesta del condensador"),
        "3"
    );
}
