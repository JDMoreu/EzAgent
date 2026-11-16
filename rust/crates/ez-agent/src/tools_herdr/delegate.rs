//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! `delegate_to_agent`: consulta, delegación, encadenado y vigilancia. Las
//! llamadas a Herdr tardan decenas de segundos; este módulo no abre ventanas ni
//! lanza procesos por su cuenta: todo pasa por el adaptador `Herdr`.

use std::sync::Arc;

use ez_core::config::python_str;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::Herdr;
use ez_herdr::herdr::{NameKind, DEFAULT_SHELL};
use serde_json::{json, Map, Value};

use super::args::{
    checked_action, last_address, optional_lines, optional_text, required_text, set_last_address,
    user_text, wrapped, ArgumentError, DelegateError, HerdrCallbacks, SharedState,
    DELEGATE_ACTIONS,
};
use super::names::Names;
use crate::orchestration::load_guide;
use crate::registry::ToolResult;

use super::port::HerdrPort;

mod run;
use run::handle_run;
pub use run::{handle_run_with_ids, read_output};

/// `Herdr` + perfiles de agente: el Python hereda `AgentDriver` dentro de
/// `Herdr`; aquí se construye con la configuración viva en cada uso (F4-11).
pub(super) fn driver(herdr: &Arc<Herdr>) -> AgentDriver<Arc<Herdr>> {
    AgentDriver::new(Arc::clone(herdr), herdr.config())
}

/// `delegate_to_agent`, con los errores ya convertidos a `ToolResult`.
pub fn handle_delegate(
    herdr: &dyn HerdrPort,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> ToolResult {
    let names = Names::new(herdr, callbacks.confirm_name.as_ref());
    let mut result = match delegate_action(herdr, &names, callbacks, state, args) {
        Ok(result) => result,
        Err(error) => error.into_result(),
    };
    names.attach(&mut result);
    result
}

fn delegate_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let action = checked_action(args, &DELEGATE_ACTIONS, "delegate_to_agent")?;
    match action.as_str() {
        "list" => {
            let agents = herdr.list_agents()?;
            Ok(ToolResult::ok(Value::Array(
                agents.into_iter().map(Value::Object).collect(),
            )))
        }
        "get" => {
            let address = pick_address(names, args, &action)?;
            Ok(wrapped(herdr.get_agent(&address)?))
        }
        "read" => read_action(herdr, names, args, &action),
        "send" => {
            let address = pick_address(names, args, &action)?;
            let task = required_text(args, "task", &action)?;
            // DECISION: la tarea llega tal cual al agente, sin preámbulo.
            Ok(wrapped(herdr.send_agent(&address, &task, true)?))
        }
        "run" => handle_run(herdr, names, callbacks, state, args),
        "open_terminal" => open_terminal_action(herdr, names, args, &action),
        "agents" => {
            let catalog = herdr.agents_catalog();
            Ok(ToolResult::ok(Value::Array(
                catalog.into_iter().map(Value::Object).collect(),
            )))
        }
        "guide" => guide_action(herdr, names, args, &action),
        "keys" => keys_action(herdr, names, args, &action),
        "configure" => configure_action(herdr, names, args, &action),
        "focus" => {
            let address = pick_address(names, args, &action)?;
            Ok(wrapped(herdr.focus_agent(&address)?))
        }
        "kill" => kill_action(herdr, names, state, args, &action),
        _ => watch_action(names, callbacks, state, args, &action),
    }
}

/// `address` resuelto contra las terminales abiertas.
fn pick_address(
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<String, DelegateError> {
    let spoken = required_text(args, "address", action)?;
    Ok(names.pick(NameKind::Terminal, &spoken)?)
}

fn read_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = pick_address(names, args, action)?;
    let lines = optional_lines(args, action)?;
    let text = herdr.read_agent(&address, lines)?;
    Ok(ToolResult::ok(json!({"address": address, "text": text})))
}

fn open_terminal_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let name = required_text(args, "name", action)?;
    let shell = optional_text(args, "shell", action)?.unwrap_or_else(|| DEFAULT_SHELL.to_string());
    let cwd = optional_text(args, "cwd", action)?;
    let space = pick_space(names, args, action)?;
    Ok(wrapped(herdr.open_terminal(
        &name,
        cwd.as_deref(),
        &shell,
        space.as_deref(),
    )?))
}

/// `space` opcional resuelto contra los espacios abiertos.
fn pick_space(
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<Option<String>, DelegateError> {
    match optional_text(args, "space", action)? {
        Some(spoken) => Ok(Some(names.pick(NameKind::Space, &spoken)?)),
        None => Ok(None),
    }
}

fn guide_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let spoken = required_text(args, "agent", action)?;
    let wanted = names.pick(NameKind::Agent, &spoken)?;
    let key = herdr
        .resolve_agent(&wanted)
        .unwrap_or_else(|| wanted.to_lowercase());
    let Some(text) = load_guide(&key) else {
        return Err(ArgumentError::new(format!(
            "No hay manual para {}. Usa action=agents.",
            ez_core::pyvalue::repr_str(&wanted)
        ))
        .into());
    };
    // Python lee los manuales en modo texto: CRLF se convierte en LF.
    let guide = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok(ToolResult::ok(json!({"agent": key, "guide": guide})))
}

fn keys_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let pressed = match args.get("keys") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        _ => {
            return Err(ArgumentError::new(
                "Para la acción «keys» indica «keys» (lista de teclas).",
            )
            .into())
        }
    };
    let keys: Vec<String> = pressed.iter().map(python_str).collect();
    let address = pick_address(names, args, action)?;
    Ok(wrapped(herdr.keys_agent(&address, &keys)?))
}

fn configure_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let model = optional_text(args, "model", action)?;
    let effort = optional_text(args, "effort", action)?;
    if model.is_none() && effort.is_none() {
        return Err(
            ArgumentError::new("Para la acción «configure» indica «model» o «effort».").into(),
        );
    }
    let address = pick_address(names, args, action)?;
    Ok(wrapped(herdr.configure_agent(
        &address,
        model.as_deref(),
        effort.as_deref(),
    )))
}

fn kill_action(
    herdr: &dyn HerdrPort,
    names: &Names<'_>,
    state: &SharedState,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = pick_address(names, args, action)?;
    let result = wrapped(herdr.kill_agent(&address, true)?);
    // DECISION: si se mata el último agente, se olvida para no vigilar un
    // terminal que ya no existe.
    if result.ok && last_address(state).as_deref() == Some(address.as_str()) {
        set_last_address(state, None);
    }
    Ok(result)
}

fn watch_action(
    names: &Names<'_>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = pick_address(names, args, action)?;
    set_last_address(state, Some(address.clone()));
    (callbacks.remember_task)(&address, "", &user_text(callbacks));
    (callbacks.watch)(&address);
    Ok(ToolResult::ok(json!({
        "address": address,
        "watching": true,
        "message": "Avisaré cuando el agente termine o se bloquee.",
    })))
}

#[cfg(test)]
mod tests;
