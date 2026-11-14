//! Port of `src/ezagent/voice/stt.py` (F4-03).
//!
//! Cliente HTTP de whisper.cpp con cliente persistente (F12), ruta alternativa
//! `/inference` ↔ `/v1/audio/transcriptions`, descubrimiento de modelos locales
//! y filtro de alucinaciones típicas de Whisper.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use ez_core::config::{int_value, python_str, truthy, VoiceConfig};
use ez_core::pyvalue::{collapse_spaces, py_strip, strip_accents};

use crate::dictionary::{correct_transcription, transcription_hint};

mod models;

pub use self::models::{
    available_whisper_models, whisper_home, whisper_model_path, whisper_models_dir,
};

/// `STTError`: fallo del cliente de transcripción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SttError {
    message: String,
}

impl SttError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for SttError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SttError {}

// F12: un cliente por proceso reutiliza conexión y TLS entre turnos. El
// endpoint que funcionó se recuerda para no repetir el 404 de la ruta alterna.
static CLIENT: OnceLock<Mutex<Option<reqwest::blocking::Client>>> = OnceLock::new();
static WORKING_ENDPOINT: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<reqwest::blocking::Client>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

fn endpoint_cache() -> &'static Mutex<HashMap<String, String>> {
    WORKING_ENDPOINT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `_client()`: cliente compartido, creado la primera vez que hace falta.
fn shared_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    let mut slot = lock(client_slot());
    if let Some(client) = slot.as_ref() {
        return Ok(client.clone());
    }
    let client = reqwest::blocking::Client::builder().build()?;
    *slot = Some(client.clone());
    Ok(client)
}

/// Cierra el cliente persistente (pruebas y fin de proceso).
pub fn close_client() {
    // DECISION: Rust no tiene `atexit`; el cliente se libera al terminar el proceso.
    *lock(client_slot()) = None;
    lock(endpoint_cache()).clear();
}

/// `_remember_endpoint`.
fn remember_endpoint(primary: &str, url: &str) {
    lock(endpoint_cache()).insert(primary.to_string(), url.to_string());
}

/// `_forget_endpoint`.
fn forget_endpoint(primary: &str, url: &str) {
    let mut cache = lock(endpoint_cache());
    if cache.get(primary).is_some_and(|cached| cached == url) {
        cache.remove(primary);
    }
}

/// `_endpoints`: primaria, alternativa y, primero, la que ya funcionó.
fn endpoints(primary: &str) -> Vec<String> {
    let mut found = vec![primary.to_string()];
    if let Some(fallback) = derive_fallback(primary) {
        found.push(fallback);
    }
    let cached = lock(endpoint_cache()).get(primary).cloned();
    if let Some(cached) = cached {
        if let Some(index) = found.iter().position(|url| url == &cached) {
            let url = found.remove(index);
            found.insert(0, url);
        }
    }
    found
}

/// Partes de una URL, con la semántica de `urllib.parse.urlsplit` que usa el
/// Python (sin dependencia extra: solo esquema, autoridad, ruta y query).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UrlParts {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub query: String,
}

impl UrlParts {
    pub(crate) fn split(url: &str) -> Self {
        let (scheme, rest) = match url.split_once("://") {
            Some((scheme, rest)) if is_scheme(scheme) => (scheme.to_lowercase(), rest),
            _ => (String::new(), url),
        };
        let (netloc, remainder) = if scheme.is_empty() {
            (String::new(), rest)
        } else if let Some(index) = rest.find(['/', '?', '#']) {
            (rest[..index].to_string(), &rest[index..])
        } else {
            (rest.to_string(), "")
        };
        let without_fragment = remainder.split('#').next().unwrap_or(remainder);
        let (path, query) = without_fragment
            .split_once('?')
            .unwrap_or((without_fragment, ""));
        Self {
            scheme,
            netloc,
            path: path.to_string(),
            query: query.to_string(),
        }
    }

    pub(crate) fn unsplit(&self) -> String {
        let mut url = String::new();
        if !self.scheme.is_empty() {
            url.push_str(&self.scheme);
            url.push(':');
        }
        if !self.netloc.is_empty() {
            url.push_str("//");
            url.push_str(&self.netloc);
        }
        url.push_str(&self.path);
        if !self.query.is_empty() {
            url.push('?');
            url.push_str(&self.query);
        }
        url
    }

