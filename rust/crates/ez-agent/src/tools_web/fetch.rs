//! Descarga manual de páginas: redirecciones, límites y extracción de texto.
//!
//! Las costuras [`WebTransport`], [`HostResolver`] y [`FetchClock`] son el
//! equivalente Rust del `transport=`, el DNS falso y el reloj congelado que el
//! oráculo de Python inyecta en las pruebas; en producción se usan las
//! implementaciones del sistema.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{format_g, py_splitlines, py_strip};
use reqwest::blocking::{Client, Response};
use reqwest::header::ACCEPT;
use reqwest::{redirect::Policy, StatusCode, Url};
use scraper::{Html, Selector};

use super::url::{blocks_private_ips, request_target, validate_public_url_with};
use super::{
    WebToolError, ACCEPT as ACCEPT_VALUE, FETCH_TIMEOUT_S, MAX_FETCH_BYTES, MAX_REDIRECTS,
};

/// Petición tal como la ve un transporte inyectado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// Respuesta de un transporte inyectado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl WebResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Fallo del transporte; las variantes se traducen al texto del Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Timeout(String),
    Io(String),
    Other(String),
}

/// Transporte HTTP inyectable (equivale a `httpx.BaseTransport`).
pub trait WebTransport: Send + Sync {
    fn request(&self, request: &WebRequest) -> Result<WebResponse, TransportError>;
}

/// Resolución de nombres inyectable; `None` es el `gaierror` del Python.
pub trait HostResolver: Send + Sync {
    fn resolve(&self, host: &str, port: u16) -> Option<Vec<IpAddr>>;
}

/// Reloj inyectable para el presupuesto total de la descarga.
pub trait FetchClock: Send + Sync {
    fn now(&self) -> Instant;
}

/// Resolución del sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemResolver;

impl HostResolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> Option<Vec<IpAddr>> {
        use std::net::ToSocketAddrs;
        let infos = (host, port).to_socket_addrs().ok()?;
        Some(infos.map(|info| info.ip()).collect())
    }
}

/// Reloj del sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl FetchClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Costuras de la descarga; vacías en producción.
#[derive(Clone, Default)]
pub struct FetchHooks {
    pub transport: Option<Arc<dyn WebTransport>>,
    pub resolver: Option<Arc<dyn HostResolver>>,
    pub clock: Option<Arc<dyn FetchClock>>,
    /// Reproduce la conexión fijada a la IP validada (F36) aun con transporte
    /// inyectado; el `transport=` del Python no fija y el HTTP de producción sí.
    pub pin_requests: bool,
}

impl FetchHooks {
    fn resolver(&self) -> &dyn HostResolver {
        self.resolver.as_deref().unwrap_or(&SystemResolver)
    }

    fn now(&self) -> Instant {
        self.clock
            .as_deref()
            .map_or_else(Instant::now, FetchClock::now)
    }
}

/// Descarga siguiendo redirecciones a mano y devuelve `(url_final, texto)`.
pub(super) fn download_page_text_with(
    config: &AgentConfig,
    url: &str,
    hooks: &FetchHooks,
) -> Result<(String, String), WebToolError> {
    // DECISION: los 8 s son un presupuesto total (redirecciones incluidas).
    let deadline = hooks.now() + Duration::from_secs_f64(FETCH_TIMEOUT_S);
    let mut current_url = url.to_string();
    let mut redirects = 0_usize;
    loop {
        let (request_url, pin) = hop_target(&current_url, config, redirects, hooks.resolver())?;
        let remaining = deadline.saturating_duration_since(hooks.now());
        if remaining.is_zero() {
            return Err(timeout_error());
        }
        let (connect_url, host_header) = if hooks.pin_requests && pin.is_some() {
            let (connect, headers, _) =
                super::url::pinned_request_target_with(&request_url, config, hooks.resolver())?;
            let host = headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("host"))
                .map(|(_, value)| value.clone());
            (connect, host)
        } else {
            (request_url.clone(), None)
        };
        let response = exchange(
            &request_url,
            &connect_url,
            host_header.as_deref(),
            pin,
            remaining,
            hooks,
        )?;
        if is_redirect(&response) {
            let location = response.header("location").unwrap_or_default();
            current_url = next_redirect(&request_url, location, redirects)?;
            redirects += 1;
            continue;
        }
        if response.status >= 400 {
            return Err(WebToolError::new(format!(
                "La página respondió con HTTP {}.",
                response.status
            )));
        }
        let content_type = response.header("content-type").map(str::to_string);
        let payload = check_body(response.body, deadline, hooks)?;
        let page_text = extract_text(&decode_body(&payload, content_type.as_deref()));
        return Ok((request_url, page_text));
    }
}

