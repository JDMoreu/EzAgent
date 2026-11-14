//! Port of `openai SDK usage in llm.py` (F4-06).
//!
//! Cliente mínimo de Chat Completions en streaming con `reqwest::blocking`:
//! mismo cuerpo de petición que arma el `Messenger`, mismos trozos que entrega
//! el SDK de OpenAI y los mismos errores HTTP que `friendly_llm_error` clasifica.

use std::io::{BufRead, BufReader, Read};
use std::sync::Mutex;
use std::time::Duration;

use ez_core::config::int_value;
use ez_core::pyloads;
use ez_core::pyvalue::{py_strip, python_str};
use reqwest::blocking::{Client, Response};
use serde_json::{json, Map, Value};

use crate::condenser::TextCompleter;
use crate::errors::LlmError;
use crate::sse::SseReader;

/// `base_url` que usa el SDK de OpenAI cuando no se configura ninguna.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Llamada a herramienta parcial, tal como llega en `delta.tool_calls`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCallDelta {
    /// `index` del SDK (posición de la llamada dentro del mensaje).
    pub index: usize,
    /// `id` de la llamada, solo en el primer fragmento.
    pub id: Option<String>,
    /// Nombre de la función, repartido en fragmentos.
    pub name: Option<String>,
    /// Trozo de argumentos JSON que hay que concatenar.
    pub arguments: String,
}

/// Un trozo de respuesta con la forma de `chunk.choices[0]` del SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    /// Texto nuevo del asistente (`delta.content`), si lo hay.
    pub content: Option<String>,
    /// Llamadas a herramienta parciales de este fragmento.
    pub tool_calls: Vec<ToolCallDelta>,
    /// `finish_reason` acumulado por el `Messenger` (`stop`, `length`, `tool_calls`).
    pub finish_reason: Option<String>,
    /// `usage` del evento, cuando el proveedor lo manda.
    pub usage: Option<Value>,
}

/// Flujo de trozos que consume el `Messenger`, con cierre explícito como en el SDK.
///
/// Es `Send` para que el `Messenger` pueda compartirlo con el hilo que cancela
/// el turno (F4-07).
pub trait ChatStream: Iterator<Item = Result<Chunk, LlmError>> + Send {
    /// Cierra la conexión (best-effort); las lecturas posteriores devuelven `None`.
    fn close(&mut self);
}

/// Cliente compatible con `OpenAI().chat.completions.create(stream=True, ...)`.
pub struct OpenAiChatClient {
    url: String,
    headers: Vec<(String, String)>,
    timeout: Duration,
    client: Mutex<Option<Client>>,
}

impl OpenAiChatClient {
    /// `api_key` viaja en `Authorization`; nunca se registra ni se pone en errores.
    pub fn new(
        api_key: &str,
        base_url: &str,
        default_headers: &[(String, String)],
        timeout: f64,
    ) -> Self {
        let base = py_strip(base_url).trim_end_matches('/');
        // DECISION: el Python pasa `base_url or None`; vacío = valor por defecto del SDK.
        let base = if base.is_empty() {
            DEFAULT_BASE_URL
        } else {
            base
        };
        let mut headers = default_headers.to_vec();
        headers.push(("authorization".to_string(), format!("Bearer {api_key}")));
        headers.push(("content-type".to_string(), "application/json".to_string()));
        Self {
            url: format!("{base}/chat/completions"),
            headers,
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

    /// Abre la petición y devuelve el flujo de trozos (siempre `stream=true`).
    pub fn stream(&self, request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        let body = build_body(request);
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
            let text = read_text(response).unwrap_or_default();
            let error = LlmError::http(status, format!("Error code: {status} - {text}"));
            return Err(error.with_retry_after(retry_after));
        }
        Ok(Box::new(OpenAiStream::over(response)))
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

/// Cuerpo HTTP: las claves del `request` del `Messenger`, `extra_body` aplanado
/// y `stream` forzado. Las claves privadas (`_…`) y `timeout` no viajan.
pub fn build_body(request: &Value) -> Value {
    let mut body = Map::new();
    if let Value::Object(map) = request {
        for (key, value) in map {
            if key.starts_with('_') || key == "timeout" || key == "stream" {
                continue;
            }
            if key == "extra_body" {
                if let Value::Object(extra) = value {
                    for (extra_key, extra_value) in extra {
                        body.insert(extra_key.clone(), extra_value.clone());
                    }
                }
                continue;
            }
            body.insert(key.clone(), value.clone());
        }
    }
    body.insert("stream".to_string(), Value::Bool(true));
    Value::Object(body)
}

/// Petición de `_complete_text` del condensador (respuesta corta, temperatura 0).
pub(crate) fn condenser_request(
    provider: &str,
    model: &str,
    messages: &[Value],
    max_tokens: u32,
) -> Value {
    let mut request = json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0,
        "timeout": 10,
    });
    if provider == "opencode" || provider == "openai" {
        request["extra_body"] = json!({"reasoning_effort": "none"});
    }
    request
}

impl TextCompleter for OpenAiChatClient {
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

/// Flujo de Chat Completions: eventos `data:` con JSON y `[DONE]` al final.
pub struct OpenAiStream<R: BufRead> {
    reader: Option<SseReader<R>>,
    done: bool,
}

impl<R: BufRead> OpenAiStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: Some(SseReader::new(reader)),
            done: false,
        }
    }
}

