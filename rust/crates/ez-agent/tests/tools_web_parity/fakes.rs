//! Dobles del dominio `tools_web`: transporte, DNS y reloj inyectados.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ez_agent::tools_web::{
    FetchClock, HostResolver, TransportError, WebRequest, WebResponse, WebTransport,
    MAX_FETCH_BYTES,
};
use serde_json::Value;

use super::PUBLIC;

pub(super) enum Dns {
    Addresses(Vec<IpAddr>),
    Fail,
}

pub(super) struct FakeResolver(pub(super) BTreeMap<String, Dns>);

impl HostResolver for FakeResolver {
    fn resolve(&self, host: &str, _port: u16) -> Option<Vec<IpAddr>> {
        match self.0.get(host) {
            Some(Dns::Addresses(addresses)) => Some(addresses.clone()),
            _ => None,
        }
    }
}

pub(super) struct FakeClock {
    base: Instant,
    values: Mutex<Vec<f64>>,
}

impl FakeClock {
    pub(super) fn new(values: &[f64]) -> Self {
        Self {
            base: Instant::now(),
            values: Mutex::new(values.to_vec()),
        }
    }
}

impl FetchClock for FakeClock {
    fn now(&self) -> Instant {
        let mut values = self.values.lock().expect("reloj");
        let value = if values.len() > 1 {
            values.remove(0)
        } else {
            values.first().copied().unwrap_or(0.0)
        };
        self.base + Duration::from_secs_f64(value)
    }
}

pub(super) type Handler =
    Box<dyn Fn(&WebRequest) -> Result<WebResponse, TransportError> + Send + Sync>;

pub(super) struct FakeTransport {
    pub(super) calls: Mutex<Vec<WebRequest>>,
    pub(super) handler: Handler,
}

impl WebTransport for FakeTransport {
    fn request(&self, request: &WebRequest) -> Result<WebResponse, TransportError> {
        self.calls.lock().expect("llamadas").push(request.clone());
        (self.handler)(request)
    }
}

pub(super) fn page(status: u16, body: &[u8], content_type: &str) -> WebResponse {
    WebResponse {
        status,
        headers: vec![("content-type".to_string(), content_type.to_string())],
        body: body.to_vec(),
    }
}

pub(super) fn redirect(location: &str) -> WebResponse {
    WebResponse {
        status: 302,
        headers: vec![("location".to_string(), location.to_string())],
        body: Vec::new(),
    }
}

