//! Streaming SSE y traducción de eventos de la API Messages (F4-06).
//!
//! Port de `iter_sse` y `translate_events` de `anthropic_client.py`: convierte
//! los eventos de Messages en los mismos [`Chunk`] de Chat Completions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read};

use ez_core::config::int_value;
use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{Map, Value};

use super::{str_or_empty, STOP_REASONS};
use crate::errors::LlmError;
use crate::openai::{ChatStream, Chunk, ToolCallDelta};
use crate::sse::SseReader;

/// `_sse_event`: JSON de un bloque `data:`, solo si es un objeto.
fn parse_event(data: &str) -> Option<Value> {
    let text = py_strip(data);
    if text.is_empty() || text == "[DONE]" {
        return None;
    }
    match pyloads::loads(text) {
        Ok(Value::Object(map)) => Some(Value::Object(map)),
        _ => None,
    }
}

/// `_sse_events`: bloque multilínea o, si no es JSON, varias líneas sueltas.
fn parse_block(payload: &str) -> Vec<Value> {
    let joined = py_strip(payload);
    if joined.is_empty() {
        return Vec::new();
    }
    if let Some(event) = parse_event(joined) {
        return vec![event];
    }
    // DECISION: hay servidores que mandan varios eventos sin línea en blanco;
    // se intentan por separado antes de descartar el bloque entero.
    payload.split('\n').filter_map(parse_event).collect()
}

/// Eventos `data:` de un flujo SSE de Messages.
pub struct SseEvents<R: BufRead> {
    reader: Option<SseReader<R>>,
    pending: VecDeque<Value>,
    done: bool,
}

impl<R: BufRead> SseEvents<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: Some(SseReader::new(reader)),
            pending: VecDeque::new(),
            done: false,
        }
    }
}

impl<R: BufRead> Iterator for SseEvents<R> {
    type Item = Result<Value, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(Ok(event));
            }
            if self.done {
                return None;
            }
            let Some(reader) = self.reader.as_mut() else {
                self.done = true;
                return None;
            };
            match reader.next_payload() {
                None => {
                    self.done = true;
                    self.reader = None;
                    return None;
                }
                Some(Err(error)) => {
                    self.done = true;
                    self.reader = None;
                    return Some(Err(LlmError::other(error.to_string())));
                }
                Some(Ok(payload)) => self.pending.extend(parse_block(&payload)),
            }
        }
    }
}

/// Eventos `data:` de un flujo SSE, como `iter_sse` del Python.
pub fn iter_sse<R: BufRead>(reader: R) -> SseEvents<R> {
    SseEvents::new(reader)
}

enum Translated {
    Chunk(Chunk),
    Stop,
    Nothing,
}

#[derive(Default)]
struct TranslateState {
    tool_index: HashMap<i64, usize>,
    got_args: HashSet<i64>,
}

fn stop_reason(stop: &str) -> String {
    STOP_REASONS
        .iter()
        .find(|(name, _)| *name == stop)
        .map_or("stop", |(_, reason)| reason)
        .to_string()
}

fn translate_event(event: &Value, state: &mut TranslateState) -> Result<Translated, LlmError> {
    let Some(object) = event.as_object() else {
        return Ok(Translated::Nothing);
    };
    let kind = object.get("type").and_then(Value::as_str);
    if kind == Some("error") {
        return error_translation(object);
    }
    let block_index = object.get("index").map_or(0, |value| int_value(value, 0));
    match kind {
        Some("content_block_start") => Ok(block_start(object, block_index, state)),
        Some("content_block_delta") => Ok(block_delta(object, block_index, state)),
        Some("content_block_stop") => Ok(block_stop(block_index, state)),
        Some("message_delta") => Ok(message_delta(object)),
        Some("message_stop") => Ok(Translated::Stop),
        _ => Ok(Translated::Nothing),
    }
}

fn error_translation(object: &Map<String, Value>) -> Result<Translated, LlmError> {
    let error = object.get("error").and_then(Value::as_object);
    let error_type = error
        .and_then(|error| error.get("type"))
        .map(python_str)
        .unwrap_or_else(|| "error".to_string());
    let message = error
        .and_then(|error| error.get("message"))
        .map(python_str)
        .unwrap_or_default();
    Err(LlmError::other(format!(
        "Anthropic: {error_type}: {message}"
    )))
}

