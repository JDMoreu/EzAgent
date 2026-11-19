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

/// Margen que se suma al tope de una operación que puede durar más que una
/// llamada normal: el paso de espera de `send` (borrador y arranque) y el
/// tope de `wait`.
// DECISION: `send` no recibe timeout del llamador; se usa el tope normal + 10 s.
const EXTRA_TIMEOUT: Duration = Duration::from_secs(10);

/// Cliente síncrono de `hc connect`; se puede clonar y usar desde varios hilos.
#[derive(Clone)]
pub struct HcClient {
    conn: Arc<Conn>,
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
        })
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
        let value = self.call("terminal.run", spec.params(), CALL_TIMEOUT)?;
        decode(value, "terminal.run")
    }

    /// Terminales vivas que llevan todas esas etiquetas.
    pub fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, TermError> {
        // DECISION: sin `all`, `hc` devuelve solo vivas, que es el inventario.
        let value = self.call("terminal.list", json!({ "labels": labels }), CALL_TIMEOUT)?;
        let reply: ListReply = decode(value, "terminal.list")?;
        Ok(reply.terminals)
    }

    /// Los datos de una terminal, viva o terminada.
    pub fn get(&self, terminal: &str) -> Result<Terminal, TermError> {
        let value = self.call(
            "terminal.get",
            json!({ "terminal": terminal }),
            CALL_TIMEOUT,
        )?;
        decode(value, "terminal.get")
    }

    /// La pantalla más `lines` de historial.
    pub fn read(&self, terminal: &str, lines: u32) -> Result<ReadText, TermError> {
        let value = self.call(
            "terminal.read",
            json!({ "terminal": terminal, "lines": lines }),
            CALL_TIMEOUT,
        )?;
        decode(value, "terminal.read")
    }

    /// Pega texto y pulsa Enter.
    pub fn send(&self, terminal: &str, text: &str) -> Result<SendOutcome, TermError> {
        let value = self.call(
            "terminal.send",
            json!({ "terminal": terminal, "text": text }),
            CALL_TIMEOUT + EXTRA_TIMEOUT,
        )?;
        decode(value, "terminal.send")
    }

    /// Pulsa teclas con nombre (`enter`, `ctrl+c`, ...).
    pub fn keys(&self, terminal: &str, keys: &[String]) -> Result<(), TermError> {
        self.call(
            "terminal.keys",
            json!({ "terminal": terminal, "keys": keys }),
            CALL_TIMEOUT,
        )?;
        Ok(())
    }

    /// Pulsa la tecla de parada del programa.
    pub fn interrupt(&self, terminal: &str) -> Result<(), TermError> {
        self.call(
            "terminal.interrupt",
            json!({ "terminal": terminal }),
            CALL_TIMEOUT,
        )?;
        Ok(())
    }

    /// Termina el programa de una terminal.
    pub fn kill(&self, terminal: &str) -> Result<(), TermError> {
        self.call(
            "terminal.kill",
            json!({ "terminal": terminal }),
            CALL_TIMEOUT,
        )?;
        Ok(())
    }

    /// Borra una terminal terminada.
    pub fn rm(&self, terminal: &str) -> Result<(), TermError> {
        self.call("terminal.rm", json!({ "terminal": terminal }), CALL_TIMEOUT)?;
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
        let budget = Duration::from_secs_f64(seconds) + EXTRA_TIMEOUT;
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