    /// `parts.hostname`: sin credenciales, en minúsculas y sin corchetes IPv6.
    pub(crate) fn host(&self) -> Option<String> {
        let authority = self.netloc.rsplit('@').next().unwrap_or(&self.netloc);
        if authority.is_empty() {
            return None;
        }
        if let Some(rest) = authority.strip_prefix('[') {
            let (host, _) = rest.split_once(']')?;
            return Some(host.to_lowercase());
        }
        let host = match authority.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|character| character.is_ascii_digit()) => host,
            _ => authority,
        };
        Some(host.to_lowercase())
    }

    /// `parts.port`: `None` si falta o no es un puerto válido.
    pub(crate) fn port(&self) -> Option<u16> {
        let authority = self.netloc.rsplit('@').next().unwrap_or(&self.netloc);
        let port = if let Some(rest) = authority.strip_prefix('[') {
            rest.split_once(']')?.1.strip_prefix(':')?
        } else {
            authority.rsplit_once(':')?.1
        };
        port.parse().ok()
    }

    /// `parts.port or (443 if parts.scheme == "https" else 80)`.
    pub(crate) fn default_port(&self) -> u16 {
        self.port()
            .unwrap_or(if self.scheme == "https" { 443 } else { 80 })
    }
}

fn is_scheme(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

/// `_derive_fallback`: alterna entre `/inference` y la ruta OpenAI.
pub fn derive_fallback(url: &str) -> Option<String> {
    let mut parts = UrlParts::split(url);
    let path = parts.path.trim_end_matches('/');
    parts.path = if let Some(base) = path.strip_suffix("/inference") {
        format!("{base}/v1/audio/transcriptions")
    } else {
        let base = path.strip_suffix("/v1/audio/transcriptions")?;
        format!("{base}/inference")
    };
    Some(parts.unsplit())
}

fn python_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Value::String(text) => py_strip(text).parse().ok(),
        _ => None,
    }
}

fn timeout_secs(config: &VoiceConfig) -> f64 {
    let value = config
        .get("stt_timeout_secs")
        .and_then(python_float)
        .unwrap_or(60.0);
    if value == 0.0 || !value.is_finite() {
        60.0
    } else {
        value
    }
}

fn config_text(config: &VoiceConfig, key: &str) -> String {
    config
        .get(key)
        .filter(|value| !value.is_null())
        .map(python_str)
        .unwrap_or_default()
}

fn config_truthy(config: &VoiceConfig, key: &str, default: bool) -> bool {
    config.get(key).map_or(default, truthy)
}

fn agent_names(config: &VoiceConfig) -> Vec<String> {
    match config.get("agents") {
        Some(Value::Object(agents)) => agents.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// `_beam_size`: beam válido para whisper.cpp (1–8); un valor raro vuelve a 5.
fn beam_size(value: Option<&Value>) -> i64 {
    value.map_or(5, |value| int_value(value, 5)).clamp(1, 8)
}

/// `_form`: campos multipart de la petición, en el orden del Python.
fn form(config: &VoiceConfig) -> Result<Vec<(String, String)>, SttError> {
    let mut model = config_text(config, "stt_model");
    if model.is_empty() {
        model = "ggml-large-v3-turbo-q8_0".to_string();
    }
    let mut fields = vec![
        ("model".to_string(), model),
        ("language".to_string(), "es".to_string()),
        ("response_format".to_string(), "json".to_string()),
        ("temperature".to_string(), "0.00".to_string()),
    ];
    let beam = beam_size(config.get("stt_beam_size"));
    if beam > 1 {
        fields.push(("beam_size".to_string(), beam.to_string()));
    }
    let mut prompt = config_text(config, "stt_prompt");
    if config_truthy(config, "stt_vocabulary_hints", true) {
        prompt = transcription_hint(
            &prompt,
            &config_text(config, "stt_dictionary"),
            &agent_names(config),
        )
        .map_err(|error| {
            SttError::new(format!("Diccionario de transcripción inválido: {error}"))
        })?;
    }
    if !prompt.is_empty() {
        fields.push(("prompt".to_string(), prompt));
    }
    Ok(fields)
}

fn dictionary_error(error: crate::dictionary::DictionaryError) -> SttError {
    SttError::new(format!("Diccionario de transcripción inválido: {error}"))
}

/// `_text`: usa `text` si la respuesta es JSON y el cuerpo entero si no.
fn text_from_body(config: &VoiceConfig, body: &str) -> Result<String, SttError> {
    let text = match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(object)) => object
            .get("text")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new())),
        // DECISION: el Python lanzaría AttributeError con un JSON que no es
        // objeto; aquí se usa el cuerpo como texto para no perder la frase.
        Ok(_) | Err(_) => Value::String(body.to_string()),
    };
    let collapsed = collapse_spaces(&python_str(&text));
    correct_transcription(&collapsed, &config_text(config, "stt_dictionary"))
        .map_err(dictionary_error)
}

fn boundary() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("----ezagent-boundary-{count}-{nanos}")
}

