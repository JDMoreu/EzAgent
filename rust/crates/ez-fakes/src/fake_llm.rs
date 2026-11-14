//! Servidor falso de Chat Completions (OpenAI) y Messages (Anthropic) (F1-11).
//!
//! Escucha solo en `127.0.0.1`, sirve SSE guiado por un escenario JSON y anota
//! cada petición (cuerpo exacto, menos credenciales) en `requests.jsonl`.
//! Rutas: `…/chat/completions` (OpenAI) y `…/messages` (Anthropic), con
//! `GET /health` y `GET /v1/models` para las comprobaciones manuales.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use tracing::{info, warn};

mod record;
mod scenario;
mod sse;

pub use scenario::{Scenario, ToolCallSpec, Turn, FALLBACK_TEXT};

use record::RecordEntry;

/// Puerto por defecto de `fake-llm`.
pub const DEFAULT_PORT: u16 = 8790;

/// Proveedor que imita la ruta HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    OpenAi,
    Anthropic,
}

impl Route {
    /// Deduce el protocolo por la ruta; `None` si no es una ruta conocida.
    pub fn of(path: &str) -> Option<Self> {
        let path = path.split('?').next().unwrap_or(path);
        if path.ends_with("/messages") {
            return Some(Self::Anthropic);
        }
        if path.ends_with("/chat/completions") {
            return Some(Self::OpenAi);
        }
        None
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

/// Opciones de línea de comandos.
#[derive(Debug, Clone)]
pub struct Options {
    pub scenario: PathBuf,
    pub record: PathBuf,
    pub port: u16,
}

impl Options {
    /// `--scenario` es obligatorio; `--record` y `--port` tienen valores por defecto.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut scenario = None;
        let mut record = PathBuf::from("requests.jsonl");
        let mut port = DEFAULT_PORT;
        let mut index = 0;
        while index < args.len() {
            let value = args
                .get(index + 1)
                .with_context(|| format!("{} necesita un valor", args[index]))?;
            match args[index].as_str() {
                "--scenario" => scenario = Some(PathBuf::from(value)),
                "--record" => record = PathBuf::from(value),
                "--port" => {
                    port = value
                        .parse::<u16>()
                        .with_context(|| format!("puerto no válido: {value}"))?;
                }
                other => anyhow::bail!("argumento desconocido: {other}"),
            }
            index += 2;
        }
        if port == 0 {
            anyhow::bail!("--port debe ser mayor que 0");
        }
        Ok(Self {
            scenario: scenario.context("falta --scenario <ruta>")?,
            record,
            port,
        })
    }
}

/// Contador de turnos del guion y de peticiones recibidas.
#[derive(Debug, Default)]
struct Position {
    next_turn: usize,
    requests: u64,
}

impl Position {
    fn take(&mut self, scenario: &Scenario) -> (Turn, u64) {
        let turn = scenario.turn(self.next_turn);
        if !scenario.is_fallback(self.next_turn) {
            self.next_turn += 1;
        }
        self.requests += 1;
        (turn, self.requests)
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("cabecera HTTP válida")
}

fn respond(request: Request, status: u16, content_type: &str, body: String) {
    let response = Response::from_data(body.into_bytes())
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-cache"));
    if let Err(error) = request.respond(response) {
        warn!("fake-llm: no pude responder: {error}");
    }
}

fn respond_json(request: Request, status: u16, value: &Value) {
    respond(
        request,
        status,
        "application/json; charset=utf-8",
        value.to_string(),
    );
}

fn respond_error(request: Request, route: Route, turn: &Turn) {
    let value = sse::error_body(route, turn);
    let mut response = Response::from_data(value.to_string().into_bytes())
        .with_status_code(StatusCode(turn.status_code()))
        .with_header(header("Content-Type", "application/json; charset=utf-8"));
    if let Some(seconds) = turn.retry_after {
        response = response.with_header(header("Retry-After", &seconds.to_string()));
    }
    if let Err(error) = request.respond(response) {
        warn!("fake-llm: no pude responder el error: {error}");
    }
}

fn read_body(request: &mut Request) -> String {
    let mut bytes = Vec::new();
    if let Err(error) = request.as_reader().read_to_end(&mut bytes) {
        warn!("fake-llm: no pude leer el cuerpo: {error}");
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn header_pairs(request: &Request) -> Vec<(String, String)> {
    request
        .headers()
        .iter()
        .map(|header| {
            (
                header.field.as_str().as_str().to_string(),
                header.value.as_str().to_string(),
            )
        })
        .collect()
}

fn has_api_key(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        (name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key"))
            && !value.trim().is_empty()
    })
}

fn write_record(path: &Path, entry: &RecordEntry<'_>) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                warn!("fake-llm: no pude crear {}: {error}", parent.display());
                return;
            }
        }
    }
    let line = record::line(entry);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(mut file) => {
            if let Err(error) = writeln!(file, "{line}") {
                warn!("fake-llm: no pude anotar la petición: {error}");
            }
        }
        Err(error) => warn!(
            "fake-llm: no pude abrir {} para anotar: {error}",
            path.display()
        ),
    }
}

fn respond_turn(
    request: Request,
    route: Route,
    turn: &Turn,
    model: &str,
    chunk_chars: usize,
    number: u64,
    stream: bool,
) {
    if stream {
        let body = match route {
            Route::OpenAi => sse::openai_sse(turn, model, chunk_chars, number),
            Route::Anthropic => sse::anthropic_sse(turn, model, chunk_chars, number),
        };
        respond(request, 200, "text/event-stream; charset=utf-8", body);
    } else {
        let value = match route {
            Route::OpenAi => sse::openai_completion(turn, model, number),
            Route::Anthropic => sse::anthropic_message(turn, model, number),
        };
        respond_json(request, 200, &value);
    }
}

