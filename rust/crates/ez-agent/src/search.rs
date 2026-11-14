//! Port of `src/ezagent/agent/search.py` (F3-10).
//!
//! Proveedores de búsqueda web intercambiables. El proveedor por defecto
//! implementa el backend HTML de DuckDuckGo con `reqwest`; la URL base es
//! configurable para poder apuntar a un servidor de pruebas.

use std::time::Duration;

use ez_core::pyvalue::py_strip;
use reqwest::Url;
use scraper::{Html, Selector};

/// Resultado normalizado de una búsqueda.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Error de búsqueda con un mensaje en español apto para el usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchError {
    pub message: String,
    /// Clase estable para la auditoría: unavailable/timeout/rate_limit/provider.
    pub kind: String,
}

impl SearchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: "provider".into(),
        }
    }

    pub fn with_kind(message: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: kind.into(),
        }
    }

    /// Port de `SearchUnavailableError`: el proveedor no está disponible.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::with_kind(message, "unavailable")
    }
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SearchError {}

/// Contrato mínimo de un proveedor de búsqueda.
pub trait SearchProvider: Send + Sync {
    fn search(&self, query: &str, max_results: i64) -> Result<Vec<SearchResult>, SearchError>;
}

/// Clase estable del fallo para auditoría y para `ToolResult.error_kind`.
pub fn classify_error(type_name: &str, text: &str) -> String {
    let name = type_name.to_lowercase();
    let text = text.to_lowercase();
    if name.contains("timeout") || text.contains("timeout") || text.contains("timed out") {
        return "timeout".into();
    }
    if name.contains("ratelimit")
        || name.contains("toomanyrequests")
        || text.contains("rate limit")
        || text.contains("too many requests")
        || text.contains("429")
    {
        return "rate_limit".into();
    }
    "provider".into()
}

pub const DEFAULT_REGION: &str = "wt-wt";
pub const DEFAULT_TIMEOUT_S: f64 = 8.0;
pub const MAX_RESULTS: i64 = 10;

// DECISION: `ddgs` usa por defecto `https://html.duckduckgo.com/html/` con
// POST (q, kl); se reproduce solo ese backend con reqwest.
pub const DEFAULT_BASE_URL: &str = "https://html.duckduckgo.com";
const USER_AGENT: &str = "EzAgent/0.1";

/// Proveedor de búsqueda basado en el backend HTML de DuckDuckGo.
#[derive(Debug, Clone, PartialEq)]
pub struct DdgsSearchProvider {
    pub region: String,
    pub backend: Option<String>,
    pub timeout_s: f64,
    pub base_url: String,
}

impl Default for DdgsSearchProvider {
    fn default() -> Self {
        Self {
            region: DEFAULT_REGION.into(),
            backend: None,
            timeout_s: DEFAULT_TIMEOUT_S,
            base_url: DEFAULT_BASE_URL.into(),
        }
    }
}

impl DdgsSearchProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = region.into();
        self
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    pub fn with_timeout(mut self, timeout_s: f64) -> Self {
        self.timeout_s = timeout_s;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn endpoint(&self) -> String {
        // DECISION: solo se implementa el backend HTML; `backend` se acepta
        // por compatibilidad con la API de `ddgs`, pero no cambia la ruta.
        format!("{}/html/", self.base_url.trim_end_matches('/'))
    }

    fn client(&self) -> Result<reqwest::blocking::Client, SearchError> {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs_f64(self.timeout_s.max(0.0)))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| {
                SearchError::with_kind(
                    format!("La búsqueda web falló: {error}"),
                    classify_error("requesterror", &error.to_string()),
                )
            })
    }

    fn request_error(&self, error: reqwest::Error) -> SearchError {
        let text = error.to_string();
        let name = if error.is_timeout() {
            "timeouterror"
        } else {
            "requesterror"
        };
        SearchError::with_kind(
            format!("La búsqueda web falló: {text}"),
            classify_error(name, &text),
        )
    }
}

impl SearchProvider for DdgsSearchProvider {
    fn search(&self, query: &str, max_results: i64) -> Result<Vec<SearchResult>, SearchError> {
        let normalized_query = py_strip(query);
        if normalized_query.is_empty() {
            return Err(SearchError::new(
                "La consulta de búsqueda no puede estar vacía.",
            ));
        }
        if !(1..=MAX_RESULTS).contains(&max_results) {
            return Err(SearchError::new(
                "El número de resultados debe estar entre 1 y 10.",
            ));
        }
        let client = self.client()?;
        let body = format!(
            "q={}&kl={}",
            form_encode(normalized_query),
            form_encode(&self.region)
        );
        let response = client
            .post(self.endpoint())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .map_err(|error| self.request_error(error))?;
        let status = response.status().as_u16();
        if status >= 400 {
            let text = format!("HTTP {status}");
            return Err(SearchError::with_kind(
                format!("La búsqueda web falló: {text}"),
                classify_error("requesterror", &text),
            ));
        }
        let bytes = response
            .bytes()
            .map_err(|error| self.request_error(error))?;
        let body = String::from_utf8_lossy(&bytes);
        Ok(parse_results(&body, &self.base_url, max_results))
    }
}

