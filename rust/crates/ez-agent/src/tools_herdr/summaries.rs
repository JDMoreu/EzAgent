//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! Resúmenes de confirmación generados por código (nunca por el LLM), nivel de
//! riesgo de cada acción y nombres visibles de los agentes.

use ez_core::config::{python_str, truthy};
use ez_herdr::herdr::{visible_name, Herdr};
use serde_json::{Map, Value};

use super::args::{action, delegate_level, shown, text_field};
use crate::orchestration::{describe_steps, parse_steps};
use crate::registry::{ToolInfo, ToolResult};

/// `_delegate_summary`.
pub fn delegate_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    match action(args).as_str() {
        "list" => "Listar los agentes vivos de Herdr.".to_string(),
        "get" => {
            if text_field(args, "address").is_empty() {
                "Consultar un agente de Herdr.".to_string()
            } else {
                format!(
                    "Consultar {}.",
                    shown(&text_field(args, "address"), label_for)
                )
            }
        }
        "read" => read_summary(args, label_for),
        "send" => send_summary(args, label_for),
        "agents" => "Consultar los agentes disponibles y sus modelos.".to_string(),
        "guide" => format!(
            "Leer el manual del agente {}.",
            if text_field(args, "agent").is_empty() {
                "indicado".to_string()
            } else {
                text_field(args, "agent")
            }
        ),
        "keys" => keys_summary(args),
        "configure" => configure_summary(args, label_for),
        "run" => run_summary(args, label_for),
        "open_terminal" => {
            let name = text_field(args, "name");
            if name.is_empty() {
                "Abrir una terminal en Herdr.".to_string()
            } else {
                format!("Abrir una terminal {name} en Herdr.")
            }
        }
        "focus" => named_summary(args, label_for, "Enfocar", "Enfocar un agente de Herdr."),
        "kill" => named_summary(args, label_for, "Terminar", "Terminar un agente de Herdr."),
        "watch" => {
            let address = text_field(args, "address");
            if address.is_empty() {
                "Vigilar un agente y avisar al terminar.".to_string()
            } else {
                format!(
                    "Vigilar {} y avisar al terminar.",
                    shown(&address, label_for)
                )
            }
        }
        _ => "Acción desconocida para Herdr.".to_string(),
    }
}

fn read_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    let address = text_field(args, "address");
    let lines = match args.get("lines") {
        Some(value @ Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Some(python_str(value))
        }
        _ => None,
    };
    if !address.is_empty() {
        if let Some(lines) = lines {
            return format!(
                "Leer las últimas {lines} líneas d{}.",
                shown(&address, label_for)
            );
        }
        return format!("Leer el buffer d{}.", shown(&address, label_for));
    }
    "Leer el buffer de un agente de Herdr.".to_string()
}

fn send_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    let address = text_field(args, "address");
    let task = text_field(args, "task");
    if !address.is_empty() {
        let target = shown(&address, label_for);
        if task.is_empty() {
            return format!("Enviar una tarea a {target}.");
        }
        return format!("Enviar a {target} la tarea: {task}");
    }
    "Enviar una tarea a un agente de Herdr.".to_string()
}

fn keys_summary(args: &Map<String, Value>) -> String {
    let pressed = match args.get("keys") {
        Some(Value::Array(items)) => items.iter().map(python_str).collect::<Vec<_>>().join(" "),
        _ => "teclas".to_string(),
    };
    // El Python reutiliza la variable `shown` y repite las teclas como destino;
    // se reproduce tal cual (ver informe).
    format!("Pulsar {pressed} en {pressed}.")
}

fn configure_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    let mut wanted = Vec::new();
    for (key, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
        let value = text_field(args, key);
        if !value.is_empty() {
            wanted.push(format!("{label} {value}"));
        }
    }
    let address = text_field(args, "address");
    let target = if address.is_empty() {
        "un agente".to_string()
    } else {
        shown(&address, label_for)
    };
    let changes = if wanted.is_empty() {
        "la configuración".to_string()
    } else {
        wanted.join(" y ")
    };
    format!("Cambiar {changes} de {target}.")
}

