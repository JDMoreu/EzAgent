//! Dobles de prueba del `Messenger` (F4-07/F4-08): cliente guionizado, streams
//! controlables y runtime falso.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use ez_core::config::VoiceConfig;
use ez_core::pyjson::{dumps_with, DumpOptions};
use serde_json::{json, Value};

use crate::errors::LlmError;
use crate::messenger::{AgentRuntimeApi, AskMode, Messenger, MessengerCallbacks, RuntimeFailure};
use crate::openai::{ChatStream, Chunk, ToolCallDelta};

/// Guion de una llamada al proveedor.
pub(crate) enum Script {
    Chunks(Vec<Chunk>),
    Fail(LlmError),
    Blocked {
        first: Chunk,
        rest: Vec<Chunk>,
        gate: Arc<Gate>,
    },
}

/// Puerta manual para dejar un stream bloqueado a mitad.
pub(crate) struct Gate {
    released: Mutex<bool>,
    condvar: Condvar,
}

impl Gate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            released: Mutex::new(false),
            condvar: Condvar::new(),
        })
    }

    pub(crate) fn release(&self) {
        let mut released = self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *released = true;
        self.condvar.notify_all();
    }

    fn wait(&self) {
        let mut released = self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*released {
            released = self
                .condvar
                .wait(released)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

pub(crate) struct FakeClient {
    scripts: Mutex<VecDeque<Script>>,
    pub requests: Mutex<Vec<Value>>,
    pub closed: Arc<AtomicUsize>,
}

impl FakeClient {
    pub(crate) fn new(scripts: Vec<Script>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into()),
            requests: Mutex::new(Vec::new()),
            closed: Arc::new(AtomicUsize::new(0)),
        })
    }
}

impl crate::messenger::LlmClient for FakeClient {
    fn stream(&self, request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.clone());
        let script = self
            .scripts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front();
        match script {
            Some(Script::Chunks(chunks)) => Ok(Box::new(VecStream {
                chunks: chunks.into(),
                closed: self.closed.clone(),
            })),
            Some(Script::Fail(error)) => Err(error),
            Some(Script::Blocked { first, rest, gate }) => Ok(Box::new(BlockingStream {
                first: Some(first),
                rest: rest.into(),
                gate,
                closed: self.closed.clone(),
            })),
            None => Ok(Box::new(VecStream {
                chunks: Vec::new().into(),
                closed: self.closed.clone(),
            })),
        }
    }

    fn close(&self) {}
}

struct VecStream {
    chunks: VecDeque<Chunk>,
    closed: Arc<AtomicUsize>,
}

impl Iterator for VecStream {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.pop_front().map(Ok)
    }
}

impl ChatStream for VecStream {
    fn close(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
        self.chunks.clear();
    }
}

impl Drop for VecStream {
    fn drop(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}

struct BlockingStream {
    first: Option<Chunk>,
    rest: VecDeque<Chunk>,
    gate: Arc<Gate>,
    closed: Arc<AtomicUsize>,
}

impl Iterator for BlockingStream {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            return Some(Ok(first));
        }
        let chunk = self.rest.pop_front()?;
        self.gate.wait();
        Some(Ok(chunk))
    }
}

impl ChatStream for BlockingStream {
    fn close(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
        self.first = None;
        self.rest.clear();
        self.gate.release();
    }
}

impl Drop for BlockingStream {
    fn drop(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}

/// Estado del runtime doble.
pub(crate) struct RuntimeState {
    pub prompt: String,
    pub parts: Option<(String, String)>,
    pub tools: Vec<Value>,
    pub base_tools: Vec<Value>,
    pub begin_error: Option<RuntimeFailure>,
    pub tools_error: Option<RuntimeFailure>,
    pub executed: Vec<(String, String)>,
    pub results: Vec<(String, Result<Value, RuntimeFailure>)>,
    pub recorded: Vec<String>,
    pub turns: Vec<String>,
    pub remaining: Option<i64>,
    pub notify: Option<String>,
    pub confirm: Option<bool>,
    pub refreshed: usize,
}

pub(crate) struct FakeRuntime {
    state: Mutex<RuntimeState>,
}

impl FakeRuntime {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RuntimeState {
                prompt: "PROMPT-SISTEMA".into(),
                parts: None,
                tools: vec![json!({"type": "function", "function": {"name": "get_datetime"}})],
                base_tools: Vec::new(),
                begin_error: None,
                tools_error: None,
                executed: Vec::new(),
                results: Vec::new(),
                recorded: Vec::new(),
                turns: Vec::new(),
                remaining: None,
                notify: None,
                confirm: None,
                refreshed: 0,
            }),
        })
    }

    pub(crate) fn state(&self) -> std::sync::MutexGuard<'_, RuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn result(self: &Arc<Self>, name: &str, value: Result<Value, RuntimeFailure>) {
        self.state().results.push((name.to_string(), value));
    }
}

