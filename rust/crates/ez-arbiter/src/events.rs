//! Eventos del árbitro: traza para la vista (R12) y novedades para el elefante.

use serde::{Deserialize, Serialize};

/// Evento de traza (R12): una línea de la bitácora y un mensaje a la vista.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbiterEvent {
    /// Tipo (`task_proposed`, `fish_launched`, `round_closed`…).
    pub kind: String,
    pub space: String,
    pub task: Option<String>,
    /// Segundos desde 1970, del reloj del árbitro.
    pub ts: f64,
    /// Datos del caso; nunca incluye tokens.
    #[serde(default)]
    pub data: serde_json::Value,
}

impl ArbiterEvent {
    /// Evento con datos nulos.
    pub fn new(kind: impl Into<String>, space: impl Into<String>, ts: f64) -> Self {
        Self {
            kind: kind.into(),
            space: space.into(),
            task: None,
            ts,
            data: serde_json::Value::Null,
        }
    }

    /// Copia del evento sin secretos (lo que se escribe en la bitácora).
    // DECISION: el token viaja a la vista en fish_launched pero no se escribe
    // en la bitácora: las claves nunca se registran en disco.
    pub fn redacted(&self) -> Self {
        let mut copy = self.clone();
        if let Some(object) = copy.data.as_object_mut() {
            if object.contains_key("token") {
                object.insert("token".into(), serde_json::Value::from("***"));
            }
        }
        copy
    }
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
    pub data: serde_json::Value,
}

impl ElephantEvent {
    /// Evento con mensaje y datos.
    pub fn new(
        kind: impl Into<String>,
        space: impl Into<String>,
        ts: f64,
        message: impl Into<String>,
        data: serde_json::Value,
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
