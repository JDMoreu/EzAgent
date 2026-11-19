//! Comandos Tauri del motor: agentes principales, árbol, traza, tarjetas e
//! inventario (diseño §15.6 y brief ECO-6c). Los registra `lib.rs`.

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::commands::AppState;
use crate::demo;
use crate::runtime::spaces::MainChoice;
use crate::runtime::{MainAgentInfo, SpaceTree};

const NO_RUNTIME: &str = "El motor del espacio de trabajo no está disponible.";

impl AppState {
    /// Dirección hc del agente principal por omisión (lo lanza si hace falta).
    pub fn space_elephant(&self, space: &str) -> Result<String, String> {
        let id = ez_arbiter::ElephantKey::default_for(space).elephant;
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .ensure_main_agent(space, &id)
    }

    /// Crea un agente principal con worktree y lo lanza.
    pub fn main_agent_create(
        &self,
        space: &str,
        name: &str,
        choice: &MainChoice,
    ) -> Result<MainAgentInfo, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .main_agent_create(space, name, choice)
    }

    /// Quita un agente principal y su worktree.
    pub fn main_agent_remove(&self, space: &str, elephant: &str) -> Result<(), String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .main_agent_remove(space, elephant)
    }

    /// Árbol de agentes principales y subagentes de un espacio. En demostración
    /// se devuelve el árbol de ejemplo, sin herdr-core.
    pub fn space_tree(&self, space: &str) -> Result<SpaceTree, String> {
        if self.demo {
            return Ok(demo::tree());
        }
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .space_tree(space)
    }

    /// Dirección hc de un nodo del árbol (lanza el principal si hace falta).
    pub fn node_address(&self, space: &str, node: &str) -> Result<String, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .node_address(space, node)
    }

    /// Envía texto crudo a una dirección hc.
    pub fn agent_send(&self, address: &str, text: &str) -> Result<(), String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| NO_RUNTIME.to_string())?
            .agent_send(address, text)
    }

    /// Traza y tarjetas del agente principal. En demostración devuelve los seis
    /// eventos y la tarjeta de ejemplo con la forma del motor: `{eventos,
    /// tarjetas, tarea}`.
    pub fn hub_state_for(&self, space: &str, elephant: Option<&str>) -> Value {
        if self.demo {
            return demo::trace();
        }
        let id = elephant
            .map(str::to_owned)
            .unwrap_or_else(|| ez_arbiter::ElephantKey::default_for(space).elephant);
        match &self.runtime {
            Some(runtime) => runtime
                .hub_state(space, &id)
                .unwrap_or_else(|error| json!({ "error": error })),
            None => json!({ "error": NO_RUNTIME }),
        }
    }

    /// Resuelve una tarjeta: en demostración usa la memoria; si no, el árbitro.
    pub fn card_action_for(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Value, String> {
        if !self.demo {
            let runtime = self
                .runtime
                .as_ref()
                .ok_or_else(|| NO_RUNTIME.to_string())?;
            return runtime.card_action(card, action, comment);
        }
        self.card_action(card, action, comment)?;
        Ok(json!({}))
    }

    /// Lista de papeles disponibles (los mismos datos que `ez roles`).
    pub fn roles_list(&self, space: Option<&str>, elephant: Option<&str>) -> Value {
        let Some(runtime) = self.runtime.as_ref() else {
            return Value::String(String::new());
        };
        let space = space
            .map(str::to_owned)
            .or_else(|| self.spaces().first().map(|space| space.id.clone()))
            .unwrap_or_default();
        let id = elephant
            .map(str::to_owned)
            .unwrap_or_else(|| ez_arbiter::ElephantKey::default_for(&space).elephant);
        runtime
            .roles_list(&space, &id)
            .unwrap_or_else(|_| Value::String(String::new()))
    }

    /// Inventario de agentes (los mismos datos que `ez inventory`).
    pub fn inventory(&self) -> Value {
        match &self.runtime {
            Some(runtime) => runtime
                .inventory()
                .unwrap_or_else(|_| json!({ "message": "", "data": [] })),
            None => json!({ "message": "", "data": [] }),
        }
    }
}

/// Traza y tarjetas de la tarea del agente principal.
#[tauri::command]
pub fn hub_state(state: State<'_, AppState>, space: String, elephant: Option<String>) -> Value {
    state.hub_state_for(&space, elephant.as_deref())
}

/// Resuelve una tarjeta; `merge` y `keep` integran o dejan la rama del principal.
#[tauri::command]
pub fn card_action(
    app: AppHandle,
    state: State<'_, AppState>,
    card: String,
    action: String,
    comment: Option<String>,
) -> Result<Value, String> {
    let result = state.card_action_for(&card, &action, comment.as_deref())?;
    if let Err(error) = app.emit("hub://changed", ()) {
        tracing::warn!(%error, "no pude avisar a la vista");
    }
    Ok(result)
}

/// Crea un agente principal con su worktree y lo lanza (§15.1).
#[tauri::command]
pub fn main_agent_create(
    state: State<'_, AppState>,
    space: String,
    nombre: String,
    agente: String,
    modelo: String,
    esfuerzo: Option<String>,
) -> Result<MainAgentInfo, String> {
    let choice = MainChoice {
        agent: agente,
        model: modelo,
        effort: esfuerzo,
    };
    state.main_agent_create(&space, &nombre, &choice)
}

/// Quita un agente principal y su worktree.
#[tauri::command]
pub fn main_agent_remove(
    state: State<'_, AppState>,
    space: String,
    elephant: String,
) -> Result<(), String> {
    state.main_agent_remove(&space, &elephant)
}

/// Árbol de agentes principales y subagentes de un espacio.
#[tauri::command]
pub fn space_tree(state: State<'_, AppState>, space: String) -> Result<SpaceTree, String> {
    state.space_tree(&space)
}

/// Dirección hc de un nodo; lanza el agente principal si hace falta.
#[tauri::command]
pub fn node_address(
    state: State<'_, AppState>,
    space: String,
    node: String,
) -> Result<String, String> {
    state.node_address(&space, &node)
}

/// Envía el texto de la barra a la terminal activa, principal o subagente.
#[tauri::command]
pub fn agent_send(state: State<'_, AppState>, address: String, text: String) -> Result<(), String> {
    state.agent_send(&address, &text)
}

/// Papeles disponibles: los mismos datos que `ez roles`.
#[tauri::command]
pub fn roles_list(
    state: State<'_, AppState>,
    space: Option<String>,
    elephant: Option<String>,
) -> Value {
    state.roles_list(space.as_deref(), elephant.as_deref())
}

/// Inventario de agentes: los mismos datos que `ez inventory`.
#[tauri::command]
pub fn inventory(state: State<'_, AppState>) -> Value {
    state.inventory()
}
