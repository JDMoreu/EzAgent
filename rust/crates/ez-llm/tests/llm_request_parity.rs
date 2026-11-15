//! Paridad de `llm_request.py` contra `parity/fixtures/llm_request.jsonl`.
//!
//! Los casos `turn` corren el `Messenger` completo contra un servidor HTTP
//! local que captura cada cuerpo y responde con el texto del caso.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ez_core::config::VoiceConfig;
use ez_llm::anthropic::{convert_messages, convert_tools, messages_url, AnthropicChatClient};
use ez_llm::messenger::{
    AgentRuntimeApi, AskMode, Messenger, MessengerCallbacks, RuntimeFailure, BASE_TOOLS,
};
use ez_parity::fixtures::{self, FixtureCase};
use regex::Regex;
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/llm_request.jsonl";

// --------------------------------------------------------------- servidor

struct RecorderServer {
    url: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    handle: Option<JoinHandle<()>>,
}

impl RecorderServer {
    fn start(reply: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("puerto de prueba libre");
        listener
            .set_nonblocking(true)
            .expect("servidor sin bloqueo");
        let address = listener.local_addr().expect("dirección del servidor");
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_requests = requests.clone();
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let _ = socket.set_nonblocking(false);
                        let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                        let Some((path, body)) = read_request(&mut socket) else {
                            continue;
                        };
                        thread_requests.lock().expect("peticiones").push(json!({
                            "path": path,
                            "body": body,
                        }));
                        let data = if path.ends_with("/messages") {
                            anthropic_sse(&reply)
                        } else {
                            openai_sse(&reply)
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                             cache-control: no-cache\r\ncontent-length: {}\r\n\
                             connection: close\r\n\r\n{data}",
                            data.len()
                        );
                        let _ = socket.write_all(response.as_bytes());
                        let _ = socket.flush();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url: format!("http://{address}"),
            stop,
            requests,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("peticiones").clone()
    }
}

impl Drop for RecorderServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request(socket: &mut TcpStream) -> Option<(String, Value)> {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 1024];
    let head = loop {
        let read = socket.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..read]);
        if let Some(head) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break head;
        }
    };
    let length = content_length(&data[..head]);
    while data.len() < head + 4 + length {
        let read = socket.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
    }
    let path = String::from_utf8_lossy(&data[..head])
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string();
    let body = if data.len() >= head + 4 {
        serde_json::from_slice(&data[head + 4..]).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    Some((path, body))
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

fn openai_sse(text: &str) -> String {
    let payload = json!({
        "id": "chatcmpl-local",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "local",
        "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": "stop"}],
    });
    format!("data: {payload}\n\ndata: [DONE]\n\n")
}

fn anthropic_sse(text: &str) -> String {
    let events = [
        json!({"type": "message_start", "message": {"id": "msg_local", "type": "message"}}),
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": text}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 1}}),
        json!({"type": "message_stop"}),
    ];
    events
        .iter()
        .map(|event| {
            let kind = event["type"].as_str().unwrap_or("");
            format!("event: {kind}\ndata: {event}\n\n")
        })
        .collect()
}

// ---------------------------------------------------------------- runtime

struct FakeRuntime {
    prompt: String,
    volatile: String,
    tools: Vec<Value>,
    prompt_parts: bool,
    begin_fails: bool,
    tools_for_fails: bool,
    remaining: Option<i64>,
    executed: Mutex<Vec<Value>>,
    recorded: Mutex<Vec<String>>,
    turns: Mutex<Vec<String>>,
}

impl FakeRuntime {
    fn new(spec: &Value) -> Self {
        let tools = spec
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| {
                vec![json!({
                    "type": "function",
                    "function": {
                        "name": "get_datetime",
                        "description": "Hora actual",
                        "parameters": {"type": "object", "properties": {}},
                    }
                })]
            });
        Self {
            prompt: spec
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("PROMPT-SISTEMA")
                .to_string(),
            volatile: spec
                .get("volatile")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            tools,
            prompt_parts: spec
                .get("prompt_parts")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            begin_fails: spec
                .get("begin_fails")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tools_for_fails: spec
                .get("tools_for_fails")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            remaining: spec.get("remaining").and_then(Value::as_i64),
            executed: Mutex::new(Vec::new()),
            recorded: Mutex::new(Vec::new()),
            turns: Mutex::new(Vec::new()),
        }
    }
}

impl AgentRuntimeApi for FakeRuntime {
    fn begin_turn(&self, text: &str) -> Result<(), RuntimeFailure> {
        if self.begin_fails {
            return Err(RuntimeFailure::new("RuntimeError", "memoria corrupta"));
        }
        self.turns.lock().expect("turnos").push(text.to_string());
        Ok(())
    }

