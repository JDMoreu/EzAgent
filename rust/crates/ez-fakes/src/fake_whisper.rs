//! Servidor falso de `whisper-server` (F1-11).
//!
//! Imita `POST /inference` (multipart de `httpx` con el campo `file`) y la
//! variante OpenAI `/v1/audio/transcriptions`. Devuelve `{"text": ...}` con la
//! transcripción del guion, en orden, o la asociada al SHA-256 del WAV. La
//! sonda de salud de `ensure_whisper` es un TCP abierto; además se ofrece
//! `GET /health` para las comprobaciones manuales.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use tracing::{info, warn};

mod multipart;
mod sha256;

/// Puerto por defecto: el mismo que documenta `.env.example` para whisper.
pub const DEFAULT_PORT: u16 = 8088;

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// Un fallo guionizado (F7-05): se consume una petición sin transcripción.
///
/// `drop` cierra la conexión sin responder, como un whisper-server que muere a
/// mitad de petición; si no, responde `status` con `{"error": error}`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fault {
    pub status: u16,
    pub error: String,
    pub delay_ms: u64,
    pub drop: bool,
}

/// Guion de transcripciones. `by_hash` gana y no consume el orden; `faults` se
/// consume una por petición antes de mirar las transcripciones.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Scenario {
    #[serde(default)]
    pub transcripts: Vec<String>,
    #[serde(default)]
    pub by_hash: BTreeMap<String, String>,
    #[serde(default)]
    pub repeat_last: bool,
    #[serde(default)]
    pub faults: Vec<Fault>,
}

impl Scenario {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("no pude leer el escenario {}", path.display()))?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text).context("el escenario de fake-whisper no es JSON válido")
    }
}

/// Consumo ordenado del guion, con búsqueda por hash.
#[derive(Debug, Clone)]
pub struct Transcripts {
    scenario: Scenario,
    index: usize,
    fault: usize,
}

impl Transcripts {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            index: 0,
            fault: 0,
        }
    }

    /// Siguiente fallo del guion (se consume una vez).
    pub fn next_fault(&mut self) -> Option<Fault> {
        let fault = self.scenario.faults.get(self.fault).cloned();
        if fault.is_some() {
            self.fault += 1;
        }
        fault
    }

    /// Siguiente transcripción: primero el hash, después el orden del guion.
    pub fn next(&mut self, hash: Option<&str>) -> String {
        if let Some(text) = hash.and_then(|hash| self.scenario.by_hash.get(hash)) {
            return text.clone();
        }
        if let Some(text) = self.scenario.transcripts.get(self.index).cloned() {
            self.index += 1;
            return text;
        }
        if self.scenario.repeat_last {
            return self
                .scenario
                .transcripts
                .last()
                .cloned()
                .unwrap_or_default();
        }
        String::new()
    }

    /// Posición actual del guion (para logs y pruebas).
    pub fn position(&self) -> usize {
        self.index
    }
}

/// Opciones de línea de comandos.
#[derive(Debug, Clone)]
pub struct Options {
    pub scenario: Option<PathBuf>,
    pub port: u16,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scenario: None,
            port: default_port(),
        }
    }
}

impl Options {
    /// `--scenario <ruta>` y `--port <u16>`; cualquier otra cosa es error.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut options = Options::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--scenario" => {
                    let value = args
                        .get(index + 1)
                        .context("--scenario necesita una ruta")?;
                    options.scenario = Some(PathBuf::from(value));
                    index += 2;
                }
                "--port" => {
                    let value = args.get(index + 1).context("--port necesita un número")?;
                    options.port = value
                        .parse::<u16>()
                        .with_context(|| format!("puerto no válido: {value}"))?;
                    index += 2;
                }
                other => anyhow::bail!("argumento desconocido: {other}"),
            }
        }
        Ok(options)
    }
}

/// Abre el servidor en `127.0.0.1:<port>` (nunca en otra interfaz).
pub fn bind(port: u16) -> Result<Server> {
    Server::http(("127.0.0.1", port))
        .map_err(|error| anyhow::anyhow!("no pude abrir el puerto {port}: {error}"))
}

/// Atiende peticiones hasta que el proceso termine.
pub fn serve(server: &Server, transcripts: &mut Transcripts) -> Result<()> {
    for request in server.incoming_requests() {
        handle(request, transcripts);
    }
    Ok(())
}

/// Carga el escenario (si hay) y sirve hasta que se cierre el proceso.
pub fn run(options: &Options) -> Result<()> {
    let scenario = match options.scenario.as_deref() {
        Some(path) => Scenario::load(path)?,
        None => Scenario::default(),
    };
    let server = bind(options.port)?;
    info!(
        "fake-whisper escuchando ({} transcripciones)",
        scenario.transcripts.len()
    );
    serve(&server, &mut Transcripts::new(scenario))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("cabecera HTTP válida")
}

fn header_value<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn respond(request: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", content_type));
    if let Err(error) = request.respond(response) {
        warn!("fake-whisper: no pude responder: {error}");
    }
}

fn respond_json(request: Request, status: u16, value: &Value) {
    respond(
        request,
        status,
        "application/json; charset=utf-8",
        value.to_string().into_bytes(),
    );
}

fn whisper_json(text: &str) -> Value {
    json!({ "text": text })
}

fn read_body(request: &mut Request) -> Vec<u8> {
    let mut body = Vec::new();
    if let Err(error) = request.as_reader().read_to_end(&mut body) {
        warn!("fake-whisper: no pude leer el cuerpo: {error}");
    }
    body
}

