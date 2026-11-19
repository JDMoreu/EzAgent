//! Conexión exclusiva de eventos: `events.subscribe` y la foto inicial.
//!
//! `hc` deja de responder peticiones en una conexión suscrita, así que esta
//! conexión solo transporta notificaciones. Cada notificación `event` trae su
//! secuencia (`params.seq`); tras un `missed` con foto se descartan los eventos
//! con `seq` menor o igual, que ya están incluidos en esa foto (§14.9).

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
    Created {
        /// Datos de la terminal.
        terminal: Terminal,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// El programa cambió el estado que reporta.
    Status {
        /// Dirección de la terminal.
        terminal: String,
        /// Estado nuevo.
        status: Status,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// El programa terminó.
    Finished {
        /// Dirección de la terminal.
        terminal: String,
        /// Código de salida, si lo hubo.
        exit_code: Option<i32>,
        /// Motivo del final (`exited`, `killed`, ...).
        reason: Option<String>,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// La terminal se borró.
    Removed {
        /// Dirección de la terminal.
        terminal: String,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// El programa se perdió.
    Lost {
        /// Dirección de la terminal.
        terminal: String,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// La terminal se reinició.
    Restarted {
        /// Dirección de la terminal.
        terminal: String,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// Cualquier otro cambio (`turn`, `title`, `foreground`, ...).
    Other {
        /// Dirección de la terminal.
        terminal: String,
        /// Tipo de cambio según `hc`.
        kind: String,
        /// Datos tal como llegaron.
        data: Value,
        /// Secuencia de la notificación.
        seq: u64,
    },
    /// El suscriptor se atrasó; trae la foto nueva si `hc` la mandó y se pudo leer.
    Missed {
        /// Terminales de la foto; `None` si no se pudo leer (resincronizar con `list`).
        snapshot: Option<Vec<Terminal>>,
        /// Secuencia de la foto; `None` si no se pudo leer.
        seq: Option<u64>,
    },
    /// Señal de vida de `hc` (cada 15 s).
    Heartbeat,
    /// La conexión terminó.
    Closed,
}

/// Conexión de eventos con su foto inicial y su receptor.
pub struct EventStream {
    snapshot: Vec<Terminal>,
    snapshot_seq: Option<u64>,
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
        let snapshot_seq = reply.get("seq").and_then(Value::as_u64);
        Ok(EventStream {
            snapshot,
            snapshot_seq,
            events,
            conn: Some(conn),
        })
    }

    /// Terminales que ya existían al suscribirse.
    pub fn snapshot(&self) -> &[Terminal] {
        &self.snapshot
    }

    /// Secuencia de la foto inicial, si `hc` la informó.
    pub fn snapshot_seq(&self) -> Option<u64> {
        self.snapshot_seq
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

/// Traduce una notificación y aplica el descarte tras `missed`.
/// `min_seq` es la secuencia de la última foto perdida: los eventos con
/// `seq <= min_seq` se descartan porque esa foto ya los incluye.
pub(crate) fn event_from_notification(
    value: &Value,
    min_seq: &mut Option<u64>,
) -> Option<TermEvent> {
    let method = value.get("method").and_then(Value::as_str)?;
    let params = value.get("params").unwrap_or(&Value::Null);
    match method {
        "heartbeat" => Some(TermEvent::Heartbeat),
        "missed" => Some(missed_event(params, min_seq)),
        "event" => event_from_params(params, min_seq),
        other => {
            tracing::debug!(method = other, "ez-term: notificación desconocida");
            None
        }
    }
}

fn missed_event(params: &Value, min_seq: &mut Option<u64>) -> TermEvent {
    // DECISION: sin foto legible no hay secuencia de referencia; se entrega
    // `None` y el consumidor resincroniza con `terminal.list` (§14.9).
    let (snapshot, seq) = match snapshot_from(params) {
        Some(snapshot) => (Some(snapshot), params.get("seq").and_then(Value::as_u64)),
        None => (None, None),
    };
    *min_seq = seq;
    TermEvent::Missed { snapshot, seq }
}

fn event_from_params(params: &Value, min_seq: &mut Option<u64>) -> Option<TermEvent> {
    if params.get("type").and_then(Value::as_str) == Some("missed") {
        return Some(missed_event(params, min_seq));
    }
    let notified_seq = params.get("seq").and_then(Value::as_u64);
    if let (Some(seq), Some(min)) = (notified_seq, min_seq.as_ref()) {
        if seq <= *min {
            tracing::debug!(seq, min, "ez-term: evento descartado tras missed");
            return None;
        }
    }
    let seq = notified_seq.unwrap_or(0);
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
            Ok(terminal) => TermEvent::Created { terminal, seq },
            Err(error) => {
                tracing::debug!(%error, "ez-term: created sin datos de terminal");
                TermEvent::Other {
                    terminal,
                    kind: kind.to_string(),
                    data,
                    seq,
                }
            }
        },
        "status" => match status_from(&data) {
            Some(status) => TermEvent::Status {
                terminal,
                status,
                seq,
            },
            None => TermEvent::Other {
                terminal,
                kind: kind.to_string(),
                data,
                seq,
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
            seq,
        },
        "removed" => TermEvent::Removed { terminal, seq },
        "lost" => TermEvent::Lost { terminal, seq },
        "restarted" => TermEvent::Restarted { terminal, seq },
        "heartbeat" => TermEvent::Heartbeat,
        other => TermEvent::Other {
            terminal,
            kind: other.to_string(),
            data,
            seq,
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
fn snapshot_from(params: &Value) -> Option<Vec<Terminal>> {
    let candidates = [
        params.get("terminals"),
        params.get("snapshot"),
        params.pointer("/data/terminals"),
    ];
    let list = candidates.into_iter().flatten().next()?;
    serde_json::from_value(list.clone()).ok()
}
