//! Servidor HTTP local del protocolo (diseño §7).
//!
//! Solo escucha en `127.0.0.1`, solo acepta `POST /v1/command` y exige
//! `Authorization: Bearer <token>`. Cada petición se atiende en uno de los
//! [`WORKERS`] hilos, así que un `wait` largo no bloquea al resto.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;
use tiny_http::{Header, Method, Request as HttpRequest, Response, Server, StatusCode};
use tracing::{debug, info, warn};

use crate::protocol::{Caller, Handler, Reply, Request, Tokens};

/// Dirección del servidor: solo bucle local.
pub const HOST: &str = "127.0.0.1";
/// Hilos de atención.
pub const WORKERS: usize = 8;
/// Cuerpo máximo aceptado: 1 MiB.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

const TOKEN_MESSAGE: &str = "Token inválido o ausente.";
const BAD_REQUEST: &str = "Petición inválida.";
const NOT_FOUND: &str = "No encontrado.";
const TOO_LARGE: &str = "Petición demasiado grande.";
const ROUTE: &str = "/v1/command";
/// Espera de cada hilo antes de volver a mirar la señal de parada.
const POLL: Duration = Duration::from_millis(100);

/// Servidor del protocolo mientras está encendido.
pub struct CtlServer {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    url: String,
}

impl CtlServer {
    /// Empieza a escuchar en un puerto libre de `127.0.0.1`.
    pub fn start(handler: Arc<dyn Handler>, tokens: Arc<Tokens>) -> io::Result<CtlServer> {
        let server =
            Arc::new(Server::http((HOST, 0)).map_err(|error| io::Error::other(error.to_string()))?);
        let address = server.server_addr().to_ip().ok_or_else(|| {
            io::Error::other("el servidor de EzAgent no escucha en una dirección IP")
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::with_capacity(WORKERS);
        for index in 0..WORKERS {
            let worker = Worker {
                server: Arc::clone(&server),
                handler: Arc::clone(&handler),
                tokens: Arc::clone(&tokens),
                stop: Arc::clone(&stop),
            };
            let builder = thread::Builder::new().name(format!("ez-ctl-http-{index}"));
            match builder.spawn(move || worker.run()) {
                Ok(handle) => threads.push(handle),
                Err(error) => {
                    stop.store(true, Ordering::SeqCst);
                    join(threads);
                    return Err(error);
                }
            }
        }
        let url = format!("http://{HOST}:{}", address.port());
        info!("Servidor de EzAgent en {url}");
        Ok(CtlServer { stop, threads, url })
    }

    /// Dirección donde escucha, por ejemplo `http://127.0.0.1:51234`.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Cierra el servidor y espera a los hilos de atención.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        join(self.threads);
    }
}

fn join(threads: Vec<JoinHandle<()>>) {
    for thread in threads {
        if thread.join().is_err() {
            warn!("Un hilo del servidor de EzAgent terminó con pánico");
        }
    }
}

struct Worker {
    server: Arc<Server>,
    handler: Arc<dyn Handler>,
    tokens: Arc<Tokens>,
    stop: Arc<AtomicBool>,
}

impl Worker {
    fn run(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            match self.server.recv_timeout(POLL) {
                Ok(Some(request)) => self.answer(request),
                Ok(None) => {}
                Err(error) => debug!("No pude recibir una petición de EzAgent: {error}"),
            }
        }
    }

    fn answer(&self, mut request: HttpRequest) {
        let (status, reply) = self.route(&mut request);
        let body = serde_json::to_vec(&reply).unwrap_or_else(|_| {
            b"{\"ok\":false,\"message\":\"Error interno.\",\"data\":null}".to_vec()
        });
        let mut response = Response::from_data(body)
            .with_status_code(StatusCode(status))
            .with_header(json_header());
        if status != 200 {
            // El cuerpo del error puede no haberse leído entero: no reutilizar la conexión.
            // DECISION: todo error cierra la conexión para no dejar bytes sin leer.
            response = response.with_header(close_header());
        }
        if let Err(error) = request.respond(response) {
            debug!("No pude responder a una petición de EzAgent: {error}");
        }
    }

    fn route(&self, request: &mut HttpRequest) -> (u16, Reply) {
        // DECISION: la ruta se valida antes que el token para que una ruta
        // desconocida responda 404 aunque no traiga credenciales.
        if request.method() != &Method::Post || request.url() != ROUTE {
            return (404, Reply::failure(NOT_FOUND));
        }
        let Some(caller) = self.caller(request) else {
            return (401, Reply::failure(TOKEN_MESSAGE));
        };
        let body = match read_body(request) {
            Ok(body) => body,
            Err(Problem::TooLarge) => return (413, Reply::failure(TOO_LARGE)),
            Err(Problem::Unreadable) => return (400, Reply::failure(BAD_REQUEST)),
        };
        let Ok(mut parsed) = serde_json::from_slice::<Request>(&body) else {
            return (400, Reply::failure(BAD_REQUEST));
        };
        if parsed.command.trim().is_empty() {
            return (400, Reply::failure(BAD_REQUEST));
        }
        if parsed.args.is_null() {
            parsed.args = json!({});
        }
        (200, self.handler.handle(&caller, &parsed))
    }

    fn caller(&self, request: &HttpRequest) -> Option<Caller> {
        let header = header(request, "Authorization")?;
        let (scheme, token) = header.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("Bearer") {
            return None;
        }
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        self.tokens.lookup(token)
    }
}

enum Problem {
    TooLarge,
    Unreadable,
}

fn read_body(request: &mut HttpRequest) -> Result<Vec<u8>, Problem> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| Problem::Unreadable)?;
    if body.len() > MAX_BODY_BYTES {
        return Err(Problem::TooLarge);
    }
    Ok(body)
}

fn header<'a>(request: &'a HttpRequest, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn json_header() -> Header {
    Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=utf-8"[..],
    )
    .expect("cabecera HTTP válida")
}

fn close_header() -> Header {
    Header::from_bytes(&b"Connection"[..], &b"close"[..]).expect("cabecera HTTP válida")
}
