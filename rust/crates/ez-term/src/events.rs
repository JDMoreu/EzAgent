//! Conexión exclusiva de eventos: `events.subscribe` y la foto inicial.
//!
//! `hc` deja de responder peticiones en una conexión suscrita, así que esta
//! conexión solo transporta notificaciones.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;

use crossbeam_channel::{unbounded, Receiver};
use serde_json::{json, Value};

use crate::conn::{Conn, CALL_TIMEOUT};
use crate::error::TermError;
use crate::types::{Status, Terminal};

/// Cambio que `hc` notifica a los suscriptores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermEvent {
    /// Nació una terminal.
    Created(Terminal),
    /// El programa cambió el estado que reporta.
    Status {
        /// Dirección de la terminal.
        terminal: String,
        /// Estado nuevo.
        status: Status,
    },
    /// El programa terminó.
    Finished {
        /// Dirección de la terminal.
        terminal: String,
        /// Código de salida, si lo hubo.
        exit_code: Option<i32>,
        /// Motivo del final (`exited`, `killed`, ...).
        reason: Option<String>,
    },
    /// La terminal se borró.
    Removed {
        /// Dirección de la terminal.
        terminal: String,
    },
    /// El programa se perdió.
    Lost {
        /// Dirección de la terminal.
        terminal: String,
    },
    /// La terminal se reinició.
    Restarted {
        /// Dirección de la terminal.
        terminal: String,
    },
    /// Cualquier otro cambio (`turn`, `title`, `foreground`, ...).
    Other {
        /// Dirección de la terminal.
        terminal: String,
        /// Tipo de cambio según `hc`.
        kind: String,
        /// Datos tal como llegaron.
        data: Value,
    },
    /// El suscriptor se atrasó; trae la foto nueva si `hc` la mandó.
    Missed {
        /// Terminales de la foto que acompañó al aviso.
        snapshot: Vec<Terminal>,
    },
    /// Señal de vida de `hc` (cada 15 s).
    Heartbeat,
    /// La conexión terminó.
    Closed,
}

/// Conexión de eventos con su foto inicial y su receptor.
pub struct EventStream {
    snapshot: Vec<Terminal>,
    events: Receiver<TermEvent>,
    conn: Option<Conn>,
}

impl EventStream {
    /// Se suscribe a los cambios de las terminales con esas etiquetas.
    pub fn subscribe(
        hc: &Path,
        labels: &BTreeMap<String, String>,
    ) -> Result<EventStream, TermError> {
        let (sender, receiver) = unbounded();
        let conn = Conn::spawn_hc(hc, "ez-term-events", Some(sender))?;
        Self::start(conn, receiver, labels)
    }

    /// Igual que `subscribe` sobre tuberías dadas (pruebas).
    pub fn from_io<R, W>(
        reader: R,
        writer: W,
        labels: &BTreeMap<String, String>,
    ) -> Result<EventStream, TermError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let (sender, receiver) = unbounded();
        let conn = Conn::new(reader, writer, "ez-term-events", Some(sender))?;
        Self::start(conn, receiver, labels)
    }

    fn start(
        conn: Conn,
        events: Receiver<TermEvent>,
        labels: &BTreeMap<String, String>,
    ) -> Result<EventStream, TermError> {
        conn.hello()?;
        let reply = conn.request(
            "events.subscribe",
            json!({ "labels": labels }),
            CALL_TIMEOUT,
        )?;
        let snapshot = reply
            .get("terminals")
            .and_then(Value::as_array)
            .map(|terminals| {
                terminals
                    .iter()
                    .filter_map(|terminal| {
                        serde_json::from_value::<Terminal>(terminal.clone()).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(EventStream {
            snapshot,
            events,
            conn: Some(conn),
        })
    }

    /// Terminales que ya existían al suscribirse.
    pub fn snapshot(&self) -> &[Terminal] {
        &self.snapshot
    }

    /// Cambios posteriores a la suscripción.
    pub fn events(&self) -> &Receiver<TermEvent> {
        &self.events
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        // Al cerrar la conexión se cierra la entrada de `hc` y termina su hijo.
        self.conn.take();
    }
}

/// Traduce una notificación JSON-RPC de `hc` a un `TermEvent`.
pub(crate) fn event_from_notification(value: &Value) -> Option<TermEvent> {
    let method = value.get("method").and_then(Value::as_str)?;
    let params = value.get("params").unwrap_or(&Value::Null);
    match method {
        "heartbeat" => Some(TermEvent::Heartbeat),
        "missed" => Some(TermEvent::Missed {
            snapshot: snapshot_from(params),
        }),
        "event" => event_from_params(params),
        other => {
            tracing::debug!(method = other, "ez-term: notificación desconocida");
            None
        }
    }
}

fn event_from_params(params: &Value) -> Option<TermEvent> {
    let kind = params
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let terminal = params
        .get("terminal")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let data = params.get("data").cloned().unwrap_or(Value::Null);
    Some(match kind {
        "created" => match serde_json::from_value::<Terminal>(data.clone()) {
            Ok(terminal) => TermEvent::Created(terminal),
            Err(error) => {
                tracing::debug!(%error, "ez-term: created sin datos de terminal");
                TermEvent::Other {
                    terminal,
                    kind: kind.to_string(),
                    data,
                }
            }
        },
        "status" => match status_from(&data) {
            Some(status) => TermEvent::Status { terminal, status },
            None => TermEvent::Other {
                terminal,
                kind: kind.to_string(),
                data,
            },
        },
        "finished" => TermEvent::Finished {
            terminal,
            exit_code: data
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| code as i32),
            reason: data
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        "removed" => TermEvent::Removed { terminal },
        "lost" => TermEvent::Lost { terminal },
        "restarted" => TermEvent::Restarted { terminal },
        "heartbeat" => TermEvent::Heartbeat,
        "missed" => TermEvent::Missed {
            snapshot: snapshot_from(params),
        },
        other => TermEvent::Other {
            terminal,
            kind: other.to_string(),
            data,
        },
    })
}

/// Un `data` con `state` es un `Status`; cualquier otro, un evento `Other`.
fn status_from(data: &Value) -> Option<Status> {
    data.get("state")?;
    match serde_json::from_value(data.clone()) {
        Ok(status) => Some(status),
        Err(error) => {
            tracing::debug!(%error, "ez-term: status con forma inesperada");
            None
        }
    }
}

/// Busca la foto en las formas que puede usar el aviso `missed`.
fn snapshot_from(params: &Value) -> Vec<Terminal> {
    let candidates = [
        params.get("terminals"),
        params.get("snapshot"),
        params.pointer("/data/terminals"),
    ];
    let Some(list) = candidates.into_iter().flatten().next() else {
        // DECISION: el esquema no fija la forma del aviso `missed`; si no trae
        // la foto en `terminals`/`snapshot`, se entrega vacía.
        return Vec::new();
    };
    serde_json::from_value(list.clone()).unwrap_or_default()
}
