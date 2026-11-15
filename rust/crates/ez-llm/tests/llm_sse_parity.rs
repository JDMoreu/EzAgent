//! Paridad de `llm_sse.py` contra `parity/fixtures/llm_sse.jsonl`.
//!
//! Los casos `openai_stream` y `anthropic_stream` levantan un servidor HTTP
//! local de una sola respuesta con el cuerpo crudo del caso.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread::{self, JoinHandle};

use ez_llm::anthropic::{iter_sse, translate_events, AnthropicChatClient};
use ez_llm::errors::{friendly_llm_error, ErrorKind, LlmError};
use ez_llm::openai::{Chunk, OpenAiChatClient};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/llm_sse.jsonl";

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

// ------------------------------------------------------------------ servidor

struct TestServer {
    url: String,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn serve(body: Vec<u8>, status: u16, headers: &[(String, String)], delivery: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("puerto de prueba libre");
        let address = listener.local_addr().expect("dirección del servidor");
        let delivery = delivery.to_string();
        let headers = headers.to_vec();
        let handle = thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            read_request(&mut socket);
            let response = build_response(status, &headers, &body);
            if delivery == "byte" {
                for byte in response {
                    if socket.write_all(&[byte]).is_err() {
                        return;
                    }
                    let _ = socket.flush();
                }
            } else if delivery == "line" {
                for line in split_keepends(&response) {
                    if socket.write_all(line).is_err() {
                        return;
                    }
                    let _ = socket.flush();
                }
            } else {
                let _ = socket.write_all(&response);
                let _ = socket.flush();
            }
        });
        Self {
            url: format!("http://{address}"),
            handle: Some(handle),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn split_keepends(bytes: &[u8]) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            parts.push(&bytes[start..=index]);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        parts.push(&bytes[start..]);
    }
    parts
}

fn build_response(status: u16, headers: &[(String, String)], body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    {
        let kind = if status >= 400 {
            "application/json"
        } else {
            "text/event-stream"
        };
        response.push_str(&format!("content-type: {kind}\r\n"));
    }
    response.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn read_request(socket: &mut TcpStream) {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match socket.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => data.extend_from_slice(&buffer[..read]),
        }
        let Some(head) = data.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let length = content_length(&data[..head]);
        if data.len() >= head + 4 + length {
            return;
        }
    }
}

fn content_length(head: &[u8]) -> usize {
    let text = String::from_utf8_lossy(head);
    text.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn case_body(input: &Value) -> Vec<u8> {
    let raw = string_field(input, "raw").as_bytes();
    let cut = input.get("cut_at").and_then(Value::as_u64);
    match cut {
        Some(cut) => raw[..(cut as usize).min(raw.len())].to_vec(),
        None => raw.to_vec(),
    }
}

fn case_headers(input: &Value) -> Vec<(String, String)> {
    input
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .iter()
                .map(|(name, value)| (name.clone(), value.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------- trozos a JSON

fn chunk_record(chunk: &Chunk) -> Value {
    if chunk.choices_empty {
        let mut record = Map::new();
        record.insert("choices".to_string(), json!([]));
        if let Some(usage) = &chunk.usage {
            record.insert("usage".to_string(), usage.clone());
        }
        return Value::Object(record);
    }
    let calls: Vec<Value> = chunk
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "index": call.index,
                "id": call.id,
                "name": call.name,
                "arguments": call.arguments,
            })
        })
        .collect();
    let mut record = json!({
        "choices": [{
            "content": chunk.content,
            "finish_reason": chunk.finish_reason,
            "tool_calls": calls,
        }],
    });
    if let Some(usage) = &chunk.usage {
        record["usage"] = usage.clone();
    }
    record
}

fn collect(chunks: Vec<Chunk>) -> Value {
    let records: Vec<Value> = chunks.iter().map(chunk_record).collect();
    let mut text = String::new();
    let mut finish_reasons: Vec<String> = Vec::new();
    let mut calls: BTreeMap<usize, (String, String, String)> = BTreeMap::new();
    for chunk in &chunks {
        if let Some(content) = &chunk.content {
            text.push_str(content);
        }
        if let Some(reason) = &chunk.finish_reason {
            finish_reasons.push(reason.clone());
        }
        for call in &chunk.tool_calls {
            let entry = calls.entry(call.index).or_default();
            if let Some(id) = &call.id {
                entry.0 = id.clone();
            }
            if let Some(name) = &call.name {
                entry.1.push_str(name);
            }
            entry.2.push_str(&call.arguments);
        }
    }
    let tool_calls: Vec<Value> = calls
        .into_iter()
        .map(|(index, (id, name, arguments))| {
            json!({"index": index, "id": id, "name": name, "arguments": arguments})
        })
        .collect();
    let final_finish_reason = finish_reasons.last().cloned();
    json!({
        "chunks": records,
        "text": text,
        "finish_reasons": finish_reasons,
        "final_finish_reason": final_finish_reason,
        "tool_calls": tool_calls,
    })
}

fn http_status_class(status: Option<u16>) -> &'static str {
    match status {
        Some(400) => "BadRequestError",
        Some(401) => "AuthenticationError",
        Some(403) => "PermissionDeniedError",
        Some(404) => "NotFoundError",
        Some(422) => "UnprocessableEntityError",
        Some(429) => "RateLimitError",
        Some(status) if status >= 500 => "InternalServerError",
        _ => "APIStatusError",
    }
}