impl<R: Read> OpenAiStream<BufReader<R>> {
    /// Envuelve la respuesta HTTP en un flujo de trozos.
    pub fn over(reader: R) -> Self {
        Self::new(BufReader::new(reader))
    }
}

impl<R: BufRead> Iterator for OpenAiStream<R> {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            let payload = match self.reader.as_mut()?.next_payload()? {
                Ok(payload) => payload,
                Err(error) => return Some(Err(self.fail(LlmError::other(error.to_string())))),
            };
            let text = py_strip(&payload);
            if text.is_empty() {
                continue;
            }
            if text == "[DONE]" {
                self.done = true;
                self.reader = None;
                return None;
            }
            let event = match pyloads::loads(text) {
                Ok(Value::Object(map)) => map,
                // DECISION: el SDK solo espera objetos; un JSON suelto se ignora.
                Ok(_) => continue,
                Err(error) => {
                    let message = format!("No pude leer el evento SSE del proveedor: {error}");
                    return Some(Err(self.fail(LlmError::other(message))));
                }
            };
            if event.contains_key("error") {
                return Some(Err(self.fail(error_from_event(&event))));
            }
            if let Some(chunk) = chunk_from_event(&event) {
                return Some(Ok(chunk));
            }
        }
    }
}

impl<R: BufRead> OpenAiStream<R> {
    fn fail(&mut self, error: LlmError) -> LlmError {
        self.done = true;
        self.reader = None;
        error
    }
}

impl<R: BufRead + Send> ChatStream for OpenAiStream<R> {
    fn close(&mut self) {
        self.done = true;
        self.reader = None;
    }
}

fn chunk_from_event(event: &Map<String, Value>) -> Option<Chunk> {
    let choice = event.get("choices").and_then(Value::as_array)?.first()?;
    let delta = choice.get("delta").and_then(Value::as_object);
    let content = delta
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let tool_calls = delta
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(tool_call_delta).collect())
        .unwrap_or_default();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let usage = event.get("usage").filter(|value| !value.is_null()).cloned();
    Some(Chunk {
        content,
        tool_calls,
        finish_reason,
        usage,
    })
}