pub(super) fn handler(label: &str) -> Handler {
    let html = "text/html; charset=utf-8";
    match label {
        "html" => Box::new(move |_| {
            Ok(page(
                200,
                b"<html><body><h1>Hola</h1><p>Mundo</p></body></html>",
                html,
            ))
        }),
        "empty" | "status-204" => Box::new(move |_| Ok(page(200, b"", html))),
        "scripts" => Box::new(move |_| {
            Ok(page(
                200,
                b"<div>Antes<script>alert('x')</script><style>p{}</style><noscript>no</noscript>Despu\xc3\xa9s</div>",
                html,
            ))
        }),
        "entities" => Box::new(move |_| Ok(page(200, b"<p>a &amp; b &lt; c &nbsp; d</p>", html))),
        "plain" => Box::new(move |_| Ok(page(200, "línea 1\nlínea 2\n".as_bytes(), "text/plain"))),
        "latin1" => Box::new(move |_| {
            Ok(page(
                200,
                b"<p>ca\xf1\xf3n</p>",
                "text/html; charset=iso-8859-1",
            ))
        }),
        "bad-charset" => {
            Box::new(move |_| Ok(page(200, b"<p>x</p>", "text/html; charset=no-existe")))
        }
        "invalid-utf8" => Box::new(move |_| Ok(page(200, b"<p>a\xffb</p>", html))),
        "redirect-relative" => Box::new(move |request| {
            if request.url.ends_with("/inicio") {
                Ok(redirect("/final"))
            } else {
                Ok(page(200, b"<p>final</p>", html))
            }
        }),
        "redirect-absolute" => Box::new(move |request| {
            if request.url.contains("example.test") {
                Ok(redirect("https://other.test/x"))
            } else {
                Ok(page(200, b"<p>otro</p>", html))
            }
        }),
        "redirect-too-many" => Box::new(move |_| Ok(redirect("/otra"))),
        "redirect-no-location" => Box::new(move |_| {
            Ok(WebResponse {
                status: 302,
                headers: Vec::new(),
                body: Vec::new(),
            })
        }),
        "redirect-javascript" => Box::new(move |_| Ok(redirect("javascript:alert(1)"))),
        "redirect-ftp" => Box::new(move |_| Ok(redirect("ftp://example.test/x"))),
        "redirect-private" => Box::new(move |_| Ok(redirect("http://127.0.0.1/x"))),
        "redirect-localhost" => Box::new(move |_| Ok(redirect("http://localhost/x"))),
        "status-404" => Box::new(move |_| Ok(page(404, b"no", html))),
        "status-500" => Box::new(move |_| Ok(page(500, b"error", html))),
        "too-large" => Box::new(move |_| Ok(page(200, &vec![b'z'; MAX_FETCH_BYTES + 1], html))),
        "timeout" => Box::new(move |_| Err(TransportError::Timeout("timed out".into()))),
        "oserror" => Box::new(move |_| Err(TransportError::Io("sin ruta".into()))),
        "unexpected" => Box::new(move |_| Err(TransportError::Other("roto".into()))),
        "budget" => Box::new(move |_| panic!("el caso budget no debe pedir la página")),
        "pinned-host-port" => Box::new(move |_| Ok(page(200, b"<p>puerto</p>", html))),
        "pinned-https" => Box::new(move |_| Ok(page(200, b"<p>seguro</p>", html))),
        "pinned-idn" => Box::new(move |_| Ok(page(200, b"<p>idn</p>", html))),
        "pinned-trailing-dot" => Box::new(move |_| Ok(page(200, b"<p>punto</p>", html))),
        "literal-public" => Box::new(move |_| Ok(page(200, b"<p>literal</p>", html))),
        "allow-private" | "allow-localhost-hostname" => {
            Box::new(move |_| Ok(page(200, b"<p>local</p>", html)))
        }
        _ => Box::new(move |_| Ok(page(200, b"<p>zzzz", html))),
    }
}

pub(super) fn test_page(label: &str) -> Handler {
    match label {
        "truncate" | "max-default" | "max-bool" | "max-zero" | "max-too-big" | "max-text"
        | "max-none" => Box::new(move |_| {
            Ok(page(
                200,
                b"<p>zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz</p>",
                "text/html; charset=utf-8",
            ))
        }),
        _ => handler(label),
    }
}

pub(super) fn address(text: &str) -> IpAddr {
    text.parse().expect("dirección del fixture")
}

pub(super) fn resolver_from(input: &Value) -> FakeResolver {
    let mut map = BTreeMap::new();
    if let Some(Value::Object(entries)) = input.get("dns") {
        for (host, value) in entries {
            if let Value::Array(items) = value {
                map.insert(
                    host.clone(),
                    Dns::Addresses(
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(address)
                            .collect(),
                    ),
                );
            } else {
                map.insert(host.clone(), Dns::Fail);
            }
        }
    }
    FakeResolver(map)
}

pub(super) fn target_resolver() -> FakeResolver {
    let mut map = BTreeMap::new();
    map.insert(
        "example.test".to_string(),
        Dns::Addresses(vec![
            address(PUBLIC),
            address("2606:2800:220:1:248:1893:25c8:1946"),
        ]),
    );
    map.insert(
        "v6.test".to_string(),
        Dns::Addresses(vec![address("2606:2800:220:1:248:1893:25c8:1946")]),
    );
    map.insert(
        "doble.test".to_string(),
        Dns::Addresses(vec![address("10.0.0.5")]),
    );
    map.insert(
        "local.test".to_string(),
        Dns::Addresses(vec![address("127.0.0.1")]),
    );
    map.insert(
        "ñandú.test".to_string(),
        Dns::Addresses(vec![address(PUBLIC)]),
    );
    map.insert("vacio.test".to_string(), Dns::Addresses(Vec::new()));
    map.insert("roto.test".to_string(), Dns::Fail);
    FakeResolver(map)
}
