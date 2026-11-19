//! Servidor HTTP local del protocolo (diseño §7 y §14.8).
//!
//! Solo escucha en `127.0.0.1`, solo acepta `POST /v1/command` y exige
//! `Authorization: Bearer <token>`. Un hilo reparte las peticiones: los comandos
//! van a [`WORKERS`] hilos y los `wait` a otros [`MAX_WAITS`], de modo que una
//! espera larga no bloquea un `report` ni un `status`.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::json;
use tiny_http::{Header, Method, Request as HttpRequest, Response, Server, StatusCode};
use tracing::{debug, info, warn};

use crate::protocol::{Caller, Handler, Reply, Request, Tokens};

/// Dirección del servidor: solo bucle local.
pub const HOST: &str = "127.0.0.1";
/// Hilos de atención de comandos.
pub const WORKERS: usize = 8;
/// Esperas simultáneas máximas, en hilos aparte.
pub const MAX_WAITS: usize = 32;
/// Cuerpo máximo aceptado: 1 MiB.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

const TOKEN_MESSAGE: &str = "Token inválido o ausente.";
const BAD_REQUEST: &str = "Petición inválida.";
const NOT_FOUND: &str = "No encontrado.";
const TOO_LARGE: &str = "Petición demasiado grande.";
const TOO_MANY_WAITS: &str = "Demasiadas esperas.";
const ROUTE: &str = "/v1/command";
/// Espera de cada hilo antes de volver a mirar la señal de parada.
const POLL: Duration = Duration::from_millis(100);

/// Servidor del protocolo mientras está encendido.
pub struct CtlServer {
    stop: Arc<AtomicBool>,
    acceptor: Option<JoinHandle<()>>,
    commands: Vec<JoinHandle<()>>,
    url: String,
}

impl CtlServer {
    /// Empieza a escuchar en un puerto libre de `127.0.0.1`.
    pub fn start(handler: Arc<dyn Handler>, tokens: Arc<Tokens>) -> io::Result<CtlServer> {
        Self::start_on(0, handler, tokens)
    }

    /// Empieza a escuchar en `port` de `127.0.0.1`; `0` elige un puerto libre.
    pub fn start_on(
        port: u16,
        handler: Arc<dyn Handler>,
        tokens: Arc<Tokens>,
    ) -> io::Result<CtlServer> {
        let server = Arc::new(
            Server::http((HOST, port)).map_err(|error| io::Error::other(error.to_string()))?,
        );
        let address = server.server_addr().to_ip().ok_or_else(|| {
            io::Error::other("el servidor de EzAgent no escucha en una dirección IP")
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let active_waits = Arc::new(AtomicUsize::new(0));
        let (command_tx, command_rx) = unbounded::<Job>();
        let (wait_tx, wait_rx) = unbounded::<Job>();
        let commands = spawn_commands(&handler, command_rx)?;
        spawn_waits(&handler, wait_rx, &active_waits);

        let acceptor = Acceptor {
            server: Arc::clone(&server),
            tokens,
            stop: Arc::clone(&stop),
            commands: command_tx,
            waits: wait_tx,
            active_waits,
        };
        let built = thread::Builder::new()
            .name("ez-ctl-accept".to_string())
            .spawn(move || acceptor.run());
        let acceptor = match built {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::SeqCst);
                join(commands);
                return Err(error);
            }
        };

        let url = format!("http://{HOST}:{}", address.port());
        info!("Servidor de EzAgent en {url}");
        Ok(CtlServer {
            stop,
            acceptor: Some(acceptor),
            commands,
            url,
        })
    }

    /// Dirección donde escucha, por ejemplo `http://127.0.0.1:51234`.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Deja de aceptar, espera a los comandos y no espera a los `wait` en curso.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(acceptor) = self.acceptor {
            if acceptor.join().is_err() {
                warn!("El hilo de reparto de EzAgent terminó con pánico");
            }
        }
        join(self.commands);
        // DECISION: los hilos de wait se abandonan; vuelven solos cuando el Handler responde.
    }
}

fn join(threads: Vec<JoinHandle<()>>) {
    for thread in threads {
        if thread.join().is_err() {
            warn!("Un hilo del servidor de EzAgent terminó con pánico");
        }
    }
}

fn spawn_commands(
    handler: &Arc<dyn Handler>,
    rx: Receiver<Job>,
) -> io::Result<Vec<JoinHandle<()>>> {
    let mut threads = Vec::with_capacity(WORKERS);
    for index in 0..WORKERS {
        let worker = CommandWorker {
            handler: Arc::clone(handler),
            rx: rx.clone(),
        };
        match thread::Builder::new()
            .name(format!("ez-ctl-http-{index}"))
            .spawn(move || worker.run())
        {
            Ok(handle) => threads.push(handle),
            Err(error) => {
                drop(rx);
                join(threads);
                return Err(error);
            }
        }
    }
    Ok(threads)
}

