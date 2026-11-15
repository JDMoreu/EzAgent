//! Ejecución de un turno guionizado para la paridad de `llm_tools`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ez_llm::messenger::{AskMode, Messenger, TurnError};
use ez_parity::fixtures::FixtureCase;
use regex::Regex;
use serde_json::{json, Value};

use super::client::{field, messenger_with, text_of, FakeRuntime, ScriptedClient};

pub fn request_summary(request: &Value) -> Value {
    let messages = field(request, "messages")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let tools: Vec<String> = field(request, "tools")
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    let function = if tool.get("type").and_then(Value::as_str) == Some("function") {
                        tool.get("function").cloned().unwrap_or(Value::Null)
                    } else {
                        tool.clone()
                    };
                    function
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();
    let tool_call_ids: Vec<Value> = messages
        .iter()
        .flat_map(|message| {
            field(message, "tool_calls")
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .map(|call| call.get("id").cloned().unwrap_or(Value::Null))
        .collect();
    let mut summary = json!({
        "max_tokens": request.get("max_tokens").cloned().unwrap_or(Value::Null),
        "tools": tools,
        "tool_choice": request.get("tool_choice").cloned().unwrap_or(Value::Null),
        "roles": messages.iter().map(|m| m.get("role").cloned().unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "tool_call_ids": tool_call_ids,
        "last_content": messages.last().and_then(|m| m.get("content")).cloned().unwrap_or(Value::Null),
        "system": messages.first().and_then(|m| m.get("content")).cloned().unwrap_or(Value::Null),
    });
    for key in ["temperature", "extra_body", "max_completion_tokens"] {
        if let Some(value) = request.get(key) {
            summary[key] = value.clone();
        }
    }
    summary
}

pub fn normalize_rounds(rounds: &[String]) -> Vec<String> {
    let pattern = Regex::new(r"\d+ ms").expect("patrón de milisegundos");
    rounds
        .iter()
        .map(|round| pattern.replace_all(round, "0 ms").to_string())
        .collect()
}

struct TurnSetup {
    messenger: Messenger,
    client: Arc<ScriptedClient>,
    runtime: Option<Arc<FakeRuntime>>,
    events: Arc<Mutex<Vec<String>>>,
}

fn turn_setup(case: &FixtureCase) -> TurnSetup {
    let input = &case.input;
    let clock = input.get("clock").and_then(Value::as_str);
    let client = Arc::new(ScriptedClient::new(
        field(input, "scripts")
            .as_array()
            .cloned()
            .unwrap_or_default(),
        (clock == Some("jump")).then(|| Duration::from_millis(1100)),
    ));
    let runtime = match input.get("runtime") {
        Some(Value::Null) => None,
        Some(spec) if spec.as_object().is_some_and(|map| !map.is_empty()) => {
            Some(Arc::new(FakeRuntime::with_default_prompt(spec)))
        }
        _ => Some(Arc::new(FakeRuntime::with_default_prompt(&json!({})))),
    };
    let settings = field(input, "settings")
        .as_object()
        .cloned()
        .unwrap_or_default();
    let values: Vec<(&str, Value)> = settings
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    let messenger = messenger_with(runtime.clone(), Some(client.clone()), &values);
    if let Some(history) = input.get("history").and_then(Value::as_array) {
        messenger.set_messages(history.clone());
    }
    let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collected = events.clone();
    messenger.set_on_context_event(Arc::new(move |text: &str| {
        collected.lock().expect("eventos").push(text.to_string());
    }));
    TurnSetup {
        messenger,
        client,
        runtime,
        events,
    }
}

fn runtime_list(runtime: &Option<Arc<FakeRuntime>>, get: &dyn Fn(&FakeRuntime) -> Value) -> Value {
    runtime
        .as_ref()
        .map(|runtime| get(runtime))
        .unwrap_or(json!([]))
}

pub fn turn_output(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let setup = turn_setup(case);
    let mode = if input.get("mode").and_then(Value::as_str) == Some("chat") {
        AskMode::Chat
    } else {
        AskMode::Voice
    };
    let user = text_of(input, "user");
    let user = if user.is_empty() {
        "hola".to_string()
    } else {
        user
    };
    let pieces: Vec<String> = setup
        .messenger
        .ask_stream(&user, mode)
        .collect::<Result<_, _>>()
        .map_err(turn_error_text)?;
    let (streams_closed, streams) = setup.client.streams_summary();
    Ok(json!({
        "pieces": pieces,
        "answer": pieces.concat(),
        "history": setup.messenger.messages(),
        "requests": setup.client.requests().iter().map(request_summary).collect::<Vec<_>>(),
        "executed": runtime_list(&setup.runtime, &|runtime| json!(runtime.executed.lock().expect("ejecutadas").clone())),
        "recorded": runtime_list(&setup.runtime, &|runtime| json!(runtime.recorded.lock().expect("respuestas").clone())),
        "turns": runtime_list(&setup.runtime, &|runtime| json!(runtime.turns.lock().expect("turnos").clone())),
        "last_rounds": normalize_rounds(&setup.messenger.last_rounds()),
        "context_events": setup.events.lock().expect("eventos").clone(),
        "summary": setup.messenger.summary(),
        "streams_closed": streams_closed,
        "streams": streams,
    }))
}

pub fn turn_error_text(error: TurnError) -> String {
    match error {
        TurnError::Timeout(message) => format!("LLMTurnTimeout: {message}"),
        TurnError::Cancelled(message) => format!("TurnCancelled: {message}"),
        TurnError::Runtime(failure) => format!("{}: {}", failure.kind, failure.message),
        TurnError::Llm(error) => {
            let class = match error.kind {
                ez_llm::errors::ErrorKind::Connection => "ConnectionError",
                ez_llm::errors::ErrorKind::Timeout => "APITimeoutError",
                _ => "RuntimeError",
            };
            format!("{class}: {}", error.message)
        }
    }
}
