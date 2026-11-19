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

/// Un nodo del árbol de la vista: agente principal, subagente o terminal «sin papel».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeNode {
    /// Id del agente principal o del subagente.
    pub id: String,
    /// Nombre para mostrar.
    pub name: String,
    /// Nombre del papel, si es un subagente con papel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Id del papel, si es un subagente con papel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_id: Option<String>,
    /// Título de la tarea, si es un agente principal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub agent: String,
    pub model: String,
    /// Estado de la terminal en herdr-core.
    pub status: String,
    /// Dirección hc, si la terminal está viva.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub children: Vec<TreeNode>,
}

/// Árbol de un espacio: agentes principales con sus subagentes anidados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpaceTree {
    pub nodes: Vec<TreeNode>,
}

/// Datos de un agente principal recién creado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MainAgentInfo {
    pub id: String,
    pub name: String,
    pub worktree: String,
    pub branch: String,
    pub base_branch: String,
    pub address: String,
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
