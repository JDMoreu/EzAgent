//! Transporte de una conexión: hilo escritor, hilo lector y llamadas
//! pendientes por `id`.
//!
//! Ningún lock se retiene mientras se espera una respuesta, se escribe a `hc`
//! ni se espera al subproceso: cada lock cubre solo un acceso al mapa de
//! llamadas pendientes o a la versión.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::error::TermError;
use crate::events::{event_from_notification, TermEvent};

/// Tope por llamada normal.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Respuesta a una petición: el `result` o el error ya traducido.
pub(crate) type Reply = Result<Value, TermError>;

struct Shared {
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    alive: AtomicBool,
}

impl Shared {
    /// Marca la conexión caída y despierta a todas las llamadas pendientes.
    fn fail_all(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let waiting: Vec<Sender<Reply>> = {
            let mut pending = self.pending.lock();
            pending.drain().map(|(_, sender)| sender).collect()
        };
        for sender in waiting {
            let _ = sender.send(Err(TermError::Disconnected));
        }
    }
}

/// Una conexión viva con `hc`; el dueño del proceso es `Conn`.
pub(crate) struct Conn {
    shared: Arc<Shared>,
    next_id: AtomicU64,
    out: Option<Sender<String>>,
    version: Mutex<String>,
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
}

impl Conn {
    /// Prepara los hilos sobre un lector y un escritor ya abiertos.
    pub(crate) fn new<R, W>(
        reader: R,
        writer: W,
        thread_name: &str,
        events: Option<Sender<TermEvent>>,
    ) -> Result<Self, TermError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let (out_tx, out_rx) = unbounded::<String>();
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        });

        let writer_shared = Arc::clone(&shared);
        let writer = Box::new(writer) as Box<dyn Write + Send>;
        std::thread::Builder::new()
            .name("ez-term-writer".into())
            .spawn(move || writer_loop(writer, writer_shared, out_rx))
            .map_err(|error| {
                TermError::Spawn(format!("no se pudo crear el hilo escritor: {error}"))
            })?;

        let reader_shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || reader_loop(reader, reader_shared, events))
            .map_err(|error| {
                TermError::Spawn(format!("no se pudo crear el hilo lector: {error}"))
            })?;

        Ok(Self {
            shared,
            next_id: AtomicU64::new(1),
            out: Some(out_tx),
            version: Mutex::new(String::new()),
            child: None,
            reader: Some(handle),
        })
    }

    /// Lanza `hc connect` y envuelve sus tuberías.
    pub(crate) fn spawn_hc(
        hc: &Path,
        thread_name: &str,
        events: Option<Sender<TermEvent>>,
    ) -> Result<Self, TermError> {
        if !hc.is_file() {
            return Err(TermError::NotFound);
        }
        let mut command = Command::new(hc);
        // DECISION: stderr se descarta porque nadie lo drena y llenaría la tubería.
        command
            .arg("connect")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| TermError::Spawn(error.to_string()))?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err(TermError::Spawn("hc no entregó su entrada estándar".into()));
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(TermError::Spawn("hc no entregó su salida estándar".into()));
        };
        let mut conn = match Self::new(BufReader::new(stdout), stdin, thread_name, events) {
            Ok(conn) => conn,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        conn.child = Some(child);
        Ok(conn)
    }

    /// Hace `hello` y guarda la versión del núcleo.
    pub(crate) fn hello(&self) -> Result<(), TermError> {
        let reply = self.request("hello", json!({ "generation": 1 }), CALL_TIMEOUT)?;
        let version = reply
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        *self.version.lock() = version;
        Ok(())
    }

    /// Versión que `hc` informó en `hello`.
    pub(crate) fn core_version(&self) -> String {
        self.version.lock().clone()
    }

    /// Envía una petición y espera su respuesta emparejada por `id`.
    pub(crate) fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, TermError> {
        if !self.shared.alive.load(Ordering::SeqCst) {
            return Err(TermError::Disconnected);
        }
        let Some(out) = &self.out else {
            return Err(TermError::Disconnected);
        };
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string();

        let (sender, receiver) = bounded::<Reply>(1);
        self.shared.pending.lock().insert(id, sender);
        if out.send(message).is_err() {
            self.shared.fail_all();
            return Err(TermError::Disconnected);
        }

        match receiver.recv_timeout(timeout) {
            Ok(reply) => reply,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                self.shared.pending.lock().remove(&id);
                Err(TermError::Timeout {
                    method: method.to_string(),
                })
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                self.shared.pending.lock().remove(&id);
                Err(TermError::Disconnected)
            }
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        // Al soltar el emisor, el hilo escritor termina y cierra la entrada de hc.
        self.out.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(handle) = self.reader.take() {
                let _ = handle.join();
            }
        }
    }
}

fn writer_loop(mut writer: Box<dyn Write + Send>, shared: Arc<Shared>, out: Receiver<String>) {
    for line in out {
        let written = writer
            .write_all(line.as_bytes())
            .and_then(|()| writer.write_all(b"\n"))
            .and_then(|()| writer.flush());
        if let Err(error) = written {
            tracing::debug!(%error, "ez-term: no se pudo escribir a hc");
            shared.fail_all();
            return;
        }
    }
}

fn reader_loop<R: BufRead>(mut reader: R, shared: Arc<Shared>, events: Option<Sender<TermEvent>>) {
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let line = line.trim();
                if !line.is_empty() {
                    handle_line(&shared, events.as_ref(), line);
                }
            }
            Err(error) => {
                tracing::debug!(%error, "ez-term: no se pudo leer de hc");
                break;
            }
        }
    }
    shared.fail_all();
    if let Some(events) = events {
        let _ = events.send(TermEvent::Closed);
    }
}

fn handle_line(shared: &Shared, events: Option<&Sender<TermEvent>>, line: &str) {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(%error, "ez-term: línea de hc que no es JSON");
            return;
        }
    };
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let reply = match value.get("error") {
            Some(error) => Err(rpc_error(error)),
            None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
        };
        let waiting = shared.pending.lock().remove(&id);
        match waiting {
            Some(sender) => {
                let _ = sender.send(reply);
            }
            None => tracing::debug!(id, "ez-term: respuesta sin llamada pendiente"),
        }
        return;
    }
    let Some(events) = events else {
        tracing::debug!("ez-term: notificación sin conexión de eventos");
        return;
    };
    if let Some(event) = event_from_notification(&value) {
        let _ = events.send(event);
    }
}

fn rpc_error(error: &Value) -> TermError {
    TermError::Rpc {
        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
        kind: error
            .pointer("/data/kind")
            .and_then(Value::as_str)
            .map(str::to_owned),
        message: error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error sin mensaje")
            .to_string(),
    }
}
