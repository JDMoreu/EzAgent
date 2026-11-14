//! Port of `src/ezagent/voice/anthropic_client.py` (F4-06).
//!
//! Cliente mínimo de la API Messages (Anthropic) con forma de cliente OpenAI:
//! traduce la petición al formato Messages, lee el streaming SSE y entrega los
//! mismos [`Chunk`] que el cliente de Chat Completions. La clave va en
//! `x-api-key` y nunca se registra.

use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;

use ez_core::config::int_value;
use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use reqwest::blocking::{Client, Response};
use serde_json::{json, Map, Value};

use crate::condenser::TextCompleter;
use crate::errors::LlmError;
use crate::openai::{
    condenser_request, map_transport_error, request_timeout, retry_after, seconds_or_default,
    ChatStream,
};

mod stream;
#[cfg(test)]
mod tests;

pub use stream::{iter_sse, translate_events, AnthropicStream, SseEvents, TranslatedEvents};

pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// `STOP_REASONS` del Python: motivo de Messages -> `finish_reason` de OpenAI.
pub const STOP_REASONS: [(&str, &str); 3] = [
    ("end_turn", "stop"),
    ("max_tokens", "length"),
    ("tool_use", "tool_calls"),
];

/// `…/v1` -> `…/v1/messages`; una raíz sin versión -> `…/v1/messages`.
pub fn messages_url(base_url: &str) -> String {
    let base = py_strip(base_url).trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

/// `str(value or "")`.
fn str_or_empty(value: Option<&Value>) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => String::new(),
    }
}

/// `value or ""` conservando el tipo del valor (para ids y nombres).
fn or_empty(value: Option<&Value>) -> Value {
    match value {
        Some(value) if truthy(value) => value.clone(),
        _ => Value::String(String::new()),
    }
}

/// `_text_block`: bloque de texto, o nada si el valor está en blanco.
fn text_block(text: &Value) -> Vec<Value> {
    let value = str_or_empty(Some(text));
    if py_strip(&value).is_empty() {
        Vec::new()
    } else {
        vec![json!({"type": "text", "text": value})]
    }
}

/// `_tool_input`: argumentos JSON de una llamada, solo si son un objeto.
fn tool_input(arguments: Option<&Value>) -> Value {
    match arguments {
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(Value::String(text)) if !text.is_empty() => match pyloads::loads(text) {
            Ok(Value::Object(map)) => Value::Object(map),
            _ => Value::Object(Map::new()),
        },
        _ => Value::Object(Map::new()),
    }
}

/// `_append`: une turnos seguidos del mismo rol.
fn append(out: &mut Vec<Value>, role: &str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.get("role").and_then(Value::as_str) == Some(role) {
            if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) {
                content.extend(blocks);
                return;
            }
        }
    }
    out.push(json!({"role": role, "content": blocks}));
}

fn content_or_null(message: &Value) -> &Value {
    message.get("content").unwrap_or(&Value::Null)
}

fn assistant_blocks(message: &Value) -> Vec<Value> {
    let mut blocks = text_block(content_or_null(message));
    let calls = message.get("tool_calls").and_then(Value::as_array);
    for call in calls.into_iter().flatten() {
        let function = call.get("function");
        blocks.push(json!({
            "type": "tool_use",
            "id": or_empty(call.get("id")),
            "name": or_empty(function.and_then(|function| function.get("name"))),
            "input": tool_input(function.and_then(|function| function.get("arguments"))),
        }));
    }
    blocks
}

fn tool_result_block(message: &Value) -> Value {
    json!({
        "type": "tool_result",
        "tool_use_id": or_empty(message.get("tool_call_id")),
        "content": str_or_empty(message.get("content")),
    })
}

/// Mensajes OpenAI -> `(system, mensajes Anthropic)`.
pub fn convert_messages(messages: &[Value]) -> (String, Vec<Value>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut out: Vec<Value> = Vec::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("system") => {
                let content = message.get("content");
                if out.is_empty() {
                    system_parts.push(str_or_empty(content));
                } else {
                    // DECISION: Messages solo admite un system al principio; los
                    // avisos a mitad de conversación van como texto del usuario.
                    let notice = format!(
                        "[Aviso del sistema]\n{}",
                        python_str(content.unwrap_or(&Value::Null))
                    );
                    append(&mut out, "user", text_block(&Value::String(notice)));
                }
            }
            Some("user") => append(&mut out, "user", text_block(content_or_null(message))),
            Some("assistant") => append(&mut out, "assistant", assistant_blocks(message)),
            Some("tool") => append(&mut out, "user", vec![tool_result_block(message)]),
            _ => {}
        }
    }
    let system = system_parts
        .iter()
        .filter(|part| !py_strip(part).is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");
    (system, out)
}

/// Herramientas OpenAI -> esquemas de Messages; las inválidas se descartan.
pub fn convert_tools(tools: Option<&Value>) -> Vec<Value> {
    let empty = Vec::new();
    let items = tools.and_then(Value::as_array).unwrap_or(&empty);
    let mut result = Vec::new();
    for tool in items {
        let function = if tool.get("type").and_then(Value::as_str) == Some("function") {
            tool.get("function")
        } else {
            Some(tool)
        };
        let Some(function) = function.and_then(Value::as_object) else {
            // DECISION: el Python lanzaría si una herramienta no fuera un dict.
            continue;
        };
        let Some(name) = function.get("name").filter(|value| truthy(value)) else {
            continue;
        };
        let parameters = function
            .get("parameters")
            .filter(|value| truthy(value))
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
        result.push(json!({
            "name": name,
            "description": or_empty(function.get("description")),
            "input_schema": parameters,
        }));
    }
    result
}