fn openai_error(error: &LlmError) -> String {
    let class = match error.kind {
        ErrorKind::Http => http_status_class(error.status),
        ErrorKind::Api => "APIError",
        ErrorKind::Protocol => "JSONDecodeError",
        ErrorKind::Connection => "APIConnectionError",
        ErrorKind::Timeout => "APITimeoutError",
        _ => "APIError",
    };
    format!("{class}: {}", error.message)
}

fn anthropic_error(error: &LlmError) -> String {
    let class = match error.kind {
        ErrorKind::Attribute => "AttributeError",
        ErrorKind::Value => "ValueError",
        ErrorKind::Protocol => "JSONDecodeError",
        _ => "RuntimeError",
    };
    format!("{class}: {}", error.message)
}

// ------------------------------------------------------------------- casos

fn run_openai(input: &Value) -> Result<Value, String> {
    let status = input.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
    let delivery = input
        .get("delivery")
        .and_then(Value::as_str)
        .unwrap_or("whole");
    let server = TestServer::serve(case_body(input), status, &case_headers(input), delivery);
    let client = OpenAiChatClient::new("clave-falsa", &format!("{}/v1", server.url), &[], 5.0);
    let request = json!({
        "model": "m",
        "messages": [{"role": "user", "content": "x"}],
        "stream": true,
        "max_tokens": 10,
    });
    let mut stream = client.stream(&request).map_err(|e| openai_error(&e))?;
    let mut chunks = Vec::new();
    for chunk in stream.by_ref() {
        chunks.push(chunk.map_err(|e| openai_error(&e))?);
    }
    stream.close();
    Ok(collect(chunks))
}

fn run_anthropic(input: &Value) -> Result<Value, String> {
    let status = input.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
    let delivery = input
        .get("delivery")
        .and_then(Value::as_str)
        .unwrap_or("whole");
    let server = TestServer::serve(case_body(input), status, &case_headers(input), delivery);
    let client = AnthropicChatClient::new("clave-falsa", &server.url, &[], 5.0, false);
    let request = json!({
        "model": "m",
        "messages": [{"role": "user", "content": "x"}],
        "stream": true,
        "max_tokens": 10,
    });
    let mut stream = client.stream(&request).map_err(|e| anthropic_error(&e))?;
    let mut chunks = Vec::new();
    for chunk in stream.by_ref() {
        chunks.push(chunk.map_err(|e| anthropic_error(&e))?);
    }
    stream.close();
    Ok(collect(chunks))
}

fn run_iter_sse(input: &Value) -> Value {
    let lines: Vec<String> = input
        .get("lines")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let text = lines.join("\n");
    let events: Vec<Value> = iter_sse(std::io::Cursor::new(text.into_bytes()))
        .filter_map(Result::ok)
        .collect();
    json!({"events": events})
}

fn run_translate(input: &Value) -> Result<Value, String> {
    let events = input
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let chunks: Vec<Chunk> = translate_events(events.into_iter().map(Ok))
        .collect::<Result<_, _>>()
        .map_err(|error: LlmError| anthropic_error(&error))?;
    Ok(collect(chunks))
}

fn llm_error_of(input: &Value) -> LlmError {
    let class = string_field(input, "class");
    let message = string_field(input, "message");
    if class == "LLMTurnTimeout" {
        return LlmError::turn_timeout(message);
    }
    if class.contains("Connection") || class.contains("Timeout") || class.contains("ConnectError") {
        return LlmError::connection(message);
    }
    let status = input
        .get("status")
        .and_then(Value::as_u64)
        .map(|status| status as u16);
    let mut error = match status {
        Some(status) => LlmError::http(status, message),
        None => LlmError::other(message),
    };
    if let Some(headers) = input.get("headers").and_then(Value::as_object) {
        if let Some(after) = headers.get("retry-after") {
            let text = match after {
                Value::String(text) => text.clone(),
                Value::Bool(flag) => if *flag { "True" } else { "False" }.to_string(),
                other => other.to_string(),
            };
            error = error.with_retry_after(Some(text));
        }
    }
    error
}

fn produced(case: &FixtureCase) -> Result<Value, String> {
    match case.function.as_str() {
        "iter_sse" => Ok(run_iter_sse(&case.input)),
        "translate_events" => run_translate(&case.input),
        "openai_stream" => run_openai(&case.input),
        "anthropic_stream" => run_anthropic(&case.input),
        "friendly_llm_error" => {
            let label = string_field(&case.input, "label");
            let model = string_field(&case.input, "model");
            let error = llm_error_of(&case.input);
            Ok(json!({"text": friendly_llm_error(&error, label, model)}))
        }
        other => panic!("función desconocida: {other}"),
    }
}

#[test]
fn every_llm_sse_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de llm_sse legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        match (produced(case), &case.error) {
            (Ok(value), None) => assert_eq!(value, case.output, "caso {}", case.case),
            (Err(error), Some(expected)) => assert_eq!(&error, expected, "caso {}", case.case),
            (Ok(value), Some(expected)) => panic!(
                "caso {}: Python falla con {expected:?} y Rust devolvió {value}",
                case.case
            ),
            (Err(error), None) => panic!("caso {}: fallo inesperado: {error}", case.case),
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 138, "muy pocos casos: {cases_run}");
}
