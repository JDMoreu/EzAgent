//! Respuestas de error del servidor y cierre ordenado.

mod support;

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use ez_ctl::{CtlServer, Handler, Reply, Request, Tokens, MAX_BODY_BYTES};
use support::{FakeHandler, Harness, TOKEN};

fn request(url: &str, method: &str, token: Option<&str>, body: &[u8]) -> (u16, String) {
    let client = reqwest::blocking::Client::new();
    let builder = if method == "GET" {
        client.get(url.to_string())
    } else {
        client.post(url.to_string())
    };
    let builder = match token {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    };
    let response = builder.body(body.to_vec()).send().expect("conexión");
    (response.status().as_u16(), response.text().expect("cuerpo"))
}

#[test]
fn missing_or_unknown_token_is_401() {
    let mut harness = Harness::new();
    let endpoint = format!("{}/v1/command", harness.url);
    let body = br#"{"command":"status","args":{}}"#;

    let (status, text) = request(&endpoint, "POST", None, body);
    assert_eq!(status, 401);
    assert_eq!(
        text,
        r#"{"ok":false,"message":"Token inválido o ausente.","data":null}"#
    );

    let (status, text) = request(&endpoint, "POST", Some("token-que-no-existe"), body);
    assert_eq!(status, 401);
    assert!(text.contains("Token inválido o ausente."), "{text}");
    assert!(harness.handler.calls().is_empty());
    harness.stop();
}

#[test]
fn bad_json_or_missing_command_is_400() {
    let mut harness = Harness::new();
    let endpoint = format!("{}/v1/command", harness.url);

    let (status, _) = request(&endpoint, "POST", Some(TOKEN), b"no es json");
    assert_eq!(status, 400);

    let (status, _) = request(&endpoint, "POST", Some(TOKEN), br#"{"args":{}}"#);
    assert_eq!(status, 400);

    let (status, _) = request(
        &endpoint,
        "POST",
        Some(TOKEN),
        br#"{"command":"   ","args":{}}"#,
    );
    assert_eq!(status, 400);
    assert!(harness.handler.calls().is_empty());
    harness.stop();
}

#[test]
fn another_route_or_method_is_404() {
    let mut harness = Harness::new();
    let (status, _) = request(
        &format!("{}/v1/command", harness.url),
        "GET",
        Some(TOKEN),
        b"",
    );
    assert_eq!(status, 404);

    let (status, _) = request(
        &format!("{}/v1/otra", harness.url),
        "POST",
        Some(TOKEN),
        br#"{"command":"status"}"#,
    );
    assert_eq!(status, 404);
    assert!(harness.handler.calls().is_empty());
    harness.stop();
}

#[test]
fn a_body_over_one_mib_is_413() {
    let mut harness = Harness::new();
    let endpoint = format!("{}/v1/command", harness.url);
    let body = vec![b'a'; MAX_BODY_BYTES + 1];
    let (status, text) = request(&endpoint, "POST", Some(TOKEN), &body);
    assert_eq!(status, 413);
    assert!(text.contains("demasiado grande"), "{text}");
    assert!(harness.handler.calls().is_empty());
    harness.stop();
}

#[test]
fn start_on_binds_the_requested_free_port() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let port = listener.local_addr().expect("dirección").port();
    drop(listener);
    let handler: Arc<dyn Handler> = FakeHandler::new();
    let server = CtlServer::start_on(port, handler, Arc::new(Tokens::new())).expect("servidor");
    assert_eq!(server.url(), format!("http://127.0.0.1:{port}"));
    server.stop();
}

#[test]
fn start_on_fails_when_the_port_is_busy() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto ocupado");
    let port = listener.local_addr().expect("dirección").port();
    let handler: Arc<dyn Handler> = Arc::new(BusyHandler);
    assert!(CtlServer::start_on(port, handler, Arc::new(Tokens::new())).is_err());
}

struct BusyHandler;

impl Handler for BusyHandler {
    fn handle(&self, _caller: &ez_ctl::Caller, _request: &Request) -> Reply {
        Reply::ok("hecho", serde_json::Value::Null)
    }
}

#[test]
fn stop_closes_the_port_and_url_is_loopback() {
    let mut harness = Harness::new();
    assert!(
        harness.url.starts_with("http://127.0.0.1:"),
        "{}",
        harness.url
    );
    let address = harness.url.trim_start_matches("http://").to_string();
    harness.stop();
    assert!(TcpStream::connect(address).is_err());
}
