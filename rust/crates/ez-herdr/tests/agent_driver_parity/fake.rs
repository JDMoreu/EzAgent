//! Doble de `AgentSession` (pantallas, agentes, envíos y teclas) y reloj que
//! avanza al dormir, igual que el arnés del generador.

use std::sync::{Arc, Mutex};

use ez_core::pyvalue::truthy;
use ez_herdr::herdr::HerdrError;
use ez_herdr::session::{SessionClock, SessionHost};
use serde_json::{json, Map, Value};

pub(super) fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn number_or_zero(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

pub(super) fn argv(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn list_or(value: Option<&Value>, default: Value) -> Vec<Value> {
    match value.and_then(Value::as_array) {
        Some(items) if !items.is_empty() => items.clone(),
        _ => vec![default],
    }
}

fn next(values: &[Value], index: usize) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    values[index.min(values.len() - 1)].clone()
}

struct HostState {
    screens: Vec<Value>,
    agents: Vec<Value>,
    sends: Vec<Value>,
    keys: Vec<Value>,
    screen_index: usize,
    agent_index: usize,
    send_index: usize,
    key_index: usize,
    calls: Vec<Value>,
}

#[derive(Clone)]
pub(super) struct FakeHost {
    state: Arc<Mutex<HostState>>,
}

impl FakeHost {
    pub(super) fn new(input: &Value) -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState {
                screens: list_or(input.get("screens"), json!("")),
                agents: list_or(input.get("agents"), json!({})),
                sends: list_or(input.get("sends"), json!({"ok": true})),
                keys: list_or(input.get("keys"), json!({"ok": true})),
                screen_index: 0,
                agent_index: 0,
                send_index: 0,
                key_index: 0,
                calls: Vec::new(),
            })),
        }
    }

    pub(super) fn calls(&self) -> Vec<Value> {
        self.lock().calls.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HostState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take(&self, which: &str) -> (Value, usize) {
        let mut state = self.lock();
        let (value, index) = match which {
            "screen" => (next(&state.screens, state.screen_index), state.screen_index),
            "agent" => (next(&state.agents, state.agent_index), state.agent_index),
            "send" => (next(&state.sends, state.send_index), state.send_index),
            _ => (next(&state.keys, state.key_index), state.key_index),
        };
        match which {
            "screen" => state.screen_index = index + 1,
            "agent" => state.agent_index = index + 1,
            "send" => state.send_index = index + 1,
            _ => state.key_index = index + 1,
        }
        (value, index)
    }

    fn raise(value: &Value) -> Option<HerdrError> {
        let message = value
            .as_object()
            .filter(|map| map.contains_key("raise"))
            .map(|map| text(map.get("message")));
        message.map(HerdrError::failed)
    }

    fn record(&self, call: Value) {
        self.lock().calls.push(call);
    }
}

impl SessionHost for FakeHost {
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, HerdrError> {
        let (value, _) = self.take("agent");
        self.record(json!({"kind": "get", "address": address}));
        if let Some(error) = Self::raise(&value) {
            return Err(error);
        }
        Ok(match value {
            Value::Object(map) => map,
            _ => Map::new(),
        })
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        let (value, _) = self.take("send");
        self.record(json!({
            "kind": "send",
            "address": address,
            "text": text,
            "confirmed": confirmed,
        }));
        if let Some(error) = Self::raise(&value) {
            return Err(error);
        }
        Ok(match value {
            Value::Object(map) => map,
            other => boolean_row(&other),
        })
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, HerdrError> {
        let (value, _) = self.take("key");
        self.record(json!({"kind": "keys", "address": address, "keys": keys}));
        if let Some(error) = Self::raise(&value) {
            return Err(error);
        }
        Ok(match value {
            Value::Object(map) => map,
            other => boolean_row(&other),
        })
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, HerdrError> {
        let (value, _) = self.take("screen");
        self.record(json!({"kind": "read", "address": address, "lines": lines}));
        if let Some(error) = Self::raise(&value) {
            return Err(error);
        }
        Ok(match &value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
    }
}

fn boolean_row(value: &Value) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("ok".to_string(), Value::Bool(truthy(value)));
    row
}

#[derive(Clone)]
pub(super) struct TestClock {
    state: Arc<Mutex<(f64, Vec<f64>)>>,
}

impl TestClock {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new((0.0, Vec::new()))),
        }
    }

    pub(super) fn delays(&self) -> Vec<f64> {
        self.lock().1.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (f64, Vec<f64>)> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl SessionClock for TestClock {
    fn now(&self) -> f64 {
        self.lock().0
    }

    fn sleep(&self, seconds: f64) {
        let mut state = self.lock();
        state.1.push(seconds);
        state.0 += seconds;
    }
}

/// `{"value": …, "calls": …, "delays": …}` de `_session_capture`.
pub(super) fn session_output(value: Value, host: &FakeHost, clock: &TestClock) -> Value {
    json!({"value": value, "calls": host.calls(), "delays": clock.delays()})
}
