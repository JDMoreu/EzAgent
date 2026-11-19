//! El registro del estado de un subagente (§13 del diseño).
//!
//! `ez delegate` guarda un registro por subagente para que el padre (o el
//! vigilante) sepa en qué está: `en_proceso` mientras trabaja, `terminado`,
//! `fallido` o `validado` después.

use ez_core::pyvalue::truthy;
use serde_json::{Map, Value};

use crate::driver::Choice;
use crate::subagents::{one_line, Change, DelegateState, Record, TaskState};

use super::DelegateRequest;

/// Quién escribe el cambio de estado en el historial.
pub const BY_DELEGATE: &str = "ez delegate";

/// El registro del subagente recién lanzado.
///
/// `en_proceso` si el encargo se entregó (o si no se pudo confirmar);
/// `fallido` con el motivo si el encargo no llegó.
pub fn build_record(
    request: &DelegateRequest,
    choice: &Choice,
    result: &Map<String, Value>,
) -> Record {
    let task_sent = result.get("task_sent").is_some_and(truthy);
    // DECISION: `task_sent` en falso significa que el encargo no llegó; si
    // no viene la clave, no se puede confirmar y se cuenta como entregado.
    let (state, detail) = if task_sent {
        (TaskState::InProgress, "encargo entregado".to_string())
    } else {
        (
            TaskState::Failed,
            "no se pudo entregar el encargo al subagente".to_string(),
        )
    };
    let now = crate::subagents::epoch_ms();
    let terminal = text(result, "terminal");
    let name = text(result, "name");
    let parent = text(result, "parent");
    let parent = if parent.is_empty() {
        request.parent.clone()
    } else {
        parent
    };
    let cwd = {
        let folder = text(result, "cwd");
        if folder.is_empty() {
            request.cwd.clone().unwrap_or_default()
        } else {
            folder
        }
    };
    Record {
        name,
        terminal,
        parent,
        agent: choice.key.clone(),
        label: choice.label.clone(),
        model: choice.model.clone(),
        effort: choice.effort.clone(),
        role: request.role.clone(),
        cwd,
        brief_head: one_line(&request.brief, 200),
        state,
        since_ms: now,
        history: vec![Change {
            state,
            at_ms: now,
            by: BY_DELEGATE.to_string(),
            detail,
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// Guarda el registro del subagente recién lanzado.
pub fn register(
    state: &DelegateState,
    request: &DelegateRequest,
    choice: &Choice,
    result: &Map<String, Value>,
) -> Result<Record, String> {
    let record = build_record(request, choice, result);
    state
        .registry
        .create(&record)
        .map_err(|error| error.message().to_string())?;
    Ok(record)
}

/// Un texto del resultado del lanzamiento.
fn text(result: &Map<String, Value>, key: &str) -> String {
    match result.get(key) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}