pub(super) fn hop_target(
    current_url: &str,
    config: &AgentConfig,
    redirects: usize,
    resolver: &dyn HostResolver,
) -> Result<(String, Option<(String, SocketAddr)>), WebToolError> {
    let result = if blocks_private_ips(config) {
        request_target(current_url, config, resolver).map(|target| (target.url, target.pin))
    } else {
        validate_public_url_with(current_url, config, resolver).map(|url| (url, None))
    };
    match result {
        Err(error) if redirects > 0 => Err(WebToolError::blocked_url(format!(
            "Redirección bloqueada: {}",
            error.message
        ))),
        other => other,
    }
}

fn timeout_error() -> WebToolError {
    WebToolError::page_timeout(format!(
        "La página tardó más de {} segundos en responder.",
        format_g(FETCH_TIMEOUT_S)
    ))
}

fn check_body(
    payload: Vec<u8>,
    deadline: Instant,
    hooks: &FetchHooks,
) -> Result<Vec<u8>, WebToolError> {
    if payload.len() > MAX_FETCH_BYTES {
        return Err(WebToolError::page_too_large(
            "La página supera el tamaño máximo de 2 MB.",
        ));
    }
    if hooks.now() > deadline {
        return Err(timeout_error());
    }
    Ok(payload)
}

/// URL y cabeceras que recibe un transporte inyectado.
fn injected_request(url: &str, host_header: Option<&str>) -> WebRequest {
    let parsed = Url::parse(url).ok();
    let normalized = parsed
        .as_ref()
        .map_or_else(|| url.to_string(), Url::to_string);
    let mut headers = vec![
        ("user-agent".to_string(), super::USER_AGENT.to_string()),
        ("accept".to_string(), ACCEPT_VALUE.to_string()),
    ];
    if let Some(host) = host_header {
        headers.insert(0, ("host".to_string(), host.to_string()));
    } else if let Some(parsed) = &parsed {
        if let Some(host) = parsed.host_str() {
            let host = match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
            headers.insert(0, ("host".to_string(), host));
        }
    }
    WebRequest {
        method: "GET".to_string(),
        url: normalized,
        headers,
    }
}

#[allow(clippy::too_many_arguments)]
fn exchange(
    _url: &str,
    connect_url: &str,
    host_header: Option<&str>,
    pin: Option<(String, SocketAddr)>,
    remaining: Duration,
    hooks: &FetchHooks,
) -> Result<WebResponse, WebToolError> {
    if let Some(transport) = hooks.transport.as_deref() {
        let request = injected_request(connect_url, host_header);
        return transport.request(&request).map_err(|error| match error {
            TransportError::Timeout(_) => timeout_error(),
            TransportError::Io(message) => {
                WebToolError::new(format!("No se pudo descargar la página: {message}"))
            }
            TransportError::Other(message) => {
                WebToolError::new(format!("No se pudo leer la página: {message}"))
            }
        });
    }
    real_exchange(connect_url, pin, remaining)
}