impl AgentRuntimeApi for FakeRuntime {
    fn begin_turn(&self, text: &str) -> Result<(), RuntimeFailure> {
        let mut state = self.state();
        state.turns.push(text.to_string());
        match state.begin_error.clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn system_prompt(&self) -> String {
        self.state().prompt.clone()
    }

    fn prompt_parts(&self) -> Result<(String, String), RuntimeFailure> {
        let state = self.state();
        Ok(state
            .parts
            .clone()
            .unwrap_or_else(|| (state.prompt.clone(), String::new())))
    }

    fn tools_for(&self, _text: &str) -> Result<Vec<Value>, RuntimeFailure> {
        let state = self.state();
        match &state.tools_error {
            Some(error) => Err(error.clone()),
            None => Ok(state.tools.clone()),
        }
    }

    fn base_tool_schemas(&self) -> Result<Vec<Value>, RuntimeFailure> {
        Ok(self.state().base_tools.clone())
    }

    fn remaining_tool_calls(&self) -> Option<i64> {
        self.state().remaining
    }

    fn execute(&self, name: &str, raw_arguments: &str) -> Result<Value, RuntimeFailure> {
        let mut state = self.state();
        state
            .executed
            .push((name.to_string(), raw_arguments.to_string()));
        match state.results.iter().find(|(key, _)| key == name) {
            Some((_, result)) => result.clone(),
            None => Ok(json!({"ok": true, "data": "resultado", "error": null})),
        }
    }

    fn render_result(&self, result: Value) -> String {
        dumps_with(&result, &DumpOptions::utf8())
    }

    fn record_assistant(&self, text: &str) {
        self.state().recorded.push(text.to_string());
    }

    fn refresh(&self) {
        self.state().refreshed += 1;
    }

    fn set_confirm_actions(&self, enabled: bool) {
        self.state().confirm = Some(enabled);
    }

    fn pending_notify_address(&self) -> Option<String> {
        self.state().notify.clone()
    }
}

pub(crate) fn test_config(overrides: &[(&str, Value)]) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    let defaults = [
        ("llm_provider", json!("openai")),
        ("llm_model", json!("gpt-x")),
        ("llm_temperature", json!(0.3)),
        ("llm_max_tokens", json!(150)),
        ("llm_base_url", json!("https://api.openai.com/v1")),
        ("api_key", json!("k")),
        ("llm_api_style", json!("openai")),
        ("llm_reasoning_effort", json!("none")),
        ("confirm_actions", json!(false)),
    ];
    for (key, value) in defaults {
        if !overrides.iter().any(|(given, _)| *given == key) {
            cfg.set(key, value);
        }
    }
    for (key, value) in overrides {
        cfg.set(key, value.clone());
    }
    cfg
}

pub(crate) fn messenger_with(
    scripts: Vec<Script>,
    runtime: Option<Arc<dyn AgentRuntimeApi>>,
    overrides: &[(&str, Value)],
) -> (Messenger, Arc<FakeClient>) {
    let client = FakeClient::new(scripts);
    let messenger = Messenger::new(
        test_config(overrides),
        MessengerCallbacks::default(),
        runtime,
    );
    messenger.set_client(client.clone());
    (messenger, client)
}

pub(crate) fn text(content: &str) -> Chunk {
    Chunk {
        content: Some(content.to_string()),
        ..Chunk::default()
    }
}

pub(crate) fn empty() -> Chunk {
    Chunk::default()
}

pub(crate) fn call(name: &str, arguments: &str, index: usize, id: Option<&str>) -> Chunk {
    Chunk {
        tool_calls: vec![ToolCallDelta {
            index,
            id: id.map(str::to_string),
            name: Some(name.to_string()),
            arguments: arguments.to_string(),
        }],
        ..Chunk::default()
    }
}

pub(crate) fn done(reason: &str) -> Chunk {
    Chunk {
        finish_reason: Some(reason.to_string()),
        ..Chunk::default()
    }
}

pub(crate) fn say(messenger: &Messenger, text: &str) -> String {
    messenger
        .ask_stream(text, AskMode::Voice)
        .collect::<Result<String, _>>()
        .expect("turno completo")
}

pub(crate) fn requests(client: &FakeClient) -> Vec<Value> {
    client
        .requests
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub(crate) fn tool_round(name: &str, arguments: &str, id: &str) -> Vec<Chunk> {
    vec![
        call(name, "", 0, Some(id)),
        call("", arguments, 0, None),
        done("tool_calls"),
    ]
}