fn spawn_waits(handler: &Arc<dyn Handler>, rx: Receiver<Job>, active: &Arc<AtomicUsize>) {
    for index in 0..MAX_WAITS {
        let worker = WaitWorker {
            handler: Arc::clone(handler),
            rx: rx.clone(),
            active: Arc::clone(active),
        };
        if thread::Builder::new()
            .name(format!("ez-ctl-wait-{index}"))
            .spawn(move || worker.run())
            .is_err()
        {
            warn!("No pude iniciar un hilo de espera de EzAgent");
        }
    }
}

struct Job {
    http: HttpRequest,
    caller: Caller,
    command: Request,
}

struct Acceptor {
    server: Arc<Server>,
    tokens: Arc<Tokens>,
    stop: Arc<AtomicBool>,
    commands: Sender<Job>,
    waits: Sender<Job>,
    active_waits: Arc<AtomicUsize>,
}

impl Acceptor {
    fn run(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            match self.server.recv_timeout(POLL) {
                Ok(Some(http)) => self.dispatch(http),
                Ok(None) => {}
                Err(error) => debug!("No pude recibir una petición de EzAgent: {error}"),
            }
        }
    }

    fn dispatch(&self, mut http: HttpRequest) {
        let (caller, command) = match prepare(&mut http, &self.tokens) {
            Ok(pair) => pair,
            Err((status, reply)) => {
                answer(http, status, reply);
                return;
            }
        };
        let is_wait = command.command == "wait";
        let job = Job {
            http,
            caller,
            command,
        };
        if is_wait {
            if try_acquire_wait(&self.active_waits) {
                if self.waits.send(job).is_err() {
                    release_wait(&self.active_waits);
                }
            } else {
                answer(job.http, 503, Reply::failure(TOO_MANY_WAITS));
            }
        } else if self.commands.send(job).is_err() {
            warn!("No pude repartir una petición de EzAgent");
        }
    }
}

struct CommandWorker {
    handler: Arc<dyn Handler>,
    rx: Receiver<Job>,
}

impl CommandWorker {
    fn run(&self) {
        while let Ok(job) = self.rx.recv() {
            let reply = self.handler.handle(&job.caller, &job.command);
            answer(job.http, 200, reply);
        }
    }
}

struct WaitWorker {
    handler: Arc<dyn Handler>,
    rx: Receiver<Job>,
    active: Arc<AtomicUsize>,
}

impl WaitWorker {
    fn run(&self) {
        while let Ok(job) = self.rx.recv() {
            let reply = self.handler.handle(&job.caller, &job.command);
            release_wait(&self.active);
            answer(job.http, 200, reply);
        }
    }
}

fn prepare(request: &mut HttpRequest, tokens: &Tokens) -> Result<(Caller, Request), (u16, Reply)> {
    // DECISION: la ruta se valida antes que el token para que una ruta
    // desconocida responda 404 aunque no traiga credenciales.
    if request.method() != &Method::Post || request.url() != ROUTE {
        return Err((404, Reply::failure(NOT_FOUND)));
    }
    let Some(caller) = caller(request, tokens) else {
        return Err((401, Reply::failure(TOKEN_MESSAGE)));
    };
    let body = match read_body(request) {
        Ok(body) => body,
        Err(Problem::TooLarge) => return Err((413, Reply::failure(TOO_LARGE))),
        Err(Problem::Unreadable) => return Err((400, Reply::failure(BAD_REQUEST))),
    };
    let Ok(mut parsed) = serde_json::from_slice::<Request>(&body) else {
        return Err((400, Reply::failure(BAD_REQUEST)));
    };
    if parsed.command.trim().is_empty() {
        return Err((400, Reply::failure(BAD_REQUEST)));
    }
    if parsed.args.is_null() {
        parsed.args = json!({});
    }
    Ok((caller, parsed))
}

fn answer(request: HttpRequest, status: u16, reply: Reply) {
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

fn caller(request: &HttpRequest, tokens: &Tokens) -> Option<Caller> {
    let header = header(request, "Authorization")?;
    let (scheme, token) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    tokens.lookup(token)
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

fn try_acquire_wait(active: &AtomicUsize) -> bool {
    let mut count = active.load(Ordering::SeqCst);
    loop {
        if count >= MAX_WAITS {
            return false;
        }
        match active.compare_exchange_weak(count, count + 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(found) => count = found,
        }
    }
}

fn release_wait(active: &AtomicUsize) {
    active.fetch_sub(1, Ordering::SeqCst);
}
