//! Port of `friendly_llm_error in llm.py` (F4-06).
//!
//! Un solo tipo de error para los dos clientes (`LlmError`), con el código HTTP
//! y el `retry-after` que el Python lee de la excepción del SDK, y la frase
//! corta que se dice por voz. La clave de API nunca aparece en estos textos.

use std::fmt;

use ez_core::pyvalue::head_chars;
use ez_core::redact::redact;
use serde_json::Value;

/// `type(value).__name__` de CPython para los textos de error que lo incluyen.
pub(crate) fn py_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// De qué clase viene el fallo; sustituye a `type(exc).__name__` del Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// `LLMTurnTimeout`: el turno agotó su presupuesto total.
    TurnTimeout,
    /// Respuesta HTTP con código de error.
    Http,
    /// `APIConnectionError` / `ConnectError`.
    Connection,
    /// `APITimeoutError` / `ReadTimeout`.
    Timeout,
    /// Evento `error` dentro del flujo (APIError o RuntimeError, según cliente).
    Api,
    /// Objeto donde el Python esperaba un `dict` (`AttributeError`).
    Attribute,
    /// Número o índice ilegible (`ValueError`).
    Value,
    /// JSON ilegible del flujo (`JSONDecodeError`).
    Protocol,
    /// Cualquier otro fallo del proveedor o del flujo.
    Other,
}

/// Fallo de un cliente LLM, con lo que `friendly_llm_error` necesita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmError {
    pub message: String,
    pub status: Option<u16>,
    pub retry_after: Option<String>,
    pub kind: ErrorKind,
}