fn run_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    let agent = text_field(args, "agent");
    let task = text_field(args, "task");
    let target = if agent.is_empty() {
        "un agente".to_string()
    } else {
        agent
    };
    if task.is_empty() {
        return format!("Abrir el agente {target} en una pestaña visible.");
    }
    let where_text = if args.get("background") == Some(&Value::Bool(true)) {
        "en segundo plano"
    } else {
        "en una pestaña visible"
    };
    let suffix = if args.get("gui") == Some(&Value::Bool(true)) {
        " con interfaz gráfica"
    } else {
        ""
    };
    let mut details = Vec::new();
    for (key, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
        let value = text_field(args, key);
        if !value.is_empty() {
            details.push(format!("{label} {value}"));
        }
    }
    let input_from = text_field(args, "input_from");
    if !input_from.is_empty() {
        details.push(format!("a partir de {}", shown(&input_from, label_for)));
    }
    let mut extra = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    if let Ok(steps) = parse_steps(args.get("then").unwrap_or(&Value::Null)) {
        if !steps.is_empty() {
            extra.push_str(&format!("; después: {}", describe_steps(&steps)));
        }
    }
    format!("Lanzar el agente {target} {where_text}{suffix}{extra} con la tarea: {task}")
}

fn named_summary(
    args: &Map<String, Value>,
    label_for: &dyn Fn(&str) -> String,
    verb: &str,
    fallback: &str,
) -> String {
    let address = text_field(args, "address");
    if address.is_empty() {
        fallback.to_string()
    } else {
        format!("{verb} {}.", shown(&address, label_for))
    }
}

/// `_voice_summary`.
pub fn voice_summary(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> String {
    match action(args).as_str() {
        "close" => "Cerrar la escucha al terminar de hablar.".to_string(),
        "close_and_watch" => {
            let address = text_field(args, "address");
            if address.is_empty() {
                "Cerrar la escucha y avisar cuando termine el agente.".to_string()
            } else {
                format!(
                    "Cerrar la escucha y avisar cuando termine {}.",
                    shown(&address, label_for)
                )
            }
        }
        "mic_mute" => match args.get("muted") {
            Some(Value::Bool(true)) => "Silenciar el micrófono.".to_string(),
            Some(Value::Bool(false)) => "Reactivar el micrófono.".to_string(),
            _ => "Cambiar el estado del micrófono.".to_string(),
        },
        "repeat_last" => "Repetir el último resumen.".to_string(),
        "mute" => match args.get("minutes") {
            Some(value @ Value::Number(number)) if number.is_i64() || number.is_u64() => {
                format!(
                    "Silenciar los avisos durante {} minutos.",
                    python_str(value)
                )
            }
            _ => "Silenciar los avisos.".to_string(),
        },
        _ => "Acción desconocida de control por voz.".to_string(),
    }
}

/// `_delegate_info`.
pub fn delegate_info(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> ToolInfo {
    let action = action(args);
    let known = delegate_level(&action).is_some();
    ToolInfo {
        level: delegate_level(&action).unwrap_or(2),
        always_confirm: action == "kill" || !known,
        summary: delegate_summary(args, label_for),
        ..ToolInfo::default()
    }
}

/// `_voice_info`.
pub fn voice_info(args: &Map<String, Value>, label_for: &dyn Fn(&str) -> String) -> ToolInfo {
    ToolInfo {
        level: 1,
        summary: voice_summary(args, label_for),
        ..ToolInfo::default()
    }
}

/// Campos de Herdr con los que el modelo armaba nombres dobles («Claude Code en
/// EzAgent / Claude Code»): ya están resumidos en `label` y no le llegan sueltos.
const RAW_NAME_FIELDS: [&str; 4] = ["where", "name", "labels", "title"];

/// `_labelled`: `item` con su `label` (nombre visible) y sin los campos crudos.
pub fn labelled(herdr: &Herdr, item: &mut Map<String, Value>) {
    let address = item.get("address").is_some_and(truthy);
    let has_label = item.get("label").is_some_and(truthy);
    if address && !has_label {
        let key = python_str(item.get("address").unwrap_or(&Value::Null));
        let label = herdr.label_for(&key);
        let label = if label.is_empty() {
            visible_name(item)
        } else {
            label
        };
        item.insert("label".to_string(), Value::String(label));
    }
    if item.get("label").is_some_and(truthy) {
        for key in RAW_NAME_FIELDS {
            item.remove(key);
        }
    }
}

/// `_with_labels`: cada agente del resultado lleva su `label` y solo ese nombre.
pub fn with_labels(herdr: &Herdr, mut result: ToolResult) -> ToolResult {
    match &mut result.data {
        Value::Object(item) => labelled(herdr, item),
        Value::Array(items) => {
            for item in items {
                if let Value::Object(map) = item {
                    labelled(herdr, map);
                }
            }
        }
        _ => {}
    }
    result
}

#[cfg(test)]
mod tests;
