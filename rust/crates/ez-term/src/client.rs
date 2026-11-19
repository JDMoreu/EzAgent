//! Cliente de peticiones de `hc connect`.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::conn::{Conn, CALL_TIMEOUT};
use crate::error::TermError;
use crate::types::{ReadText, RunSpec, SendOutcome, Terminal};

/// Topes de espera del cliente (§14.8, m01).
#[derive(Clone, Copy)]
struct Timing {
    /// Llamadas simples: 30 s (el tope del transporte).
    simple: Duration,
    /// `terminal.send`: 75 s (borrador y arranque del programa).
    send: Duration,
    /// Margen sobre el tope que pide el llamador a `terminal.wait`: 10 s.
    wait_extra: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            simple: CALL_TIMEOUT,
            send: Duration::from_secs(75),
            wait_extra: Duration::from_secs(10),
        }
    }
}

/// Cliente síncrono de `hc connect`; se puede clonar y usar desde varios hilos.
///
/// Tres topes (§14.8): las llamadas simples usan 30 s, `send` usa 75 s y `wait`
/// usa el tope que pide el llamador más 10 s de margen.
#[derive(Clone)]
pub struct HcClient {
    conn: Arc<Conn>,
    timing: Timing,
}

impl HcClient {
    /// Lanza `hc connect` y hace `hello`.
    pub fn connect(hc: &Path) -> Result<HcClient, TermError> {
        let conn = Conn::spawn_hc(hc, "ez-term-reader", None)?;
        Self::handshake(conn)
    }

    /// Igual que `connect` sobre tuberías dadas (pruebas).
    pub fn from_io<R, W>(reader: R, writer: W) -> Result<HcClient, TermError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let conn = Conn::new(reader, writer, "ez-term-reader", None)?;
        Self::handshake(conn)
    }

    fn handshake(conn: Conn) -> Result<HcClient, TermError> {
        conn.hello()?;
        Ok(HcClient {
            conn: Arc::new(conn),
            timing: Timing::default(),
        })
    }

    /// Sobrescribe los topes con valores cortos para pruebas; no usar en producción.
    #[doc(hidden)]
    pub fn with_timing(mut self, simple: Duration, send: Duration, wait_extra: Duration) -> Self {
        self.timing = Timing {
            simple,
            send,
            wait_extra,
        };
        self
    }

    /// Versión del núcleo que respondió `hello`.
    pub fn core_version(&self) -> String {
        self.conn.core_version()
    }

    /// Petición JSON-RPC cualquiera, con emparejamiento por `id` y tope.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, TermError> {
        self.conn.request(method, params, timeout)
    }

    /// Lanza un programa en una terminal nueva.
    pub fn run(&self, spec: &RunSpec) -> Result<Terminal, TermError> {
        let value = self.call("terminal.run", spec.params(), self.timing.simple)?;
        decode(value, "terminal.run")
    }

    /// Terminales vivas que llevan todas esas etiquetas.
    pub fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, TermError> {
        // DECISION: sin `all`, `hc` devuelve solo vivas, que es el inventario.
        let value = self.call(
            "terminal.list",
            json!({ "labels": labels }),
            self.timing.simple,
        )?;
        let reply: ListReply = decode(value, "terminal.list")?;
        Ok(reply.terminals)
    }

    /// Los datos de una terminal, viva o terminada.
    pub fn get(&self, terminal: &str) -> Result<Terminal, TermError> {
        let value = self.call(
            "terminal.get",
            json!({ "terminal": terminal }),
            self.timing.simple,
        )?;
        decode(value, "terminal.get")
    }

    /// La pantalla más `lines` de historial.
    pub fn read(&self, terminal: &str, lines: u32) -> Result<ReadText, TermError> {
        let value = self.call(
            "terminal.read",
            json!({ "terminal": terminal, "lines": lines }),
            self.timing.simple,
        )?;
        decode(value, "terminal.read")
    }

    /// Pega texto y pulsa Enter; usa el tope de 75 s de `send`.
    pub fn send(&self, terminal: &str, text: &str) -> Result<SendOutcome, TermError> {
        let value = self.call(
            "terminal.send",
            json!({ "terminal": terminal, "text": text }),
            self.timing.send,
        )?;
        decode(value, "terminal.send")
    }

    /// Pulsa teclas con nombre (`enter`, `ctrl+c`, ...).
    pub fn keys(&self, terminal: &str, keys: &[String]) -> Result<(), TermError> {
        self.call(
            "terminal.keys",
            json!({ "terminal": terminal, "keys": keys }),
            self.timing.simple,
        )?;
        Ok(())
    }

    /// Pulsa la tecla de parada del programa.
    pub fn interrupt(&self, terminal: &str) -> Result<(), TermError> {
        self.call(
            "terminal.interrupt",
            json!({ "terminal": terminal }),
            self.timing.simple,
        )?;
        Ok(())
    }

    /// Termina el programa de una terminal.
    pub fn kill(&self, terminal: &str) -> Result<(), TermError> {
        self.call(
            "terminal.kill",
            json!({ "terminal": terminal }),
            self.timing.simple,
        )?;
        Ok(())
    }

    /// Borra una terminal terminada.
    pub fn rm(&self, terminal: &str) -> Result<(), TermError> {
        self.call(
            "terminal.rm",
            json!({ "terminal": terminal }),
            self.timing.simple,
        )?;
        Ok(())
    }

    /// Espera a que la terminal deje de trabajar; el tope real es
    /// `timeout_s` más el margen de la conexión.
    pub fn wait(&self, terminal: &str, timeout_s: f64) -> Result<Value, TermError> {
        let seconds = if timeout_s.is_finite() {
            timeout_s.clamp(0.0, 86_400.0)
        } else {
            0.0
        };
        let budget = Duration::from_secs_f64(seconds) + self.timing.wait_extra;
        self.call(
            "terminal.wait",
            json!({ "terminal": terminal, "timeout": seconds }),
            budget,
        )
    }
}

#[derive(Deserialize)]
struct ListReply {
    #[serde(default)]
    terminals: Vec<Terminal>,
}

fn decode<T: DeserializeOwned>(value: Value, method: &str) -> Result<T, TermError> {
    serde_json::from_value(value).map_err(|error| TermError::Decode(format!("{method}: {error}")))
}