impl LlmError {
    /// Error HTTP con su código; el texto es el mensaje final para el usuario.
    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: Some(status),
            retry_after: None,
            kind: ErrorKind::Http,
        }
    }

    /// Error de conexión (`ConnectError` en el Python).
    pub fn connection(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
            kind: ErrorKind::Connection,
        }
    }

    /// Error de tiempo de espera de red o de lectura.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
            kind: ErrorKind::Timeout,
        }
    }

    /// `LLMTurnTimeout`: se devuelve su texto tal cual por voz.
    pub fn turn_timeout(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
            kind: ErrorKind::TurnTimeout,
        }
    }

    /// Cualquier otro fallo (flujo cortado, JSON ilegible, etcétera).
    pub fn other(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
            kind: ErrorKind::Other,
        }
    }

    /// Fallo con una clase concreta y sin código HTTP (eventos `error` del flujo).
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
            kind,
        }
    }

    /// Añade la cabecera `retry-after` que trae una respuesta 429.
    pub fn with_retry_after(mut self, value: Option<String>) -> Self {
        self.retry_after = value;
        self
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LlmError {}

/// `_status_of`: el código de la excepción o, si no lo trae, el del texto.
fn status_of(error: &LlmError) -> Option<u16> {
    if error.status.is_some() {
        return error.status;
    }
    [401, 403, 404, 413, 429].into_iter().find(|code| {
        error.message.contains(&format!(" {code}")) || error.message.contains(&format!("{code}:"))
    })
}

/// `int(after)` cuando el valor es todo dígitos (`str.isdigit()`).
fn retry_seconds(value: Option<&str>) -> Option<i64> {
    let value = value?;
    if value.is_empty() || !value.chars().all(char::is_numeric) {
        return None;
    }
    ez_core::config::python_int(value)
}

/// Frase corta para decir por voz cuando falla el proveedor del LLM.
pub fn friendly_llm_error(error: &LlmError, label: &str, model: &str) -> String {
    if error.kind == ErrorKind::TurnTimeout {
        return error.message.clone();
    }
    let status = status_of(error);
    if status == Some(429) {
        let retry = match retry_seconds(error.retry_after.as_deref()) {
            Some(seconds) => format!(" Espera unos {seconds} segundos o"),
            None => " Espera un momento o".to_string(),
        };
        return format!(
            "{label} me limitó por demasiadas peticiones o tokens por minuto.{retry} \
             elige otro modelo o proveedor en ajustes."
        );
    }
    if status == Some(413) {
        return format!(
            "La petición es más grande de lo que permite tu plan de {label} por minuto. \
             Usa otro proveedor o un plan con más tokens."
        );
    }
    let text = error.message.to_lowercase();
    if status == Some(500)
        && text.contains("tool call")
        && (text.contains("parse") || text.contains("json"))
    {
        return format!(
            "{label} cortó una llamada a herramienta a la mitad y no pudo leerla. \
             Ya limpié el historial; vuelve a intentarlo."
        );
    }
    if status == Some(404) {
        return format!("{label} no reconoce el modelo {model}. Revisa el nombre en ajustes.");
    }
    if status == Some(401) || status == Some(403) {
        return format!("La clave de {label} no es válida o no tiene acceso. Revísala en ajustes.");
    }
    if matches!(error.kind, ErrorKind::Connection | ErrorKind::Timeout) {
        return format!(
            "No pude conectar con {label}. Revisa la conexión o la dirección de la API."
        );
    }
    format!("{label}: {}", head_chars(&redact(&error.message), 300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_uses_retry_after() {
        let error = LlmError::http(429, "slow down").with_retry_after(Some("7".into()));
        assert_eq!(
            friendly_llm_error(&error, "OpenAI", "gpt"),
            "OpenAI me limitó por demasiadas peticiones o tokens por minuto. \
             Espera unos 7 segundos o elige otro modelo o proveedor en ajustes."
        );
        let error = LlmError::http(429, "slow down");
        assert!(friendly_llm_error(&error, "OpenAI", "gpt").contains("Espera un momento o"));
        let error = LlmError::http(429, "slow").with_retry_after(Some("pronto".into()));
        assert!(friendly_llm_error(&error, "OpenAI", "gpt").contains("Espera un momento o"));
    }

    #[test]
    fn status_is_read_from_the_text_when_missing() {
        let error = LlmError::other("Error code: 429 - too many");
        assert_eq!(status_of(&error), Some(429));
        assert!(friendly_llm_error(&error, "Groq", "llama").contains("me limitó"));
    }

    #[test]
    fn payload_too_large() {
        let error = LlmError::http(413, "too big");
        assert_eq!(
            friendly_llm_error(&error, "Cerebras", "gpt"),
            "La petición es más grande de lo que permite tu plan de Cerebras por minuto. \
             Usa otro proveedor o un plan con más tokens."
        );
    }

    #[test]
    fn cut_tool_call_json() {
        let error = LlmError::http(500, "Tool call arguments failed to parse as JSON");
        assert_eq!(
            friendly_llm_error(&error, "OpenAI", "gpt"),
            "OpenAI cortó una llamada a herramienta a la mitad y no pudo leerla. \
             Ya limpié el historial; vuelve a intentarlo."
        );
        let other = LlmError::http(500, "server exploded");
        assert!(friendly_llm_error(&other, "OpenAI", "gpt").starts_with("OpenAI: "));
    }

    #[test]
    fn model_and_key_errors() {
        let not_found = LlmError::http(404, "missing");
        assert_eq!(
            friendly_llm_error(&not_found, "Anthropic", "claude-x"),
            "Anthropic no reconoce el modelo claude-x. Revisa el nombre en ajustes."
        );
        for status in [401, 403] {
            let denied = LlmError::http(status, "denied");
            assert_eq!(
                friendly_llm_error(&denied, "OpenAI", "gpt"),
                "La clave de OpenAI no es válida o no tiene acceso. Revísala en ajustes."
            );
        }
    }

    #[test]
    fn connection_timeout_and_turn_timeout() {
        let refused = LlmError::connection("connection refused");
        assert_eq!(
            friendly_llm_error(&refused, "OpenAI", "gpt"),
            "No pude conectar con OpenAI. Revisa la conexión o la dirección de la API."
        );
        let timeout = LlmError::timeout("timed out");
        assert!(friendly_llm_error(&timeout, "OpenAI", "gpt").starts_with("No pude conectar"));
        let turn = LlmError::turn_timeout("El turno superó el tiempo máximo configurado.");
        assert_eq!(
            friendly_llm_error(&turn, "OpenAI", "gpt"),
            "El turno superó el tiempo máximo configurado."
        );
    }

    #[test]
    fn fallback_redacts_and_truncates() {
        let error = LlmError::other(format!("clave sk-secreta123 {}", "x".repeat(400)));
        let text = friendly_llm_error(&error, "OpenAI", "gpt");
        assert!(text.starts_with("OpenAI: "));
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("sk-secreta123"));
        assert_eq!(text.chars().count(), "OpenAI: ".chars().count() + 300);
    }
}
