//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! `voice_control`: cerrar la escucha, silenciar el micrófono, repetir el
//! último resumen y silenciar los avisos. No habla con Herdr.

use serde_json::{json, Map, Value};

use super::args::{
    checked_action, last_address, optional_text, set_last_address, user_text, wants_notify,
    ArgumentError, DelegateError, HerdrCallbacks, SharedState, VOICE_ACTIONS,
};
use crate::registry::ToolResult;

/// `voice_control`, con los errores ya convertidos a `ToolResult`.
pub fn handle_voice(
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> ToolResult {
    match voice_action(callbacks, state, args) {
        Ok(result) => result,
        Err(error) => error.into_result(),
    }
}

fn voice_action(
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let action = checked_action(args, &VOICE_ACTIONS, "voice_control")?;
    match action.as_str() {
        "close" => Ok(close_action(callbacks, state)),
        "close_and_watch" => close_and_watch_action(callbacks, state, args, &action),
        "mic_mute" => mic_mute_action(callbacks, args),
        "repeat_last" => Ok(ToolResult::ok(json!({"text": (callbacks.last_summary)()}))),
        _ => mute_action(callbacks, args),
    }
}

fn close_action(callbacks: &HerdrCallbacks, state: &SharedState) -> ToolResult {
    let text = user_text(callbacks);
    let watched = if wants_notify(&text) {
        last_address(state)
    } else {
        None
    };
    if let Some(address) = &watched {
        (callbacks.remember_task)(address, "", &text);
        (callbacks.watch)(address);
    }
    (callbacks.request_close)();
    let watching = watched.map_or(Value::Null, Value::String);
    ToolResult::ok(json!({
        "closed": true,
        "watching": watching,
        "message": "Cerraré la escucha al terminar de hablar.",
    }))
}

fn close_and_watch_action(
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = optional_text(args, "address", action)?.or_else(|| last_address(state));
    let Some(address) = address else {
        // DECISION: sin agente no se cierra, porque el usuario pidió
        // expresamente que se le avise al terminar.
        return Ok(ToolResult::err(
            "No hay ningún agente al que vigilar. Indica la \
             dirección del agente con «address».",
        ));
    };
    set_last_address(state, Some(address.clone()));
    (callbacks.remember_task)(&address, "", &user_text(callbacks));
    (callbacks.watch)(&address);
    (callbacks.request_close)();
    Ok(ToolResult::ok(json!({
        "closed": true,
        "address": address,
        "watching": true,
        "message": "Cerraré la escucha y te avisaré cuando termine.",
    })))
}

fn mic_mute_action(
    callbacks: &HerdrCallbacks,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let Some(Value::Bool(muted)) = args.get("muted") else {
        return Err(ArgumentError::new(
            "Para la acción «mic_mute» falta el parámetro booleano «muted».",
        )
        .into());
    };
    (callbacks.mic_mute)(*muted);
    let message = if *muted {
        "Micrófono silenciado."
    } else {
        "Micrófono activo."
    };
    Ok(ToolResult::ok(json!({"muted": muted, "message": message})))
}

fn mute_action(
    callbacks: &HerdrCallbacks,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let minutes = match args.get("minutes") {
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Some(number.as_i64().unwrap_or(i64::MAX))
        }
        _ => None,
    };
    let Some(minutes) = minutes else {
        return Err(ArgumentError::new(
            "Para la acción «mute» falta el parámetro entero «minutes».",
        )
        .into());
    };
    if !(1..=480).contains(&minutes) {
        return Err(ArgumentError::new("El parámetro «minutes» debe estar entre 1 y 480.").into());
    }
    (callbacks.mute)(minutes);
    Ok(ToolResult::ok(json!({
        "minutes": minutes,
        "message": format!("Silenciado {minutes} minutos."),
    })))
}

#[cfg(test)]
mod tests;