/// Cuerpo multipart construido a mano (reqwest sin la característica `multipart`).
fn multipart(fields: &[(String, String)], wav_bytes: &[u8], boundary: &str) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"utterance.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

fn post_form(
    client: Option<&reqwest::blocking::Client>,
    url: &str,
    fields: &[(String, String)],
    wav_bytes: &[u8],
    timeout: Duration,
) -> Result<reqwest::blocking::Response, reqwest::Error> {
    // DECISION: no se porta el reemplazo de `httpx.post` (costura de las
    // pruebas del Python); el parámetro `client` cubre la inyección.
    let client = match client {
        Some(client) => client.clone(),
        None => shared_client()?,
    };
    let boundary = boundary();
    let body = multipart(fields, wav_bytes, &boundary);
    client
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .timeout(timeout)
        .body(body)
        .send()
}

fn no_connection_error(last_error: &str) -> SttError {
    SttError::new(format!(
        "No puedo conectar con whisper-server. Arranca scripts\\start-whisper.ps1. {last_error}"
    ))
}

/// `transcribe`: envía el WAV a whisper.cpp y devuelve el texto corregido.
///
/// `client` es la costura de pruebas del Python (`client=`); en producción se
/// usa el cliente persistente del módulo.
pub fn transcribe(
    config: &VoiceConfig,
    wav_bytes: &[u8],
    client: Option<&reqwest::blocking::Client>,
) -> Result<String, SttError> {
    let fields = form(config)?;
    let budget = timeout_secs(config);
    if budget <= 0.0 {
        return Err(no_connection_error(""));
    }
    let deadline = Instant::now() + Duration::from_secs_f64(budget);
    let primary = config_text(config, "whisper_url");
    let endpoints = endpoints(&primary);
    let mut last_error = String::new();
    for (index, url) in endpoints.iter().enumerate() {
        // Un solo presupuesto para toda la llamada: el reintento de ruta usa
        // lo que quede, no un timeout completo nuevo.
        let timeout = if index == 0 {
            Duration::from_secs_f64(budget)
        } else {
            match deadline.checked_duration_since(Instant::now()) {
                Some(remaining) if !remaining.is_zero() => remaining,
                _ => break,
            }
        };
        let response = match post_form(client, url, &fields, wav_bytes, timeout) {
            Ok(response) => response,
            Err(error) if error.is_connect() => {
                forget_endpoint(&primary, url);
                last_error = format!("{url}: {error}");
                continue;
            }
            Err(error) if error.is_timeout() => {
                return Err(SttError::new(format!(
                    "whisper-server no respondió a tiempo ({url}): {error}"
                )));
            }
            Err(error) => {
                return Err(SttError::new(format!("whisper-server: {error}")));
            }
        };
        let status = response.status().as_u16();
        if status == 404 {
            forget_endpoint(&primary, url);
            last_error = format!("{url}: 404");
            continue;
        }
        let body = response
            .text()
            .map_err(|error| SttError::new(format!("whisper-server: {error}")))?;
        if status >= 400 {
            let shown: String = body.chars().take(200).collect();
            return Err(SttError::new(format!(
                "whisper-server respondió {status}: {shown}"
            )));
        }
        remember_endpoint(&primary, url);
        return text_from_body(config, &body);
    }
    Err(no_connection_error(&last_error))
}

// Frases que Whisper inventa con silencio o ruido (vienen de subtítulos de
// vídeos). Nadie se las dice a un agente de código: se descartan siempre.
const ALWAYS_HALLUCINATED: [&str; 7] = [
    "gracias por ver el video",
    "gracias por ver",
    "gracias por vernos",
    "suscribete",
    "suscribete al canal",
    "no olvides suscribirte",
    "hasta la proxima",
];
// Cortesías sueltas: pueden ser reales («gracias»), así que solo se descartan
// cuando el audio era casi silencio o ruido (`weak_audio`).
const HALLUCINATED_IF_WEAK: [&str; 5] = [
    "gracias",
    "muchas gracias",
    "gracias a todos",
    "hasta luego",
    "chao",
];

/// `_norm`: minúsculas, sin tildes, solo `[a-z0-9 ]` y espacios colapsados.
fn norm(text: &str) -> String {
    let raw = strip_accents(&text.to_lowercase());
    let cleaned: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect();
    collapse_spaces(&cleaned)
}

/// `True` si `text` es una frase típicamente inventada por Whisper.
pub fn is_hallucination(text: &str, weak_audio: bool) -> bool {
    let key = norm(text);
    if key.is_empty() {
        return false;
    }
    if ALWAYS_HALLUCINATED.contains(&key.as_str()) || key.contains("amara org") {
        return true;
    }
    weak_audio && HALLUCINATED_IF_WEAK.contains(&key.as_str())
}

#[cfg(test)]
mod tests;