    fn system_prompt(&self) -> String {
        self.prompt.clone()
    }

    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        if !self.prompt_parts {
            return Ok((self.prompt.clone(), String::new()));
        }
        Ok((self.prompt.clone(), self.volatile.clone()))
    }

    fn tools_for(&self, _text: &str) -> Result<Vec<Value>, RuntimeFailure> {
        if self.tools_for_fails {
            return Err(RuntimeFailure::new(
                "TypeError",
                "expected string or bytes-like object, got 'NoneType'",
            ));
        }
        Ok(self.tools.clone())
    }

    fn base_tool_schemas(&self) -> Result<Vec<Value>, RuntimeFailure> {
        Ok(BASE_TOOLS
            .iter()
            .map(|name| {
                json!({"type": "function",
                    "function": {"name": name, "parameters": {"type": "object"}}})
            })
            .collect())
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        self.remaining
    }

    fn execute(&self, name: &str, raw_arguments: &str) -> Result<Value, RuntimeFailure> {
        self.executed
            .lock()
            .expect("ejecutadas")
            .push(json!([name, raw_arguments]));
        Ok(json!({"ok": true, "data": "resultado"}))
    }

    fn render_result(&self, result: Value) -> String {
        ez_core::pyjson::dumps_with(&result, &ez_core::pyjson::DumpOptions::utf8())
    }

    fn record_assistant(&self, text: &str) {
        self.recorded
            .lock()
            .expect("respuestas")
            .push(text.to_string());
    }
}

// ------------------------------------------------------------------ casos

fn settings_of(input: &Value, base_url: &str) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    if let Some(settings) = input.get("settings").and_then(Value::as_object) {
        for (key, value) in settings {
            let value = if key == "llm_base_url" {
                Value::String(base_url.to_string())
            } else {
                value.clone()
            };
            cfg.set(key, value);
        }
    }
    cfg
}

fn normalize_rounds(rounds: &[String]) -> Vec<String> {
    let pattern = Regex::new(r"\d+ ms").expect("patrón de milisegundos");
    rounds
        .iter()
        .map(|round| pattern.replace_all(round, "0 ms").to_string())
        .collect()
}

fn run_turn(case: &FixtureCase) -> Value {
    let input = &case.input;
    let reply = input
        .get("emit")
        .and_then(Value::as_str)
        .unwrap_or("ok")
        .to_string();
    let server = RecorderServer::start(reply);
    let settings = input.get("settings").cloned().unwrap_or(Value::Null);
    let native = settings.get("llm_provider").and_then(Value::as_str) == Some("anthropic")
        && settings.get("llm_api_style").and_then(Value::as_str) == Some("anthropic");
    let base = if native {
        server.url.clone()
    } else {
        format!("{}/v1", server.url)
    };
    let cfg = settings_of(input, &base);
    let runtime = match input.get("runtime") {
        None | Some(Value::Null) => None,
        Some(spec) => Some(Arc::new(FakeRuntime::new(spec)) as Arc<dyn AgentRuntimeApi>),
    };
    let messenger = Messenger::new(cfg, MessengerCallbacks::default(), runtime.clone());
    if let Some(history) = input.get("history").and_then(Value::as_array) {
        messenger.import_context(
            Vec::new(),
            input.get("summary").and_then(Value::as_str).unwrap_or(""),
            "",
        );
        messenger.set_messages(history.clone());
    }
    let mode = if input.get("mode").and_then(Value::as_str) == Some("chat") {
        AskMode::Chat
    } else {
        AskMode::Voice
    };
    let user = input.get("user").and_then(Value::as_str).unwrap_or("hola");
    let pieces: Vec<String> = messenger
        .ask_stream(user, mode)
        .collect::<Result<_, _>>()
        .expect("turno sin error");
    let answer = pieces.concat();
    json!({
        "requests": server.requests(),
        "pieces": pieces,
        "answer": answer,
        "history": messenger.messages(),
        "summary": messenger.summary(),
        "last_rounds": normalize_rounds(&messenger.last_rounds()),
    })
}

fn produced(case: &FixtureCase) -> Value {
    match case.function.as_str() {
        "messages_url" => {
            let base = case
                .input
                .get("base_url")
                .and_then(Value::as_str)
                .unwrap_or("");
            json!({"url": messages_url(base)})
        }
        "convert_messages" => {
            let messages = case
                .input
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let (system, converted) = convert_messages(&messages);
            json!({"system": system, "messages": converted})
        }
        "convert_tools" => {
            let tools = case.input.get("tools");
            json!({"tools": convert_tools(tools)})
        }
        "build_body" => {
            let prompt_cache = case
                .input
                .get("prompt_cache")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let request = case.input.get("request").cloned().unwrap_or(Value::Null);
            let client = AnthropicChatClient::new(
                "clave-falsa",
                "https://api.anthropic.com/v1",
                &[],
                60.0,
                prompt_cache,
            );
            json!({"body": client.build_body(&request)})
        }
        "turn" => run_turn(case),
        other => panic!("función desconocida: {other}"),
    }
}

#[test]
fn every_llm_request_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de llm_request legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        assert_eq!(case.error, None, "caso {}", case.case);
        assert_eq!(produced(case), case.output, "caso {}", case.case);
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 82, "muy pocos casos: {cases_run}");
}
