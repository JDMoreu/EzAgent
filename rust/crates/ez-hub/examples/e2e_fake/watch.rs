//! Observación de tarjetas y eventos del árbitro durante el ejemplo.

use std::collections::BTreeSet;
use std::path::Path;

use ez_hub::runtime::HubRuntime;
use serde_json::Value;

/// Tarjeta pendiente de decisión de la persona.
#[derive(Debug, Clone)]
pub struct Card {
    /// Id que usa `card_action`.
    pub id: String,
    /// `task`, `team`, `pause` o `integrate`.
    pub kind: String,
}

/// Estado leído de la traza del agente principal.
#[derive(Debug, Clone)]
pub struct Seen {
    /// Tarjetas abiertas ahora mismo.
    pub cards: Vec<Card>,
    /// Tipos de evento de traza vistos hasta ahora.
    pub kinds: BTreeSet<String>,
    /// Últimos eventos, para el informe de fallo.
    pub tail: Vec<Value>,
}

/// Lee la traza del agente principal sin esperas largas ni candados retenidos.
pub fn read(runtime: &HubRuntime, space: &str, elephant: &str) -> Result<Seen, String> {
    let state = runtime.hub_state(space, elephant)?;
    let cards = state["tarjetas"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let id = value["id"].as_str()?;
                    let kind = value["kind"].as_str().unwrap_or("").to_string();
                    Some(Card {
                        id: id.to_string(),
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut kinds = BTreeSet::new();
    let mut tail = Vec::new();
    if let Some(events) = state["eventos"].as_array() {
        for event in events {
            let kind = event["kind"]["type"].as_str().unwrap_or("").to_string();
            kinds.insert(kind);
            tail.push(event.clone());
        }
    }
    if tail.len() > 30 {
        tail.drain(..tail.len() - 30);
    }
    Ok(Seen { cards, kinds, tail })
}

/// Comprueba la bitácora y el estado en disco: dos rondas limpias y sin tokens.
pub fn on_disk(data: &Path, token: &str) -> Result<DiskCheck, String> {
    let tasks = data.join("tareas");
    let entries = std::fs::read_dir(&tasks)
        .map_err(|error| format!("no pude leer {}: {error}", tasks.display()))?;
    let mut folder = None;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            folder = Some(path);
            break;
        }
    }
    let folder = folder.ok_or_else(|| "no hay tareas en datos".to_string())?;
    let log = std::fs::read_to_string(folder.join("bitacora.jsonl"))
        .map_err(|error| format!("no pude leer la bitácora: {error}"))?;
    let state = std::fs::read_to_string(folder.join("estado.json"))
        .map_err(|error| format!("no pude leer estado.json: {error}"))?;
    let mut clean = 0;
    for line in log.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["kind"]["type"].as_str() == Some("round_closed")
            && event["kind"]["clean"].as_bool() == Some(true)
        {
            clean += 1;
        }
    }
    let leaked = log.contains(token)
        || state.contains(token)
        || log.contains("\"token\"")
        || state.contains("\"token\"");
    Ok(DiskCheck {
        clean_rounds: clean,
        leaked_token: leaked,
    })
}

/// Resultado de la comprobación en disco.
#[derive(Debug, Clone)]
pub struct DiskCheck {
    /// Rondas cerradas limpias en la bitácora.
    pub clean_rounds: usize,
    /// Aparece algún token donde no debe.
    pub leaked_token: bool,
}
