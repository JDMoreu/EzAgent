//! Cliente y runtime guionizados para la paridad de `llm_tools`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ez_core::config::VoiceConfig;
use ez_llm::errors::LlmError;
use ez_llm::messenger::{
    AgentRuntimeApi, LlmClient, Messenger, MessengerCallbacks, RuntimeFailure, BASE_TOOLS,
};
use ez_llm::openai::{ChatStream, Chunk, ToolCallDelta};
use serde_json::{json, Map, Value};

pub fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

pub fn text_of(value: &Value, key: &str) -> String {
    field(value, key).as_str().unwrap_or("").to_string()
}

pub fn strings_of(value: &Value, key: &str) -> Vec<String> {
    field(value, key)
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------- cliente guionizado

fn script_error(text: &str) -> LlmError {
    let (class, message) = text.split_once(": ").unwrap_or(("RuntimeError", text));
    if class.contains("Connection") || class.contains("ConnectError") {
        LlmError::connection(message)
    } else if class.contains("Timeout") {
        LlmError::timeout(message)
    } else {
        LlmError::other(message)
    }
}

fn chunk_of(record: &Value) -> Chunk {
    let choices = field(record, "choices")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let Some(choice) = choices.first() else {
        return Chunk {
            choices_empty: true,
            ..Chunk::default()
        };
    };
    let calls = field(choice, "tool_calls")
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|call| ToolCallDelta {
                    index: field(call, "index").as_u64().unwrap_or(0) as usize,
                    id: field(call, "id").as_str().map(str::to_string),
                    name: field(call, "name").as_str().map(str::to_string),
                    arguments: text_of(call, "arguments"),
                })
                .collect()
        })
        .unwrap_or_default();
    Chunk {
        content: field(choice, "content").as_str().map(str::to_string),
        tool_calls: calls,
        finish_reason: field(choice, "finish_reason").as_str().map(str::to_string),
        usage: None,
        choices_empty: false,
    }
}

pub struct ScriptedStream {
    pub chunks: VecDeque<Chunk>,
    pub boom: Option<LlmError>,
    pub closed: Arc<Mutex<bool>>,
}

impl Iterator for ScriptedStream {
    type Item = Result<Chunk, LlmError>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chunk) = self.chunks.pop_front() {
            return Some(Ok(chunk));
        }
        self.boom.take().map(Err)
    }
}

impl ChatStream for ScriptedStream {
    fn close(&mut self) {
        *self.closed.lock().expect("cierre del flujo") = true;
        self.chunks.clear();
        self.boom = None;
    }
}

pub struct ScriptedClient {
    scripts: Mutex<VecDeque<Value>>,
    requests: Mutex<Vec<Value>>,
    streams: Mutex<Vec<Arc<Mutex<bool>>>>,
    pub closed: AtomicBool,
    slow_first_call: Option<Duration>,
}

impl ScriptedClient {
    pub fn new(scripts: Vec<Value>, slow_first_call: Option<Duration>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
            streams: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            slow_first_call,
        }
    }

    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("peticiones").clone()
    }

    pub fn streams_summary(&self) -> (usize, usize) {
        let streams = self.streams.lock().expect("flujos");
        let closed = streams
            .iter()
            .filter(|closed| *closed.lock().expect("cierre"))
            .count();
        (closed, streams.len())
    }
}