fn tool_call_delta(value: &Value) -> Option<ToolCallDelta> {
    let object = value.as_object()?;
    // DECISION: el SDK usa `usize`; un índice negativo imposible en la práctica
    // se trata como 0 para no perder la llamada.
    let index =
        usize::try_from(object.get("index").map_or(0, |value| int_value(value, 0))).unwrap_or(0);
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let function = object.get("function").and_then(Value::as_object);
    let name = function
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let arguments = function
        .and_then(|function| function.get("arguments"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(ToolCallDelta {
        index,
        id,
        name,
        arguments,
    })
}

fn error_from_event(event: &Map<String, Value>) -> LlmError {
    // DECISION: el SDK no documenta un texto para el error en mitad del flujo;
    // se usa su mismo formato `Error code: …` para que la clasificación funcione.
    let error = event.get("error").cloned().unwrap_or(Value::Null);
    let code = event
        .get("code")
        .or_else(|| error.get("code"))
        .and_then(|value| u16::try_from(int_value(value, 0)).ok());
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| python_str(&error));
    let status = code.unwrap_or(500);
    LlmError::http(status, format!("Error code: {status} - {message}"))
}

pub(crate) fn read_text(response: Response) -> Result<String, LlmError> {
    let mut raw = Vec::new();
    let mut response = response;
    response
        .read_to_end(&mut raw)
        .map_err(|error| LlmError::other(error.to_string()))?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

pub(crate) fn retry_after(response: &Response) -> Option<String> {
    response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub(crate) fn map_transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::timeout(error.to_string())
    } else if error.is_connect() {
        LlmError::connection(error.to_string())
    } else {
        LlmError::other(error.to_string())
    }
}

pub(crate) fn seconds_or_default(value: f64, default: f64) -> Duration {
    if value.is_finite() && value > 0.0 {
        Duration::from_secs_f64(value)
    } else {
        Duration::from_secs_f64(default)
    }
}

pub(crate) fn request_timeout(default: Duration, request: &Value) -> Duration {
    let seconds = request.get("timeout").and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => py_strip(text).parse().ok(),
        _ => None,
    });
    seconds
        .filter(|value| value.is_finite() && *value > 0.0)
        .map_or(default, Duration::from_secs_f64)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::sse::testing::FakeServer;

    fn chunks(body: &str) -> Vec<Chunk> {
        OpenAiStream::new(Cursor::new(body.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .expect("flujo SSE válido")
    }

    #[test]
    fn parses_text_tool_calls_finish_reason_and_usage() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hola\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",",
            "\"function\":{\"name\":\"web\",\"arguments\":\"{\\\"q\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,",
            "\"function\":{\"arguments\":\"\\\"x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],",
            "\"usage\":{\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );
        let result = chunks(body);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content.as_deref(), Some("Hola"));
        assert_eq!(result[1].tool_calls[0].index, 0);
        assert_eq!(result[1].tool_calls[0].id.as_deref(), Some("c1"));
        assert_eq!(result[1].tool_calls[0].name.as_deref(), Some("web"));
        assert_eq!(result[1].tool_calls[0].arguments, "{\"q\":");
        assert_eq!(result[2].finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(result[2].usage.as_ref().expect("uso")["total_tokens"], 5);
    }

    #[test]
    fn skips_events_without_choices_and_stops_at_done() {
        let body = "data: {\"choices\":[]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\ndata: [DONE]\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"ignorado\"}}]}\n\n";
        let result = chunks(body);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.as_deref(), Some("x"));
    }

    #[test]
    fn body_flattens_extra_body_and_hides_private_keys() {
        let request = json!({
            "model": "m",
            "messages": [],
            "max_tokens": 10,
            "stream": true,
            "temperature": 0.2,
            "extra_body": {"reasoning_effort": "none"},
            "_volatile": "datos",
            "_notices": [],
            "timeout": 5,
        });
        let body = build_body(&request);
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["reasoning_effort"], json!("none"));
        assert_eq!(body["model"], json!("m"));
        assert!(body.get("_volatile").is_none());
        assert!(body.get("_notices").is_none());
        assert!(body.get("timeout").is_none());
    }

    #[test]
    fn streams_from_a_real_http_server() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"uno \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"dos\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let server = FakeServer::serve("200 OK", &[("content-type", "text/event-stream")], body);
        let client = OpenAiChatClient::new("clave", &server.base_url, &[], 5.0);
        let request = json!({"model": "m", "messages": [{"role": "user", "content": "hola"}]});
        let text: String = client
            .stream(&request)
            .expect("stream abierto")
            .filter_map(Result::ok)
            .filter_map(|chunk| chunk.content)
            .collect();
        assert_eq!(text, "uno dos");
    }

    #[test]
    fn http_error_keeps_status_and_retry_after() {
        let server = FakeServer::serve(
            "429 Too Many Requests",
            &[("retry-after", "7"), ("content-type", "application/json")],
            "{\"error\":{\"message\":\"slow down\"}}",
        );
        let client = OpenAiChatClient::new("clave", &server.base_url, &[], 5.0);
        let request = json!({"model": "m", "messages": []});
        let error = match client.stream(&request) {
            Ok(_) => panic!("el servidor debía rechazar la petición"),
            Err(error) => error,
        };
        assert_eq!(error.status, Some(429));
        assert_eq!(error.retry_after.as_deref(), Some("7"));
        assert!(error.message.starts_with("Error code: 429 - "));
        assert!(crate::errors::friendly_llm_error(&error, "OpenAI", "m")
            .contains("Espera unos 7 segundos o"));
    }

    #[test]
    fn condenser_reads_a_short_streamed_reply() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"1\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"2\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let server = FakeServer::serve("200 OK", &[], body);
        let client = OpenAiChatClient::new("clave", &server.base_url, &[], 5.0);
        let messages = [json!({"role": "user", "content": "línea"})];
        assert_eq!(
            client
                .complete_text("openai", "m", &messages, 8)
                .expect("respuesta del condensador"),
            "12"
        );
    }
}
