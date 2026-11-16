//! `_guard_second_call`: corta el turno antes de la segunda llamada al modelo
//! (F8-02).
//!
//! Se envuelve el cliente del proveedor: la primera llamada pasa al cliente
//! real y cualquier otra devuelve un error que el eval reconoce como el corte
//! esperado, de modo que las herramientas pedidas quedan registradas en el
//! historial y no se ejecuta ninguna.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ez_llm::anthropic::AnthropicChatClient;
use ez_llm::errors::LlmError;
use ez_llm::messenger::{LlmClient, LlmSettings, Messenger};
use ez_llm::openai::{ChatStream, OpenAiChatClient};
use serde_json::Value;

/// Texto del corte; no llega al usuario (el eval lo descarta).
const STOP_TEXT: &str = "el eval corta antes de la segunda llamada al modelo";

/// Cliente con presupuesto de una sola llamada al proveedor.
pub(super) struct SecondCallGuard {
    inner: Arc<dyn LlmClient>,
    calls: AtomicUsize,
    stopped: AtomicBool,
}

impl SecondCallGuard {
    /// `True` cuando el turno se cortó por la segunda llamada (no es un fallo).
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

impl LlmClient for SecondCallGuard {
    fn stream(&self, request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) >= 1 {
            self.stopped.store(true, Ordering::SeqCst);
            return Err(LlmError::other(STOP_TEXT));
        }
        self.inner.stream(request)
    }

    fn close(&self) {
        self.inner.close();
    }
}

/// Sustituye el cliente del mensajero por el guardián y lo devuelve.
pub(super) fn install(messenger: &Messenger) -> Arc<SecondCallGuard> {
    let settings = messenger.settings();
    let guard = Arc::new(SecondCallGuard {
        inner: build_client(&settings),
        calls: AtomicUsize::new(0),
        stopped: AtomicBool::new(false),
    });
    messenger.set_client(Arc::clone(&guard) as Arc<dyn LlmClient>);
    guard
}

/// DECISION: `build_client` de `ez-llm` es `pub(crate)` y `ez-app` no puede
/// tocarlo; se copian sus mismas cabeceras, clave por omisión y tiempos.
fn build_client(settings: &LlmSettings) -> Arc<dyn LlmClient> {
    let mut headers = vec![("user-agent".to_string(), "EzAgent-Voice/0.1".to_string())];
    if settings.provider == "opencode" {
        headers.push(("x-opencode-session".to_string(), session_hex()));
    }
    if settings.provider == "anthropic" {
        if let Ok(workspace) = std::env::var("ANTHROPIC_WORKSPACE_ID") {
            if !workspace.is_empty() {
                headers.push(("anthropic-workspace-id".to_string(), workspace));
            }
        }
    }
    let api_key = if settings.api_key.is_empty() {
        "sin-clave"
    } else {
        settings.api_key.as_str()
    };
    if settings.api_style == "anthropic" {
        Arc::new(AnthropicChatClient::new(
            api_key,
            &settings.base_url,
            &headers,
            settings.request_timeout(),
            true,
        ))
    } else {
        Arc::new(OpenAiChatClient::new(
            api_key,
            &settings.base_url,
            &headers,
            settings.request_timeout(),
        ))
    }
}

/// `uuid4().hex` para `x-opencode-session`: opaco y distinto por corrida.
fn session_hex() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:032x}{:08x}{count:016x}", std::process::id())
}