fn invalid_body() -> Turn {
    Turn {
        status: 400,
        error: "El cuerpo de la petición no es JSON válido.".to_string(),
        error_type: "invalid_request_error".to_string(),
        ..Turn::default()
    }
}

/// Atiende una petición de completación ya enrutada.
fn completion(
    mut request: Request,
    route: Route,
    scenario: &Scenario,
    state: &mut Position,
    record: &Path,
) {
    let headers = header_pairs(&request);
    let raw_body = read_body(&mut request);
    let body: Result<Value, _> = serde_json::from_str(&raw_body);
    let parsed = body.as_ref().ok();
    let stream = parsed
        .and_then(|value| value.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let model = parsed
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .unwrap_or(&scenario.model)
        .to_string();
    let path = request.url().split('?').next().unwrap_or("").to_string();
    let (turn, number) = state.take(scenario);
    write_record(
        record,
        &RecordEntry {
            number,
            route,
            method: "POST",
            path: &path,
            stream,
            headers: &headers,
            has_api_key: has_api_key(&headers),
            raw_body: &raw_body,
            body: parsed,
        },
    );
    if parsed.is_none() {
        respond_error(request, route, &invalid_body());
        return;
    }
    if turn.delay_ms > 0 {
        sleep(Duration::from_millis(turn.delay_ms));
    }
    if turn.is_error() {
        respond_error(request, route, &turn);
        return;
    }
    respond_turn(
        request,
        route,
        &turn,
        &model,
        scenario.chunk_chars,
        number,
        stream,
    );
}

/// Abre el servidor en `127.0.0.1:<port>` (nunca en otra interfaz).
pub fn bind(port: u16) -> Result<Server> {
    Server::http(("127.0.0.1", port))
        .map_err(|error| anyhow::anyhow!("no pude abrir el puerto {port}: {error}"))
}

/// Sirve hasta que el proceso termine.
pub fn run(options: &Options) -> Result<()> {
    let scenario = Scenario::load(&options.scenario)?;
    let server = bind(options.port)?;
    info!(
        "fake-llm escuchando en http://127.0.0.1:{} ({} turnos)",
        options.port,
        scenario.turns.len()
    );
    let mut state = Position::default();
    for request in server.incoming_requests() {
        handle(request, &scenario, &mut state, &options.record);
    }
    Ok(())
}

fn handle(request: Request, scenario: &Scenario, state: &mut Position, record: &Path) {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or("").to_string();
    info!("fake-llm: {method} {path}");
    if method == Method::Get && matches!(path.as_str(), "/" | "/health" | "/healthz") {
        respond_json(
            request,
            200,
            &json!({"status": "ok", "model": scenario.model}),
        );
        return;
    }
    if method == Method::Get && path.ends_with("/models") {
        respond_json(
            request,
            200,
            &json!({
                "object": "list",
                "data": [{"id": scenario.model, "object": "model", "owned_by": "fake"}],
            }),
        );
        return;
    }
    if method == Method::Post {
        if let Some(route) = Route::of(&path) {
            completion(request, route, scenario, state, record);
            return;
        }
    }
    respond_json(
        request,
        404,
        &json!({"error": {"message": "Ruta desconocida en fake-llm."}}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_are_detected_by_suffix() {
        assert_eq!(Route::of("/v1/chat/completions"), Some(Route::OpenAi));
        assert_eq!(Route::of("/chat/completions?x=1"), Some(Route::OpenAi));
        assert_eq!(Route::of("/v1/messages"), Some(Route::Anthropic));
        assert_eq!(Route::of("/anthropic/v1/messages"), Some(Route::Anthropic));
        assert_eq!(Route::of("/v1/models"), None);
        assert_eq!(Route::OpenAi.name(), "openai");
    }

    #[test]
    fn script_advances_and_fallback_repeats_without_consuming() {
        let scenario =
            Scenario::from_json(r#"{"turns": [{"content": "uno"}, {"content": "dos"}]}"#)
                .expect("escenario");
        let mut state = Position::default();
        assert_eq!(state.take(&scenario).0.content, "uno");
        assert_eq!(state.take(&scenario).0.content, "dos");
        assert_eq!(state.take(&scenario).0.content, FALLBACK_TEXT);
        assert_eq!(state.take(&scenario).0.content, FALLBACK_TEXT);
        assert_eq!(state.requests, 4);
    }

    #[test]
    fn options_require_a_scenario() {
        let options = Options::parse(&[
            "--scenario".to_string(),
            "s.json".to_string(),
            "--port".to_string(),
            "9000".to_string(),
        ])
        .expect("opciones");
        assert_eq!(options.port, 9000);
        assert_eq!(options.record, PathBuf::from("requests.jsonl"));
        assert!(Options::parse(&[]).is_err());
        assert!(Options::parse(&["--port".into(), "x".into()]).is_err());
        assert!(Options::parse(&[
            "--scenario".into(),
            "s.json".into(),
            "--port".into(),
            "0".into()
        ])
        .is_err());
    }

    #[test]
    fn api_key_presence_is_detected_without_reading_its_value() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer sk-x".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ];
        assert!(has_api_key(&headers));
        assert!(!has_api_key(&[(
            "Authorization".to_string(),
            "  ".to_string()
        )]));
    }
}
