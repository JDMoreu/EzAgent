//! Eventos del árbitro: traza tipada (R12, diseño §14.8) y novedades para el
//! elefante (`ez wait`). Cada evento de traza lleva un `seq` monotónico.

use std::path::PathBuf;

use ez_team::Seat;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::verify::CheckRecord;

/// Tipo de tarjeta que espera una decisión de la persona (diseño §14.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Task,
    Team,
    Pause,
}

impl CardKind {
    /// Id del tipo (`task`, `team`, `pause`).
    pub fn id(self) -> &'static str {
        match self {
            CardKind::Task => "task",
            CardKind::Team => "team",
            CardKind::Pause => "pause",
        }
    }
}

/// Tarjeta abierta para la persona en la columna de traza (diseño §14.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub space: String,
    pub task: Option<String>,
    pub kind: CardKind,
    pub title: String,
    pub body: String,
}

/// Payload tipado de cada evento de traza (diseño §14.8).
///
/// Se serializa con `#[serde(tag = "type")]`: `event.kind.type` es el nombre
/// del evento y el resto de campos son su payload. Nunca incluye tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    TaskProposed {
        title: String,
        card: String,
    },
    TaskApproved {},
    TaskRejected {
        comment: String,
    },
    TeamProposed {
        card: String,
        seats: Vec<Seat>,
    },
    TeamApproved {},
    TeamRejected {
        comment: String,
    },
    RoundStarted {
        round: u32,
    },
    FishLaunched {
        fish: String,
        role: String,
        terminal: String,
        attempt: u32,
    },
    FishReported {
        fish: String,
        attempt: u32,
        verdict: String,
        file: PathBuf,
    },
    FishBlocked {
        fish: String,
        attempt: u32,
        kind: Option<String>,
    },
    /// Evento informativo de terminal terminada sin trabajo pendiente.
    FishExited {
        fish: String,
        attempt: u32,
    },
    FishFailed {
        fish: String,
        attempt: u32,
        reason: String,
    },
    /// Corrección entregada a un pez (no se elimina en el diseño).
    FishCorrected {
        fish: String,
        file: PathBuf,
        report: PathBuf,
    },
    Verification {
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        criterios: Vec<CheckRecord>,
    },
    RoundClosed {
        round: u32,
        clean: bool,
        reasons: Vec<String>,
    },
    Paused {
        reason: String,
        message: String,
    },
    Resumed {},
    TaskClosed {},
    TaskStopped {},
    Rejected {
        rule: String,
        message: String,
    },
    CardOpened(Card),
    CardClosed {
        id: String,
    },
}

impl EventKind {
    /// Construye el payload tipado a partir del tipo y los datos que emitía la
    /// traza anterior. Centraliza la conversión para no repetirla en cada
    /// productor de eventos; ningún dato contiene tokens.
    pub(crate) fn from_parts(kind: &str, data: &Value) -> Self {
        let text = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let number = |key: &str| data.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
        let records = |key: &str| -> Vec<CheckRecord> {
            serde_json::from_value(data.get(key).cloned().unwrap_or(Value::Null))
                .unwrap_or_default()
        };
        match kind {
            "task_proposed" => Self::TaskProposed {
                title: text("title"),
                card: text("card"),
            },
            "task_approved" => Self::TaskApproved {},
            "task_rejected" => Self::TaskRejected {
                comment: text("comment"),
            },
            "team_proposed" => Self::TeamProposed {
                card: text("card"),
                seats: serde_json::from_value(data.get("seats").cloned().unwrap_or(Value::Null))
                    .unwrap_or_default(),
            },
            "team_approved" => Self::TeamApproved {},
            "team_rejected" => Self::TeamRejected {
                comment: text("comment"),
            },
            "round_started" => Self::RoundStarted {
                round: number("round"),
            },
            "fish_launched" => Self::FishLaunched {
                fish: text("fish"),
                role: text("role"),
                terminal: text("terminal"),
                attempt: number("attempt"),
            },
            "fish_reported" => Self::FishReported {
                fish: text("fish"),
                attempt: number("attempt"),
                verdict: text("verdict"),
                file: PathBuf::from(text("file")),
            },
            "fish_blocked" => Self::FishBlocked {
                fish: text("fish"),
                attempt: number("attempt"),
                kind: data.get("kind").and_then(Value::as_str).map(str::to_string),
            },
            "fish_exited" => Self::FishExited {
                fish: text("fish"),
                attempt: number("attempt"),
            },
            "fish_failed" => Self::FishFailed {
                fish: text("fish"),
                attempt: number("attempt"),
                reason: text("reason"),
            },
            "fish_corrected" => Self::FishCorrected {
                fish: text("fish"),
                file: PathBuf::from(text("file")),
                report: PathBuf::from(text("report")),
            },
            "verification" => Self::Verification {
                ok: data.get("ok").and_then(Value::as_bool).unwrap_or(false),
                failed: data
                    .get("failed")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                total: data
                    .get("total")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                criterios: records("criterios"),
            },
            "round_closed" => Self::RoundClosed {
                round: number("round"),
                clean: data.get("clean").and_then(Value::as_bool).unwrap_or(false),
                reasons: serde_json::from_value(
                    data.get("reasons").cloned().unwrap_or(Value::Null),
                )
                .unwrap_or_default(),
            },
            "paused" => Self::Paused {
                reason: text("reason"),
                message: text("message"),
            },
            "resumed" => Self::Resumed {},
            "task_closed" => Self::TaskClosed {},
            "stopped" | "task_stopped" => Self::TaskStopped {},
            "rejected" => Self::Rejected {
                rule: text("rule"),
                message: text("message"),
            },
            other => Self::Rejected {
                rule: "R12".to_string(),
                message: format!("evento de traza desconocido: {other}"),
            },
        }
    }
}

/// Evento de traza (R12): una línea de la bitácora y un mensaje a la vista.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbiterEvent {
    /// Orden monotónico en el proceso; también se escribe en la bitácora.
    pub seq: u64,
    pub kind: EventKind,
    pub space: String,
    pub task: Option<String>,
    /// Segundos desde 1970, del reloj del árbitro.
    pub ts: f64,
}

/// Novedad que espera el elefante (`ez wait`). El mensaje va en español.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElephantEvent {
    pub kind: String,
    pub space: String,
    pub task: Option<String>,
    pub ts: f64,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

impl ElephantEvent {
    /// Evento con mensaje y datos.
    pub fn new(
        kind: impl Into<String>,
        space: impl Into<String>,
        ts: f64,
        message: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            kind: kind.into(),
            space: space.into(),
            task: None,
            ts,
            message: message.into(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Card, CardKind, EventKind};
    use serde_json::json;

    #[test]
    fn event_kind_is_tagged_and_payload_is_typed() {
        let event = EventKind::from_parts(
            "fish_launched",
            &json!({"fish": "qa-r1", "role": "qa", "terminal": "t2", "attempt": 1}),
        );
        let value = serde_json::to_value(&event).expect("serializar");
        assert_eq!(value["type"], "fish_launched");
        assert_eq!(value["fish"], "qa-r1");
        assert_eq!(value["attempt"], 1);
    }

    #[test]
    fn card_opened_flattens_the_card() {
        let card = Card {
            id: "c1".into(),
            space: "s1".into(),
            task: None,
            kind: CardKind::Task,
            title: "Tarea".into(),
            body: "Cuerpo".into(),
        };
        let value = serde_json::to_value(EventKind::CardOpened(card)).expect("serializar");
        assert_eq!(value["type"], "card_opened");
        assert_eq!(value["kind"], "task");
        assert_eq!(value["title"], "Tarea");
    }
}