fn real_exchange(
    url: &str,
    pin: Option<(String, SocketAddr)>,
    remaining: Duration,
) -> Result<WebResponse, WebToolError> {
    let mut builder = Client::builder()
        .redirect(Policy::none())
        // DECISION: ignorar proxies del entorno evita que un HTTP_PROXY
        // inesperado se salte la validación de destino.
        .no_proxy()
        .timeout(remaining)
        .user_agent(super::USER_AGENT);
    if let Some((host, address)) = pin {
        // F36: la conexión va a la IP recién validada; Host y SNI salen de la
        // URL original.
        builder = builder.resolve(&host, address);
        // Un host con punto final (`example.com.`) se valida y fija sin el
        // punto; se registran ambas formas para que la IP fijada se aplique.
        if !host.ends_with('.') {
            builder = builder.resolve(&format!("{host}."), address);
        }
    }
    let client = builder
        .build()
        .map_err(|error| WebToolError::new(format!("No se pudo leer la página: {error}")))?;
    let response = client
        .get(url)
        .header(ACCEPT, ACCEPT_VALUE)
        .send()
        .map_err(map_request_error)?;
    read_response(response)
}

fn read_response(mut response: Response) -> Result<WebResponse, WebToolError> {
    let status = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let mut body = Vec::new();
    response
        .read_to_end(&mut body)
        .map_err(|error| WebToolError::new(format!("No se pudo descargar la página: {error}")))?;
    Ok(WebResponse {
        status,
        headers,
        body,
    })
}

fn map_request_error(error: reqwest::Error) -> WebToolError {
    if error.is_timeout() {
        return timeout_error();
    }
    if error.is_builder() {
        return WebToolError::new(format!("La URL no es válida: {error}"));
    }
    if error.is_request() {
        return WebToolError::new(format!("No se pudo descargar la página: {error}"));
    }
    WebToolError::new(format!("No se pudo leer la página: {error}"))
}

fn is_redirect(response: &WebResponse) -> bool {
    matches!(
        StatusCode::from_u16(response.status).ok(),
        Some(
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        )
    )
}

pub(super) fn next_redirect(
    base: &str,
    location: &str,
    redirects: usize,
) -> Result<String, WebToolError> {
    if redirects >= MAX_REDIRECTS {
        return Err(WebToolError::new(format!(
            "Se superó el máximo de {MAX_REDIRECTS} redirecciones."
        )));
    }
    if location.is_empty() {
        return Err(WebToolError::new(
            "La página devolvió una redirección sin destino.",
        ));
    }
    redirect_target(base, location)
}

pub(super) fn redirect_target(base: &str, location: &str) -> Result<String, WebToolError> {
    let joined = Url::parse(base)
        .and_then(|url| url.join(location))
        .map_err(|_| {
            WebToolError::blocked_url("Redirección bloqueada: la URL de destino no es válida.")
        })?;
    if joined.scheme() != "http" && joined.scheme() != "https" {
        return Err(WebToolError::blocked_url(
            "Redirección bloqueada: solo se permiten URL con esquema http:// o https://.",
        ));
    }
    Ok(joined.to_string())
}

fn decode_body(bytes: &[u8], content_type: Option<&str>) -> String {
    let charset = content_type
        .and_then(charset_param)
        .unwrap_or_else(|| "utf-8".to_string());
    match charset.as_str() {
        // DECISION: sin `encoding_rs` solo se distinguen UTF-8 y Latin-1;
        // windows-1252 se aproxima a Latin-1 y el resto cae a UTF-8 lossy.
        "iso-8859-1" | "latin-1" | "latin1" | "windows-1252" | "cp1252" => bytes
            .iter()
            .map(|byte| char::from(*byte))
            .collect::<String>(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn charset_param(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"').to_lowercase())
    })
}

/// Extrae el texto visible sin scripts, estilos ni `noscript`.
pub fn extract_text(page_html: &str) -> String {
    let mut document = Html::parse_document(page_html);
    let selector = Selector::parse("script, style, noscript").expect("selector válido");
    let ids: Vec<_> = document
        .select(&selector)
        .map(|element| element.id())
        .collect();
    for id in ids {
        if let Some(mut node) = document.tree.get_mut(id) {
            node.detach();
        }
    }
    let text = document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("\n");
    py_splitlines(&text)
        .iter()
        .map(|line| py_strip(line))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