/// Cliente que imita `OpenAI().chat.completions.create(stream=True, ...)`.
pub struct AnthropicChatClient {
    url: String,
    headers: Vec<(String, String)>,
    prompt_cache: bool,
    timeout: Duration,
    client: Mutex<Option<Client>>,
}

impl AnthropicChatClient {
    pub fn new(
        api_key: &str,
        base_url: &str,
        default_headers: &[(String, String)],
        timeout: f64,
        prompt_cache: bool,
    ) -> Self {
        let mut headers = default_headers.to_vec();
        headers.push(("x-api-key".to_string(), api_key.to_string()));
        headers.push((
            "anthropic-version".to_string(),
            ANTHROPIC_VERSION.to_string(),
        ));
        headers.push(("content-type".to_string(), "application/json".to_string()));
        Self {
            url: messages_url(base_url),
            headers,
            prompt_cache,
            timeout: seconds_or_default(timeout, 30.0),
            client: Mutex::new(None),
        }
    }

    /// Cierra el cliente HTTP compartido; la siguiente petición lo recrea.
    pub fn close(&self) {
        let mut guard = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    /// Cuerpo de la petición Messages, con `stream` siempre activo.
    pub fn build_body(&self, request: &Value) -> Value {
        let messages = request
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (system, converted) = convert_messages(&messages);
        let max_tokens = request
            .get("max_tokens")
            .filter(|value| truthy(value))
            .map_or(1024, |value| int_value(value, 1024));
        let mut body = Map::new();
        body.insert(
            "model".to_string(),
            request.get("model").cloned().unwrap_or(Value::Null),
        );
        body.insert("max_tokens".to_string(), json!(max_tokens));
        body.insert("messages".to_string(), Value::Array(converted));
        body.insert("stream".to_string(), Value::Bool(true));
        if !system.is_empty() {
            body.insert("system".to_string(), self.system_body(system));
        }
        if let Some(temperature) = request
            .get("temperature")
            .filter(|value| !value.is_null())
            .and_then(float_value)
        {
            body.insert("temperature".to_string(), json!(temperature));
        }
        let mut tools = convert_tools(request.get("tools"));
        if !tools.is_empty() {
            if self.prompt_cache {
                if let Some(last) = tools.last_mut() {
                    last["cache_control"] = json!({"type": "ephemeral"});
                }
            }
            body.insert("tools".to_string(), Value::Array(tools));
            body.insert("tool_choice".to_string(), json!({"type": "auto"}));
        }
        Value::Object(body)
    }

    fn system_body(&self, system: String) -> Value {
        if self.prompt_cache {
            json!([{
                "type": "text",
                "text": system,
                "cache_control": {"type": "ephemeral"},
            }])
        } else {
            Value::String(system)
        }
    }

    /// Abre la petición Messages y devuelve su flujo de trozos.
    pub fn stream(&self, request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        let body = self.build_body(request);
        let client = self.http()?;
        let response = client
            .post(&self.url)
            .headers(self.header_map())
            .timeout(request_timeout(self.timeout, request))
            .body(body.to_string())
            .send()
            .map_err(map_transport_error)?;
        let status = response.status().as_u16();
        if status >= 400 {
            let retry_after = retry_after(&response);
            let text = read_limited(response)?;
            let message = format!("La API compatible con Anthropic respondió {status}: {text}");
            return Err(LlmError::http(status, message).with_retry_after(retry_after));
        }
        Ok(Box::new(AnthropicStream::over(response)))
    }

    fn http(&self) -> Result<Client, LlmError> {
        let mut guard = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| LlmError::connection(error.to_string()))?;
        *guard = Some(client.clone());
        Ok(client)
    }

    fn header_map(&self) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (name, value) in &self.headers {
            let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) else {
                continue;
            };
            let Ok(value) = reqwest::header::HeaderValue::from_str(value) else {
                continue;
            };
            map.insert(name, value);
        }
        map
    }
}

impl TextCompleter for AnthropicChatClient {
    fn complete_text(
        &self,
        provider: &str,
        model: &str,
        messages: &[Value],
        max_tokens: u32,
    ) -> Result<String, String> {
        let request = condenser_request(provider, model, messages, max_tokens);
        let mut stream = self.stream(&request).map_err(|error| error.to_string())?;
        let mut text = String::new();
        let result = stream.by_ref().try_for_each(|chunk| {
            let chunk = chunk?;
            if let Some(content) = chunk.content {
                text.push_str(&content);
            }
            Ok(())
        });
        stream.close();
        result.map_err(|error: LlmError| error.to_string())?;
        Ok(text)
    }
}

fn float_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        // DECISION: una temperatura no numérica se omite; el Python lanzaría.
        Value::String(text) => py_strip(text).parse().ok(),
        _ => None,
    }
}

fn read_limited(response: Response) -> Result<String, LlmError> {
    let mut raw = Vec::new();
    let mut response = response;
    response
        .read_to_end(&mut raw)
        .map_err(|error| LlmError::other(error.to_string()))?;
    let text = String::from_utf8_lossy(&raw);
    Ok(ez_core::pyvalue::head_chars(&text, 300).to_string())
}
