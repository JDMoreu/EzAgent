//! Dobles en memoria para `ez-term`: tuberías de líneas sobre canales y un
//! `hc` falso que responde guiones.

// Cada binario de prueba usa una parte distinta de este módulo.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use ez_term::{EventStream, HcClient};
use serde_json::{json, Value};

/// Lector de líneas que recibe bytes por canal; al cerrarse el canal da EOF.
pub struct PipeReader {
    rx: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    position: usize,
}

impl PipeReader {
    fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: Vec::new(),
            position: 0,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        if self.position < self.buffer.len() {
            return Ok(());
        }
        match self.rx.recv() {
            Ok(bytes) => {
                self.buffer = bytes;
                self.position = 0;
            }
            Err(_) => {
                self.buffer.clear();
                self.position = 0;
            }
        }
        Ok(())
    }
}

impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.fill()?;
        let available = &self.buffer[self.position..];
        let count = available.len().min(out.len());
        out[..count].copy_from_slice(&available[..count]);
        self.position += count;
        Ok(count)
    }
}

impl BufRead for PipeReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.fill()?;
        Ok(&self.buffer[self.position..])
    }

    fn consume(&mut self, amount: usize) {
        self.position = (self.position + amount).min(self.buffer.len());
    }
}

/// Escritor que entrega una línea completa por canal en cada `\n`.
pub struct PipeWriter {
    tx: Sender<String>,
    buffer: Vec<u8>,
}

impl PipeWriter {
    fn new(tx: Sender<String>) -> Self {
        Self {
            tx,
            buffer: Vec::new(),
        }
    }
}

impl Write for PipeWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(data);
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=index).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
            let _ = self.tx.send(text);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let text = String::from_utf8_lossy(&self.buffer).into_owned();
            self.buffer.clear();
            let _ = self.tx.send(text);
        }
        Ok(())
    }
}

/// El otro lado del cliente: recibe sus peticiones y le manda líneas.
pub struct Fake {
    to_client: Sender<Vec<u8>>,
    from_client: Receiver<String>,
}

/// Crea el par cliente/servidor en memoria.
pub fn fake() -> (Fake, PipeReader, PipeWriter) {
    let (to_client, client_reader) = unbounded::<Vec<u8>>();
    let (client_writer, from_client) = unbounded::<String>();
    (
        Fake {
            to_client,
            from_client,
        },
        PipeReader::new(client_reader),
        PipeWriter::new(client_writer),
    )
}

/// Resultado que `hc` devuelve a `hello`.
pub fn hello_result() -> Value {
    json!({
        "core": {"id": "falso", "name": "prueba"},
        "features": [],
        "framed": false,
        "generation": 1,
        "methods": [],
        "version": "0.1.21",
    })
}

/// Cliente sobre un guion que corre en su propio hilo. El guion recibe el
/// `Fake` y debe responder `hello` antes de seguir con las demás peticiones.
pub fn client_scripted<F>(script: F) -> (HcClient, JoinHandle<()>)
where
    F: FnOnce(Fake) + Send + 'static,
{
    let (fake, reader, writer) = fake();
    let handle = thread::spawn(move || script(fake));
    let client = HcClient::from_io(reader, writer).expect("hello");
    (client, handle)
}

/// Igual que `client_scripted`, para la conexión de eventos.
pub fn events_scripted<F>(script: F) -> (EventStream, JoinHandle<()>)
where
    F: FnOnce(Fake) + Send + 'static,
{
    let (fake, reader, writer) = fake();
    let handle = thread::spawn(move || script(fake));
    let labels = BTreeMap::from([("ez".to_string(), "1".to_string())]);
    let stream = EventStream::from_io(reader, writer, &labels).expect("events.subscribe");
    (stream, handle)
}

impl Fake {
    /// Lee `hello` y responde. Se llama al principio de cada guion.
    pub fn hello(&self) {
        let request = self.request();
        assert_eq!(request["method"], "hello");
        assert_eq!(request["params"]["generation"], 1);
        self.respond(&request, hello_result());
    }

    /// Envía una respuesta con ese id.
    pub fn reply_id(&self, id: u64, result: Value) {
        self.raw(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    /// Responde la petición recibida, con su mismo id.
    pub fn respond(&self, request: &Value, result: Value) {
        self.raw(json!({"jsonrpc": "2.0", "id": request["id"], "result": result}));
    }

    /// Responde con un error JSON-RPC con `kind`.
    pub fn respond_error(&self, request: &Value, code: i64, kind: &str, message: &str) {
        self.raw(json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "error": {"code": code, "message": message, "data": {"kind": kind}},
        }));
    }

    /// Notificación `event` de `hc`.
    pub fn notify(&self, params: Value) {
        self.raw(json!({"jsonrpc": "2.0", "method": "event", "params": params}));
    }

    /// Cualquier línea JSON, tal cual.
    pub fn raw(&self, value: Value) {
        let line = format!("{value}\n");
        let _ = self.to_client.send(line.into_bytes());
    }

    /// Lee la siguiente petición del cliente (falla si no llega).
    pub fn request(&self) -> Value {
        let line = self
            .from_client
            .recv_timeout(Duration::from_secs(5))
            .expect("el cliente debía enviar una petición");
        serde_json::from_str(&line).expect("la petición debía ser JSON")
    }
}

/// Un `Terminal` mínimo como los que devuelve `hc`.
pub fn terminal(address: &str, name: &str) -> Value {
    json!({
        "address": address,
        "argv": ["hostname"],
        "cols": 200,
        "created_ms": 1,
        "cwd": "C:\\tmp",
        "labels": {"ez": "1"},
        "name": name,
        "rows": 120,
        "screen": "exact",
        "state": "live",
        "turn": 0,
    })
}
