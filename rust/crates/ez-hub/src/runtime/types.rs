//! Tipos de datos que expone el motor a la vista (diseño §7 y §10).

use serde::Serialize;

/// Estado del motor del espacio de trabajo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeStatus {
    /// `normal`, `degradado` o `demo`.
    pub mode: String,
    /// Motivo cuando el motor está degradado; vacío en modo normal.
    pub reason: String,
    /// Dirección HTTP del servidor `ez`, si está encendido.
    pub url: String,
}

impl RuntimeStatus {
    /// Estado normal con la dirección del servidor.
    pub fn normal(url: impl Into<String>) -> Self {
        Self {
            mode: "normal".to_string(),
            reason: String::new(),
            url: url.into(),
        }
    }

    /// Estado degradado con el motivo que se muestra a la persona.
    pub fn degraded(reason: impl Into<String>) -> Self {
        Self {
            mode: "degradado".to_string(),
            reason: reason.into(),
            url: String::new(),
        }
    }

    /// Estado del modo demostración (sin motor).
    pub fn demo() -> Self {
        Self {
            mode: "demo".to_string(),
            reason: String::new(),
            url: String::new(),
        }
    }
}

/// Un pez vivo de un espacio, tal como lo ve la ventana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FishInfo {
    /// Dirección de la terminal en herdr-core.
    pub address: String,
    /// Identificador del pez (`dev-r1-1`, `qa-r1`…).
    pub fish: String,
    /// Papel del pez (`planner`, `developer`, `qa`…).
    pub role: String,
    /// Agente configurado para el pez.
    pub agent: String,
    /// Modelo configurado para el pez.
    pub model: String,
    /// Estado del agente (`idle`, `working`, `done`, `blocked`, `error`).
    pub status: String,
}