impl LlmClient for ScriptedClient {
    fn stream(&self, request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        self.requests
            .lock()
            .expect("peticiones")
            .push(request.clone());
        if let Some(delay) = self.slow_first_call {
            thread::sleep(delay);
        }
        let script = self
            .scripts
            .lock()
            .expect("guiones")
            .pop_front()
            .ok_or_else(|| LlmError::other("guion agotado"))?;
        if let Some(error) = script.get("error").and_then(Value::as_str) {
            return Err(script_error(error));
        }
        let chunks = script
            .get("chunks")
            .or_else(|| script.get("stream"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let closed = Arc::new(Mutex::new(false));
        self.streams.lock().expect("flujos").push(closed.clone());
        Ok(Box::new(ScriptedStream {
            chunks: chunks.iter().map(chunk_of).collect(),
            boom: script.get("boom").and_then(Value::as_str).map(script_error),
            closed,
        }))
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

// ------------------------------------------------------------------- runtime

pub struct FakeRuntime {
    pub prompt: Mutex<String>,
    volatile: String,
    tools: Vec<Value>,
    results: Map<String, Value>,
    remaining: Mutex<Vec<i64>>,
    parts: bool,
    parts_fails: bool,
    begin_fails: bool,
    tools_for_fails: bool,
    registry_fails: bool,
    pending: Option<String>,
    pub executed: Mutex<Vec<Value>>,
    pub recorded: Mutex<Vec<String>>,
    pub turns: Mutex<Vec<String>>,
    pub refreshed: AtomicUsize,
    pub confirm: Mutex<Option<bool>>,
    pub cancelled: AtomicUsize,
}

impl FakeRuntime {
    fn new(spec: &Value) -> Self {
        let remaining = match field(spec, "remaining") {
            Value::Number(number) => vec![number.as_i64().unwrap_or(0)],
            Value::Array(values) => values.iter().map(|v| v.as_i64().unwrap_or(0)).collect(),
            _ => Vec::new(),
        };
        Self {
            prompt: Mutex::new(text_of(spec, "prompt")),
            volatile: text_of(spec, "volatile"),
            tools: field(spec, "tools").as_array().cloned().unwrap_or_default(),
            results: field(spec, "results")
                .as_object()
                .cloned()
                .unwrap_or_default(),
            remaining: Mutex::new(remaining),
            parts: field(spec, "parts").as_bool().unwrap_or(false),
            parts_fails: field(spec, "parts_fails").as_bool().unwrap_or(false),
            begin_fails: field(spec, "begin_fails").as_bool().unwrap_or(false),
            tools_for_fails: field(spec, "tools_for_fails").as_bool().unwrap_or(false),
            registry_fails: field(spec, "registry_fails").as_bool().unwrap_or(false),
            pending: field(spec, "pending").as_str().map(str::to_string),
            executed: Mutex::new(Vec::new()),
            recorded: Mutex::new(Vec::new()),
            turns: Mutex::new(Vec::new()),
            refreshed: AtomicUsize::new(0),
            confirm: Mutex::new(None),
            cancelled: AtomicUsize::new(0),
        }
    }

    pub fn with_default_prompt(spec: &Value) -> Self {
        let mut runtime = Self::new(spec);
        if runtime.prompt.lock().expect("prompt").is_empty() {
            *runtime.prompt.lock().expect("prompt") = "PROMPT-SISTEMA".to_string();
        }
        if runtime.tools.is_empty() && field(spec, "tools").is_null() {
            runtime.tools = vec![json!({
                "type": "function",
                "function": {"name": "get_datetime", "parameters": {"type": "object"}},
            })];
        }
        runtime
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
        self.prompt.lock().expect("prompt").clone()
    }

    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        if self.parts_fails {
            return Err(RuntimeFailure::new("RuntimeError", "sin partes"));
        }
        let prompt = self.system_prompt();
        if self.parts {
            return Ok((prompt, self.volatile.clone()));
        }
        Ok((prompt, String::new()))
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
        if self.registry_fails {
            return Err(RuntimeFailure::new("RuntimeError", "registro roto"));
        }
        Ok(BASE_TOOLS
            .iter()
            .map(|name| {
                json!({"type": "function",
                    "function": {"name": name, "parameters": {"type": "object"}}})
            })
            .collect())
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        let mut queue = self.remaining.lock().expect("presupuesto");
        if queue.is_empty() {
            return None;
        }
        if queue.len() > 1 {
            queue.remove(0).into()
        } else {
            queue[0].into()
        }
    }

    fn execute(&self, name: &str, raw_arguments: &str) -> Result<Value, RuntimeFailure> {
        self.executed
            .lock()
            .expect("ejecutadas")
            .push(json!([name, raw_arguments]));
        let result = self
            .results
            .get(name)
            .cloned()
            .unwrap_or(json!({"ok": true, "data": "resultado"}));
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            let (class, message) = error.split_once(": ").unwrap_or(("RuntimeError", error));
            return Err(RuntimeFailure::new(class, message));
        }
        Ok(result)
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

    fn refresh(&self) {
        self.refreshed.fetch_add(1, Ordering::SeqCst);
    }

    fn set_confirm_actions(&self, enabled: bool) {
        *self.confirm.lock().expect("confirmación") = Some(enabled);
    }

    fn pending_notify_address(&self) -> Option<String> {
        // `pending == "raise"` modela al runtime que falla; el Messenger lo traga.
        self.pending.clone().filter(|address| address != "raise")
    }

    fn cancel_confirmation(&self) {
        self.cancelled.fetch_add(1, Ordering::SeqCst);
    }
}

// -------------------------------------------------------------------- helpers

pub fn default_messenger() -> Messenger {
    messenger_with(
        Some(Arc::new(FakeRuntime::with_default_prompt(&json!({})))),
        None,
        &[],
    )
}

pub fn messenger_with(
    runtime: Option<Arc<FakeRuntime>>,
    client: Option<Arc<ScriptedClient>>,
    values: &[(&str, Value)],
) -> Messenger {
    let mut cfg = VoiceConfig::new();
    for (key, value) in values {
        cfg.set(key, value.clone());
    }
    let runtime = runtime.map(|runtime| runtime as Arc<dyn AgentRuntimeApi>);
    let messenger = Messenger::new(cfg, MessengerCallbacks::default(), runtime);
    if let Some(client) = client {
        messenger.set_client(client);
    }
    messenger
}
