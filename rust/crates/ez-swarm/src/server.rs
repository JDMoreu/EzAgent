//! Port of `src/ezagent/swarm/server.py` (F3-13).
//!
//! Minimal local HTTP server. It only listens on `127.0.0.1` and every request
//! needs the member token (AGENTS.md §5). There is a single point, `POST /v1`,
//! with `{"argv": [...]}`: command parsing lives in [`SwarmService`], so the
//! `ezs` client is dumb and there is only one place to maintain.
//!
//! The port is ephemeral. It is published in `endpoint.json`, which the client
//! reads on every call: if EzAgent restarts on another port, the agents that
//! were already open find it without being relaunched.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::python_str;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use tracing::{debug, error, info, warn};

use crate::service::{Reply, SwarmService};

/// Address the server listens on: loopback only.
pub const HOST: &str = "127.0.0.1";
/// Largest accepted request body.
pub const MAX_REQUEST_BYTES: usize = 16_384;
/// Largest number of arguments accepted.
pub const MAX_ARGS: usize = 64;

const LOCAL_HOSTS: [&str; 2] = ["127.0.0.1", "localhost"];
const NOT_FOUND: &str = "No encontrado.";
const MISSING_TOKEN: &str = "Falta el token del enjambre.";
const BAD_REQUEST: &str = "Petición no válida o demasiado larga.";

struct Running {
    server: Arc<Server>,
    thread: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    url: String,
}

/// Starts and stops the local server and publishes where it listens.
pub struct SwarmServer {
    service: Arc<SwarmService>,
    /// File the client reads to find the server (`endpoint.json`).
    pub endpoint_path: PathBuf,
    running: Mutex<Option<Running>>,
}

impl SwarmServer {
    /// Server for `service`, publishing its address in `endpoint_path`.
    pub fn new(service: Arc<SwarmService>, endpoint_path: impl Into<PathBuf>) -> Self {
        Self {
            service,
            endpoint_path: endpoint_path.into(),
            running: Mutex::new(None),
        }
    }

    /// Address of the running server, or `""`.
    pub fn url(&self) -> String {
        self.running
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map_or_else(String::new, |running| running.url.clone())
    }

    /// Starts listening on a free port of `127.0.0.1`; returns the URL.
    pub fn start(&self) -> io::Result<String> {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(running) = running.as_ref() {
            return Ok(running.url.clone());
        }
        let server =
            Server::http((HOST, 0)).map_err(|error| io::Error::other(error.to_string()))?;
        let address = server.server_addr().to_ip().ok_or_else(|| {
            io::Error::other("el servidor del enjambre no escucha en una dirección IP")
        })?;
        let url = format!("http://{HOST}:{}/v1", address.port());
        self.service.open();
        let stop = Arc::new(AtomicBool::new(false));
        let server = Arc::new(server);
        let worker = {
            let server = Arc::clone(&server);
            let service = Arc::clone(&self.service);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("ezagent-swarm".to_string())
                .spawn(move || serve(server, service, stop))?
        };
        *running = Some(Running {
            server,
            thread: worker,
            stop,
            url: url.clone(),
        });
        self.publish(&url);
        info!("Servicio del enjambre en {}", url);
        Ok(url)
    }

    /// Stops the server and empties the published address.
    pub fn stop(&self) {
        let taken = self
            .running
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let Some(running) = taken else {
            return;
        };
        self.service.close();
        running.stop.store(true, Ordering::SeqCst);
        running.server.unblock();
        let _ = running.thread.join();
        drop(running.server);
        self.publish("");
    }

    /// Writes `endpoint.json` atomically (empty when stopped).
    fn publish(&self, url: &str) {
        if let Err(error) = self.try_publish(url) {
            warn!("No pude publicar la dirección del enjambre: {}", error);
        }
    }

    fn try_publish(&self, url: &str) -> io::Result<()> {
        if let Some(parent) = self.endpoint_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let text = dumps_with(
            &json!({"url": url, "pid": std::process::id()}),
            &DumpOptions::default(),
        );
        let mut name = self
            .endpoint_path
            .file_name()
            .map_or_else(OsString::new, |name| name.to_os_string());
        name.push(".tmp");
        let tmp = self.endpoint_path.with_file_name(name);
        fs::write(&tmp, text)?;
        fs::rename(tmp, &self.endpoint_path)
    }
}

/// Accepts requests until `stop` is set and one request gets its own thread.
fn serve(server: Arc<Server>, service: Arc<SwarmService>, stop: Arc<AtomicBool>) {
    loop {
        match server.recv() {
            Ok(request) => {
                let service = Arc::clone(&service);
                if let Err(error) = thread::Builder::new().spawn(move || handle(&service, request))
                {
                    error!("No pude atender una petición del enjambre: {}", error);
                }
            }
            Err(error) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                debug!("Error al recibir una petición del enjambre: {}", error);
            }
        }
    }
}