fn transcribe(mut request: Request, transcripts: &mut Transcripts) {
    let content_type = header_value(&request, "Content-Type")
        .unwrap_or("")
        .to_string();
    let Some(boundary) = multipart::boundary(&content_type) else {
        respond_json(
            request,
            400,
            &json!({"error": "el formulario no es multipart/form-data"}),
        );
        return;
    };
    let body = read_body(&mut request);
    let parts = match multipart::parse(&body, &boundary) {
        Ok(parts) => parts,
        Err(error) => {
            respond_json(
                request,
                400,
                &json!({"error": format!("formulario multipart inválido: {error}")}),
            );
            return;
        }
    };
    let Some(file) = multipart::file_part(&parts) else {
        respond_json(
            request,
            400,
            &json!({"error": "falta el archivo del campo «file»"}),
        );
        return;
    };
    if let Some(fault) = transcripts.next_fault() {
        apply_fault(request, &fault);
        return;
    }
    let hash = sha256::hex_digest(&file.data);
    let text = transcripts.next(Some(&hash));
    info!(
        "fake-whisper: {} bytes, sha256 {}, transcripción {}",
        file.data.len(),
        &hash[..12],
        transcripts.position()
    );
    respond_json(request, 200, &whisper_json(&text));
}

/// Aplica un fallo guionizado: espera, mata el servidor o devuelve un error.
fn apply_fault(request: Request, fault: &Fault) {
    if fault.delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(fault.delay_ms));
    }
    if fault.drop {
        // Muere el proceso con la petición en vuelo, como un whisper-server
        // que se cae a mitad de turno: el sistema cierra el socket y las
        // peticiones siguientes encuentran el puerto cerrado.
        info!("fake-whisper: muere a mitad de la petición (corte simulado)");
        std::process::exit(0);
    }
    let status = if fault.status >= 400 { fault.status } else { 500 };
    let message = if fault.error.trim().is_empty() {
        "whisper-server falló".to_string()
    } else {
        fault.error.clone()
    };
    respond_json(request, status, &json!({"error": message}));
}

fn route(request: Request, transcripts: &mut Transcripts) {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or("").to_string();
    match (&method, path.as_str()) {
        (Method::Get, "/health" | "/healthz" | "/") => {
            respond_json(request, 200, &json!({"status": "ok"}));
        }
        (Method::Post, path) if path.ends_with("/inference") => transcribe(request, transcripts),
        (Method::Post, path) if path.ends_with("/audio/transcriptions") => {
            transcribe(request, transcripts);
        }
        _ => {
            respond_json(
                request,
                404,
                &json!({"error": "ruta desconocida en fake-whisper"}),
            );
        }
    }
}

fn handle(request: Request, transcripts: &mut Transcripts) {
    let method = request.method().clone();
    let path = request.url().to_string();
    route(request, transcripts);
    info!("fake-whisper: {method} {path}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_order_and_hash() {
        let scenario = Scenario::from_json(
            r#"{
                "transcripts": ["uno", "dos"],
                "by_hash": {"abc": "por hash"}
            }"#,
        )
        .expect("escenario");
        let mut transcripts = Transcripts::new(scenario);
        assert_eq!(transcripts.next(Some("abc")), "por hash");
        assert_eq!(transcripts.position(), 0);
        assert_eq!(transcripts.next(None), "uno");
        assert_eq!(transcripts.next(None), "dos");
        assert_eq!(transcripts.next(None), "");
    }

    #[test]
    fn repeat_last_keeps_the_final_text() {
        let scenario = Scenario::from_json(r#"{"transcripts": ["eco"], "repeat_last": true}"#)
            .expect("escenario");
        let mut transcripts = Transcripts::new(scenario);
        assert_eq!(transcripts.next(None), "eco");
        assert_eq!(transcripts.next(None), "eco");
        assert_eq!(transcripts.next(None), "eco");
    }

    #[test]
    fn options_defaults_and_parsing() {
        let options = Options::parse(&[]).expect("opciones");
        assert_eq!(options.port, DEFAULT_PORT);
        assert!(options.scenario.is_none());
        let options =
            Options::parse(&["--port".to_string(), "9000".to_string()]).expect("opciones");
        assert_eq!(options.port, 9000);
        assert!(Options::parse(&["--port".into(), "x".into()]).is_err());
        assert!(Options::parse(&["--otra".into()]).is_err());
    }

    #[test]
    fn faults_are_consumed_before_the_transcripts() {
        let scenario = Scenario::from_json(
            r#"{
                "faults": [{"drop": true}, {"status": 500, "error": "sin modelo"}, {"delay_ms": 5}],
                "transcripts": ["uno"]
            }"#,
        )
        .expect("escenario");
        let mut transcripts = Transcripts::new(scenario);
        let cut = transcripts.next_fault().expect("corte");
        assert!(cut.drop);
        assert_eq!(transcripts.position(), 0);
        let failed = transcripts.next_fault().expect("error");
        assert_eq!((failed.status, failed.error.as_str()), (500, "sin modelo"));
        assert_eq!(transcripts.next_fault().map(|f| f.delay_ms), Some(5));
        assert!(transcripts.next_fault().is_none());
        assert_eq!(transcripts.next(None), "uno");
    }

    #[test]
    fn response_shape_is_readable_by_python() {
        let value = whisper_json("hola mundo");
        assert_eq!(value["text"], "hola mundo");
        assert_eq!(value.to_string(), r#"{"text":"hola mundo"}"#);
    }
}