/// Extrae resultados del HTML de DuckDuckGo y normaliza los campos.
pub(crate) fn parse_results(body: &str, base_url: &str, max_results: i64) -> Vec<SearchResult> {
    let document = Html::parse_document(body);
    let Ok(item_selector) = Selector::parse("div.result") else {
        return Vec::new();
    };
    let Ok(link_selector) = Selector::parse("a.result__a") else {
        return Vec::new();
    };
    let Ok(snippet_selector) = Selector::parse(".result__snippet") else {
        return Vec::new();
    };
    let base = Url::parse(base_url).ok();
    let mut results = Vec::new();
    for item in document.select(&item_selector) {
        let link = item.select(&link_selector).next();
        let title = link
            .map(|element| py_strip(&element.text().collect::<String>()).to_string())
            .unwrap_or_default();
        let url = link
            .and_then(|element| element.value().attr("href"))
            .map(|href| absolute_url(base.as_ref(), href))
            .unwrap_or_default();
        let snippet = item
            .select(&snippet_selector)
            .next()
            .map(|element| py_strip(&element.text().collect::<String>()).to_string())
            .unwrap_or_default();
        if title.is_empty() && url.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            snippet,
        });
        if results.len() as i64 >= max_results {
            break;
        }
    }
    results
}

/// Codificación `application/x-www-form-urlencoded` (espacio como `+`).
fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(*byte));
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn absolute_url(base: Option<&Url>, href: &str) -> String {
    let Some(base) = base else {
        return href.to_string();
    };
    let Ok(joined) = base.join(href) else {
        return href.to_string();
    };
    if let Some((_, value)) = joined.query_pairs().find(|(key, _)| key.as_ref() == "uddg") {
        return value.into_owned();
    }
    joined.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    const PAGE: &str = r#"<html><body>
      <div class="result"><a class="result__a" href="https://uno.test/a">Uno</a>
        <a class="result__snippet">primero</a></div>
      <div class="result"><a class="result__a" href="/l/?uddg=https%3A%2F%2Fdos.test%2Fb">Dos</a>
        <span class="result__snippet">segundo</span></div>
    </body></html>"#;

    fn serve_html(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
        let address = listener.local_addr().expect("dirección");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
        });
        format!("http://{address}")
    }

    #[test]
    fn classifies_errors_like_python() {
        assert_eq!(classify_error("ReadTimeout", ""), "timeout");
        assert_eq!(classify_error("Error", "Timed out"), "timeout");
        assert_eq!(classify_error("RateLimitError", ""), "rate_limit");
        assert_eq!(classify_error("", "too many requests"), "rate_limit");
        assert_eq!(classify_error("", "HTTP 429"), "rate_limit");
        assert_eq!(classify_error("ValueError", "boom"), "provider");
    }

    #[test]
    fn encodes_form_bodies() {
        assert_eq!(form_encode("clima frío"), "clima+fr%C3%ADo");
        assert_eq!(form_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn validates_query_and_limit() {
        let provider = DdgsSearchProvider::default();
        assert_eq!(
            provider.search("   ", 5).unwrap_err().message,
            "La consulta de búsqueda no puede estar vacía."
        );
        assert_eq!(
            provider.search("x", 0).unwrap_err().message,
            "El número de resultados debe estar entre 1 y 10."
        );
        assert_eq!(
            provider.search("x", 11).unwrap_err().message,
            "El número de resultados debe estar entre 1 y 10."
        );
    }

    #[test]
    fn parses_results_and_decodes_redirect_links() {
        let results = parse_results(PAGE, "https://html.duckduckgo.com", 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Uno");
        assert_eq!(results[0].url, "https://uno.test/a");
        assert_eq!(results[0].snippet, "primero");
        assert_eq!(results[1].url, "https://dos.test/b");
        assert_eq!(results[1].snippet, "segundo");
        assert_eq!(
            parse_results(PAGE, "https://html.duckduckgo.com", 1).len(),
            1
        );
    }

    #[test]
    fn searches_against_a_local_server() {
        let base = serve_html(PAGE);
        let provider = DdgsSearchProvider::default().with_base_url(base);
        let results = provider.search(" clima ", 5).expect("búsqueda");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Uno");
    }
}
