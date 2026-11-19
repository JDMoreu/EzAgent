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
    /// Construye el payload tipado a partir del tipo y los datos de la traza.
    /// Ningún dato contiene tokens; la conversión se reparte en tres grupos.
    pub(crate) fn from_parts(kind: &str, data: &Value) -> Self {
        match kind {
            "task_proposed" | "task_approved" | "task_rejected" | "team_proposed"
            | "team_approved" | "team_rejected" => Self::approval_event(kind, data),
            "round_started" | "fish_launched" | "fish_reported" | "fish_blocked"
            | "fish_exited" | "fish_failed" | "fish_corrected" => Self::fish_event(kind, data),
            _ => Self::round_event(kind, data),
        }
    }

    fn text(data: &Value, key: &str) -> String {
        data.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn number(data: &Value, key: &str) -> u32 {
        data.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
    }

    fn boolean(data: &Value, key: &str) -> bool {
        data.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    fn approval_event(kind: &str, data: &Value) -> Self {
        match kind {
            "task_proposed" => Self::TaskProposed {
                title: Self::text(data, "title"),
                card: Self::text(data, "card"),
            },
            "task_rejected" => Self::TaskRejected {
                comment: Self::text(data, "comment"),
            },
            "team_proposed" => Self::TeamProposed {
                card: Self::text(data, "card"),
                seats: serde_json::from_value(data.get("seats").cloned().unwrap_or(Value::Null))
                    .unwrap_or_default(),
            },
            "team_rejected" => Self::TeamRejected {
                comment: Self::text(data, "comment"),
            },
            "team_approved" => Self::TeamApproved {},
            _ => Self::TaskApproved {},
        }
    }

    fn fish_event(kind: &str, data: &Value) -> Self {
        let fish = Self::text(data, "fish");
        let attempt = Self::number(data, "attempt");
        match kind {
            "round_started" => Self::RoundStarted {
                round: Self::number(data, "round"),
            },
            "fish_launched" => Self::FishLaunched {
                fish,
                role: Self::text(data, "role"),
                terminal: Self::text(data, "terminal"),
                attempt,
            },
            "fish_reported" => Self::FishReported {
                fish,
                attempt,
                verdict: Self::text(data, "verdict"),
                file: PathBuf::from(Self::text(data, "file")),
            },
            "fish_blocked" => Self::FishBlocked {
                fish,
                attempt,
                kind: data.get("kind").and_then(Value::as_str).map(str::to_string),
            },
            "fish_failed" => Self::FishFailed {
                fish,
                attempt,
                reason: Self::text(data, "reason"),
            },
            "fish_corrected" => Self::FishCorrected {
                fish,
                file: PathBuf::from(Self::text(data, "file")),
                report: PathBuf::from(Self::text(data, "report")),
            },
            _ => Self::FishExited { fish, attempt },
        }
    }

    fn round_event(kind: &str, data: &Value) -> Self {
        match kind {
            "round_started" => Self::RoundStarted {
                round: Self::number(data, "round"),
            },
            "verification" => Self::Verification {
                ok: Self::boolean(data, "ok"),
                failed: data
                    .get("failed")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                total: data
                    .get("total")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                criterios: serde_json::from_value(
                    data.get("criterios").cloned().unwrap_or(Value::Null),
                )
                .unwrap_or_default(),
            },
            "round_closed" => Self::RoundClosed {
                round: Self::number(data, "round"),
                clean: Self::boolean(data, "clean"),
                reasons: serde_json::from_value(
                    data.get("reasons").cloned().unwrap_or(Value::Null),
                )
                .unwrap_or_default(),
            },
            "paused" => Self::Paused {
                reason: Self::text(data, "reason"),
                message: Self::text(data, "message"),
            },
            "rejected" => Self::Rejected {
                rule: Self::text(data, "rule"),
                message: Self::text(data, "message"),
            },
            "resumed" => Self::Resumed {},
            "task_closed" => Self::TaskClosed {},
            "stopped" | "task_stopped" => Self::TaskStopped {},
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