fn handle(service: &SwarmService, mut request: Request) {
    let (status, reply) = route(service, &mut request);
    let data = dumps_with(
        &json!({"ok": reply.ok, "text": reply.text}),
        &DumpOptions::utf8(),
    )
    .into_bytes();
    let response = Response::from_data(data)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Server", "EzSwarm/1"));
    if let Err(error) = request.respond(response) {
        debug!("No pude responder a una petición del enjambre: {}", error);
    }
}

fn route(service: &SwarmService, request: &mut Request) -> (u16, Reply) {
    if request.method() != &Method::Post || request.url() != "/v1" || !local_host(request) {
        return (404, failure(NOT_FOUND));
    }
    let Some(token) = bearer(request) else {
        return (401, failure(MISSING_TOKEN));
    };
    let Some(argv) = arguments(request) else {
        return (400, failure(BAD_REQUEST));
    };
    match service.handle(&token, &argv) {
        Ok(reply) => (200, reply),
        Err(error) => {
            // Python closes the connection without a reply; an explicit 500 lets
            // `ezs` retry instead of waiting for its timeout.
            // DECISION: visible internal error, never the body.
            error!("Fallo al atender una petición del enjambre: {}", error);
            (500, failure(""))
        }
    }
}

fn failure(text: &str) -> Reply {
    Reply {
        ok: false,
        text: text.to_string(),
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid HTTP header")
}

fn header_value<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn local_host(request: &Request) -> bool {
    let host = header_value(request, "Host").unwrap_or("");
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    LOCAL_HOSTS.contains(&name)
}

fn bearer(request: &Request) -> Option<String> {
    let value = header_value(request, "Authorization").unwrap_or("");
    let (scheme, token) = value
        .split_once(' ')
        .map_or((value, ""), |(scheme, token)| (scheme, token));
    if scheme != "Bearer" {
        return None;
    }
    let token = ez_core::pyvalue::py_strip(token);
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn arguments(request: &mut Request) -> Option<Vec<String>> {
    let length = request.body_length()?;
    if length == 0 || length > MAX_REQUEST_BYTES {
        return None;
    }
    let mut body = Vec::with_capacity(length);
    request
        .as_reader()
        .take(length as u64)
        .read_to_end(&mut body)
        .ok()?;
    let text = String::from_utf8(body).ok()?;
    let value = ez_core::pyloads::loads(&text).ok()?;
    let Value::Object(data) = value else {
        return None;
    };
    let Some(Value::Array(items)) = data.get("argv") else {
        return None;
    };
    if items.len() > MAX_ARGS {
        return None;
    }
    Some(items.iter().map(python_str).collect())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::Duration;

    use super::*;
    use crate::db::test_support::{temp_board, TempBoard};
    use crate::db::Db;
    use crate::service::MemberSpec;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("ez-swarm-server-{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir_all(&path).expect("temporary folder");
            Self(path)
        }

        fn endpoint(&self) -> PathBuf {
            self.0.join("endpoint.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("endpoint.json"));
            let _ = fs::remove_file(self.0.join("endpoint.json.tmp"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    fn service() -> (Arc<SwarmService>, TempBoard, String) {
        let board = temp_board(0.0);
        let db = Db::new(board.path.clone(), Arc::new(|| 0.0)).expect("temporary board");
        let service = SwarmService::new(db, "ezs", Arc::new(|| 0.0));
        let members = [
            MemberSpec::new("ana", "plan", "claude"),
            MemberSpec::new("bo", "dev", "opencode"),
        ];
        let tasks = [("Portar".to_string(), "dev".to_string())];
        let (_, seats) = service
            .start("Migrar", &members, "", Some(&tasks))
            .expect("swarm start");
        let token = seats
            .iter()
            .find(|seat| seat.name == "bo")
            .map(|seat| seat.token.clone())
            .expect("bo seat");
        (Arc::new(service), board, token)
    }

    fn raw(
        port: u16,
        method: &str,
        path: &str,
        host: Option<&str>,
        token: Option<&str>,
        body: &[u8],
    ) -> (u16, String) {
        let mut stream = TcpStream::connect((HOST, port)).expect("connect to the server");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut request = format!("{method} {path} HTTP/1.0\r\n");
        if let Some(host) = host {
            request.push_str(&format!("Host: {host}\r\n"));
        }
        if let Some(token) = token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        ));
        let mut bytes = request.into_bytes();
        bytes.extend_from_slice(body);
        stream.write_all(&bytes).expect("send request");
        let mut response = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => response.extend_from_slice(&buffer[..count]),
                Err(error) => panic!("leyendo la respuesta: {error}"),
            }
        }
        let separator = b"\r\n\r\n";
        let split = response
            .windows(separator.len())
            .position(|window| window == separator)
            .expect("respuesta HTTP completa");
        let head = String::from_utf8_lossy(&response[..split]);
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("código de estado");
        let payload = String::from_utf8_lossy(&response[split + separator.len()..]).into_owned();
        (status, payload)
    }

    fn port(server: &SwarmServer) -> u16 {
        server
            .url()
            .rsplit(':')
            .next()
            .and_then(|rest| rest.split('/').next())
            .and_then(|value| value.parse().ok())
            .expect("server port")
    }

    fn published(server: &SwarmServer) -> Value {
        let text = fs::read_to_string(&server.endpoint_path).expect("published endpoint");
        ez_core::pyloads::loads(&text).expect("endpoint JSON")
    }

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn publishes_the_url_it_listens_on() {
        let (service, _board, _token) = service();
        let folder = TempDir::new();
        let server = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        let url = server.start().expect("server start");
        assert!(url.starts_with("http://127.0.0.1:"));
        assert_eq!(published(&server)["url"], Value::String(url));
        server.stop();
    }

    #[test]
    fn rejects_requests_without_token_bad_body_or_foreign_host() {
        let (service, _board, token) = service();
        let folder = TempDir::new();
        let server = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        server.start().expect("server start");
        let port = port(&server);
        let body = br#"{"argv": ["board"]}"#;
        let (status, payload) = raw(port, "POST", "/v1", Some("127.0.0.1"), None, body);
        assert_eq!(status, 401);
        assert!(
            payload.contains("Falta el token del enjambre."),
            "{payload}"
        );
        for bad in [&b"no es json"[..], &br#"{"argv": "next"}"#[..]] {
            assert_eq!(
                raw(port, "POST", "/v1", Some("127.0.0.1"), Some(&token), bad).0,
                400
            );
        }
        let long = vec![b'x'; 20_000];
        assert_eq!(
            raw(port, "POST", "/v1", Some("127.0.0.1"), Some(&token), &long).0,
            400
        );
        assert_eq!(
            raw(
                port,
                "POST",
                "/v1",
                Some("evil.example"),
                Some(&token),
                body
            )
            .0,
            404
        );
        assert_eq!(raw(port, "POST", "/v1", None, Some(&token), body).0, 404);
        assert_eq!(
            raw(port, "GET", "/v1", Some("127.0.0.1"), Some(&token), b"").0,
            404
        );
        server.stop();
    }

    #[test]
    fn answers_a_valid_command() {
        let (service, _board, token) = service();
        let folder = TempDir::new();
        let server = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        server.start().expect("server start");
        let port = port(&server);
        let (status, payload) = raw(
            port,
            "POST",
            "/v1",
            Some("localhost"),
            Some(&token),
            br#"{"argv": ["board"]}"#,
        );
        assert_eq!(status, 200);
        assert!(
            payload.starts_with(r#"{"ok": true, "text": "Enjambre"#),
            "{payload}"
        );
        let (status, payload) = raw(
            port,
            "POST",
            "/v1",
            Some("127.0.0.1"),
            Some(&token),
            br#"{"argv": ["nope"]}"#,
        );
        assert_eq!(status, 200);
        assert!(
            payload.starts_with(r#"{"ok": false, "text": "Comando desconocido: nope."#),
            "{payload}"
        );
        server.stop();
    }

    #[test]
    fn start_is_idempotent_and_stop_publishes_an_empty_url() {
        let (service, _board, _token) = service();
        let folder = TempDir::new();
        let server = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        let first = server.start().expect("server start");
        assert_eq!(server.start().expect("second start"), first);
        assert_eq!(published(&server)["url"], Value::String(first));
        server.stop();
        assert_eq!(published(&server)["url"], Value::String(String::new()));
        assert_eq!(server.url(), "");
        server.stop();
    }

    #[test]
    fn restarts_on_another_port_and_keeps_the_tokens() {
        let (service, _board, token) = service();
        let folder = TempDir::new();
        let first = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        first.start().expect("server start");
        first.stop();
        let second = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        second.start().expect("second server start");
        let env = environment(&[
            ("EZS_ENDPOINT", &folder.endpoint().to_string_lossy()),
            ("EZS_TOKEN", &token),
        ]);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = crate::cli::main_with(&["inbox".to_string()], &env, &mut out, &mut err);
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        second.stop();
    }

    #[test]
    fn cli_and_server_round_trip() {
        let (service, _board, token) = service();
        let folder = TempDir::new();
        let server = SwarmServer::new(Arc::clone(&service), folder.endpoint());
        server.start().expect("server start");
        let endpoint = folder.endpoint().to_string_lossy().into_owned();
        let env = environment(&[("EZS_ENDPOINT", &endpoint), ("EZS_TOKEN", &token)]);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = crate::cli::main_with(&["next".to_string()], &env, &mut out, &mut err);
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        let reply = String::from_utf8_lossy(&out).into_owned();
        assert!(reply.contains("Tu tarea: T1 Portar"), "{reply}");
        let env = environment(&[("EZS_ENDPOINT", &endpoint), ("EZS_TOKEN", "otro")]);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = crate::cli::main_with(&["board".to_string()], &env, &mut out, &mut err);
        assert_eq!(code, 1);
        assert!(String::from_utf8_lossy(&err).contains("no tiene un asiento válido"));
        server.stop();
    }
}