fn block_start(
    object: &Map<String, Value>,
    block_index: i64,
    state: &mut TranslateState,
) -> Translated {
    let block = object.get("content_block").and_then(Value::as_object);
    match block
        .and_then(|block| block.get("type"))
        .and_then(Value::as_str)
    {
        Some("tool_use") => {
            let position = state.tool_index.len();
            state.tool_index.insert(block_index, position);
            Translated::Chunk(tool_chunk(
                position,
                block
                    .and_then(|block| block.get("id"))
                    .and_then(Value::as_str),
                block
                    .and_then(|block| block.get("name"))
                    .and_then(Value::as_str),
                "",
            ))
        }
        Some("text") => {
            let text = block
                .and_then(|block| block.get("text"))
                .filter(|value| truthy(value));
            text.map_or(Translated::Nothing, |text| {
                Translated::Chunk(Chunk {
                    content: Some(python_str(text)),
                    ..Chunk::default()
                })
            })
        }
        _ => Translated::Nothing,
    }
}

fn block_delta(
    object: &Map<String, Value>,
    block_index: i64,
    state: &mut TranslateState,
) -> Translated {
    let delta = object.get("delta").and_then(Value::as_object);
    let delta_type = delta
        .and_then(|delta| delta.get("type"))
        .and_then(Value::as_str);
    if delta_type == Some("text_delta") {
        let text = delta
            .and_then(|delta| delta.get("text"))
            .filter(|value| truthy(value));
        return text.map_or(Translated::Nothing, |text| {
            Translated::Chunk(Chunk {
                content: Some(python_str(text)),
                ..Chunk::default()
            })
        });
    }
    if delta_type == Some("input_json_delta") {
        if let Some(position) = state.tool_index.get(&block_index).copied() {
            let partial = str_or_empty(delta.and_then(|delta| delta.get("partial_json")));
            if !partial.is_empty() {
                state.got_args.insert(block_index);
                return Translated::Chunk(tool_chunk(position, None, None, &partial));
            }
        }
    }
    Translated::Nothing
}

fn block_stop(block_index: i64, state: &TranslateState) -> Translated {
    match state.tool_index.get(&block_index).copied() {
        Some(position) if !state.got_args.contains(&block_index) => {
            // Herramienta sin argumentos: el bucle espera un JSON válido.
            Translated::Chunk(tool_chunk(position, None, None, "{}"))
        }
        _ => Translated::Nothing,
    }
}

fn message_delta(object: &Map<String, Value>) -> Translated {
    let stop = object
        .get("delta")
        .and_then(|delta| delta.get("stop_reason"))
        .filter(|value| truthy(value));
    let usage = object.get("usage");
    if stop.is_none() && !usage.is_some_and(truthy) {
        return Translated::Nothing;
    }
    let reason = stop.map(|stop| stop_reason(&python_str(stop)));
    Translated::Chunk(Chunk {
        finish_reason: reason,
        usage: usage.filter(|value| !value.is_null()).cloned(),
        ..Chunk::default()
    })
}

fn tool_chunk(index: usize, call_id: Option<&str>, name: Option<&str>, arguments: &str) -> Chunk {
    Chunk {
        tool_calls: vec![ToolCallDelta {
            index,
            id: call_id.filter(|text| !text.is_empty()).map(str::to_string),
            name: name.filter(|text| !text.is_empty()).map(str::to_string),
            arguments: arguments.to_string(),
        }],
        ..Chunk::default()
    }
}

/// Eventos de Messages -> trozos con forma de Chat Completions.
pub struct TranslatedEvents<I> {
    events: I,
    state: TranslateState,
    finished: bool,
}

impl<I: Iterator<Item = Result<Value, LlmError>>> Iterator for TranslatedEvents<I> {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        for event in self.events.by_ref() {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            };
            match translate_event(&event, &mut self.state) {
                Ok(Translated::Chunk(chunk)) => return Some(Ok(chunk)),
                Ok(Translated::Nothing) => continue,
                Ok(Translated::Stop) => {
                    self.finished = true;
                    return None;
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            }
        }
        self.finished = true;
        None
    }
}

/// Traduce los eventos de un flujo de Messages, como `translate_events`.
pub fn translate_events<I: Iterator<Item = Result<Value, LlmError>>>(
    events: I,
) -> TranslatedEvents<I> {
    TranslatedEvents {
        events,
        state: TranslateState::default(),
        finished: false,
    }
}

/// Flujo de trozos de la API Messages.
pub struct AnthropicStream<R: BufRead> {
    events: Option<TranslatedEvents<SseEvents<R>>>,
}

impl<R: BufRead> AnthropicStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            events: Some(translate_events(iter_sse(reader))),
        }
    }
}

impl<R: Read> AnthropicStream<BufReader<R>> {
    /// Envuelve la respuesta HTTP en un flujo de trozos.
    pub fn over(reader: R) -> Self {
        Self::new(BufReader::new(reader))
    }
}

impl<R: BufRead> Iterator for AnthropicStream<R> {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.events.as_mut()?.next()
    }
}

impl<R: BufRead + Send> ChatStream for AnthropicStream<R> {
    fn close(&mut self) {
        self.events = None;
    }
}
