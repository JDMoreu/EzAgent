//! Descarga manual de páginas: redirecciones, límites y extracción de texto.

use std::io::Read;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{format_g, py_splitlines, py_strip};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use reqwest::{redirect::Policy, StatusCode, Url};
use scraper::{Html, Selector};

use super::url::{blocks_private_ips, request_target, validate_public_url};
use super::{
    WebToolError, ACCEPT as ACCEPT_VALUE, FETCH_TIMEOUT_S, MAX_FETCH_BYTES, MAX_REDIRECTS,
};

/// Descarga siguiendo redirecciones a mano y devuelve `(url_final, texto)`.
pub(super) fn download_page_text(
    config: &AgentConfig,
    url: &str,
) -> Result<(String, String), WebToolError> {
    // DECISION: los 8 s son un presupuesto total (redirecciones incluidas).
    let deadline = Instant::now() + Duration::from_secs_f64(FETCH_TIMEOUT_S);
    let mut current_url = url.to_string();
    let mut redirects = 0_usize;
    loop {
        let (request_url, pin) = hop_target(&current_url, config, redirects)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timeout_error());
        }
        let response = send(&request_url, pin, remaining)?;
        if let Some(location) = redirect_location(&response) {
            current_url = next_redirect(&request_url, &location, redirects)?;
            redirects += 1;
            continue;
        }
        let status = response.status().as_u16();
        if status >= 400 {
            return Err(WebToolError::new(format!(
                "La página respondió con HTTP {status}."
            )));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let payload = read_body(response, deadline)?;
        let page_text = extract_text(&decode_body(&payload, content_type.as_deref()));
        return Ok((request_url, page_text));
    }
}

pub(super) fn hop_target(
    current_url: &str,
    config: &AgentConfig,
    redirects: usize,
) -> Result<(String, Option<(String, SocketAddr)>), WebToolError> {
    let result = if blocks_private_ips(config) {
        request_target(current_url, config).map(|target| (target.url, target.pin))
    } else {
        validate_public_url(current_url, config).map(|url| (url, None))
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

fn send(
    url: &str,
    pin: Option<(String, SocketAddr)>,
    remaining: Duration,
) -> Result<Response, WebToolError> {
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
    client
        .get(url)
        .header(ACCEPT, ACCEPT_VALUE)
        .send()
        .map_err(map_request_error)
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

fn redirect_location(response: &Response) -> Option<String> {
    if !matches!(
        response.status(),
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    ) {
        return None;
    }
    response
        .headers()
        .get(LOCATION)
        .map(|value| value.to_str().unwrap_or_default().to_string())
}

pub(super) fn next_redirect(
    base: &str,
    location: &str,
    redirects: usize,
) -> Result<String, WebToolError> {
    let target = if location.is_empty() {
        None
    } else {
        Some(redirect_target(base, location)?)
    };
    if redirects >= MAX_REDIRECTS {
        return Err(WebToolError::new(format!(
            "Se superó el máximo de {MAX_REDIRECTS} redirecciones."
        )));
    }
    target.ok_or_else(|| WebToolError::new("La página devolvió una redirección sin destino."))
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

fn read_body(mut response: Response, deadline: Instant) -> Result<Vec<u8>, WebToolError> {
    let mut payload = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = match response.read(&mut buffer) {
            Ok(read) => read,
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(timeout_error());
                }
                return Err(WebToolError::new(format!(
                    "No se pudo descargar la página: {error}"
                )));
            }
        };
        if read == 0 {
            return Ok(payload);
        }
        payload.extend_from_slice(&buffer[..read]);
        if payload.len() > MAX_FETCH_BYTES {
            return Err(WebToolError::page_too_large(
                "La página supera el tamaño máximo de 2 MB.",
            ));
        }
        if Instant::now() > deadline {
            return Err(timeout_error());
        }
    }
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
pub(super) fn extract_text(page_html: &str) -> String {
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
