//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! `delegate_to_agent`: consulta, delegación, encadenado y vigilancia. Las
//! llamadas a Herdr tardan decenas de segundos; este módulo no abre ventanas ni
//! lanza procesos por su cuenta: todo pasa por el adaptador `Herdr`.

use std::sync::Arc;

use ez_core::config::{python_str, truthy};
use ez_core::pyvalue::{py_strip, text_or_empty};
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::Herdr;
use ez_herdr::herdr::{RunAgent, DEFAULT_SHELL};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::args::{
    checked_action, last_address, optional_bool, optional_lines, optional_text, required_text,
    set_last_address, text_field, user_text, wants_close, wants_deliver, wants_notify, wrapped,
    ArgumentError, DelegateError, HerdrCallbacks, SharedState, DELEGATE_ACTIONS,
};
use crate::handoff::{answer_or_tail, compose_handoff};
use crate::handoff_packet::{build_packet, packet_text};
use crate::orchestration::{chain_spec, load_guide, parse_steps, ChainOptions, Step};
use crate::registry::ToolResult;

/// `Herdr` + perfiles de agente: el Python hereda `AgentDriver` dentro de
/// `Herdr`; aquí se construye con la configuración viva en cada uso (F4-11).
fn driver(herdr: &Arc<Herdr>) -> AgentDriver<Arc<Herdr>> {
    AgentDriver::new(Arc::clone(herdr), herdr.config())
}

/// `delegate_to_agent`, con los errores ya convertidos a `ToolResult`.
pub fn handle_delegate(
    herdr: &Arc<Herdr>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> ToolResult {
    match delegate_action(herdr, callbacks, state, args) {
        Ok(result) => result,
        Err(error) => error.into_result(),
    }
}

fn delegate_action(
    herdr: &Arc<Herdr>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let action = checked_action(args, &DELEGATE_ACTIONS, "delegate_to_agent")?;
    match action.as_str() {
        "list" => {
            let agents = herdr.list_agents(20.0)?;
            Ok(ToolResult::ok(Value::Array(
                agents.into_iter().map(Value::Object).collect(),
            )))
        }
        "get" => {
            let address = required_text(args, "address", &action)?;
            Ok(wrapped(herdr.get_agent(&address)?))
        }
        "read" => read_action(herdr, args, &action),
        "send" => {
            let address = required_text(args, "address", &action)?;
            let task = required_text(args, "task", &action)?;
            // DECISION: la tarea llega tal cual al agente, sin preámbulo.
            Ok(wrapped(herdr.send_agent(&address, &task, true)?))
        }
        "run" => handle_run(herdr, callbacks, state, args),
        "open_terminal" => open_terminal_action(herdr, args, &action),
        "agents" => {
            let catalog = driver(herdr).agents_catalog();
            Ok(ToolResult::ok(Value::Array(
                catalog.into_iter().map(Value::Object).collect(),
            )))
        }
        "guide" => guide_action(herdr, args, &action),
        "keys" => keys_action(herdr, args, &action),
        "configure" => configure_action(herdr, args, &action),
        "focus" => {
            let address = required_text(args, "address", &action)?;
            Ok(wrapped(herdr.focus_agent(&address)?))
        }
        "kill" => kill_action(herdr, state, args, &action),
        _ => watch_action(callbacks, state, args, &action),
    }
}

fn read_action(
    herdr: &Herdr,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = required_text(args, "address", action)?;
    let lines = optional_lines(args, action)?;
    let text = herdr.read_agent(&address, lines, 6000)?;
    Ok(ToolResult::ok(json!({"address": address, "text": text})))
}

fn open_terminal_action(
    herdr: &Herdr,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let name = required_text(args, "name", action)?;
    let shell = optional_text(args, "shell", action)?.unwrap_or_else(|| DEFAULT_SHELL.to_string());
    let cwd = optional_text(args, "cwd", action)?;
    let space = optional_text(args, "space", action)?;
    Ok(wrapped(herdr.open_terminal(
        &name,
        cwd.as_deref(),
        &shell,
        space.as_deref(),
    )?))
}

fn guide_action(
    herdr: &Herdr,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let wanted = required_text(args, "agent", action)?;
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
    Ok(ToolResult::ok(json!({"agent": key, "guide": text})))
}

fn keys_action(
    herdr: &Herdr,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = required_text(args, "address", action)?;
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
    Ok(wrapped(herdr.keys_agent(&address, &keys)?))
}

fn configure_action(
    herdr: &Arc<Herdr>,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = required_text(args, "address", action)?;
    let model = optional_text(args, "model", action)?;
    let effort = optional_text(args, "effort", action)?;
    if model.is_none() && effort.is_none() {
        return Err(
            ArgumentError::new("Para la acción «configure» indica «model» o «effort».").into(),
        );
    }
    Ok(wrapped(driver(herdr).configure_agent(
        &address,
        model.as_deref(),
        effort.as_deref(),
    )))
}

fn kill_action(
    herdr: &Herdr,
    state: &SharedState,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = required_text(args, "address", action)?;
    let result = wrapped(herdr.kill_agent(&address, true)?);
    // DECISION: si se mata el último agente, se olvida para no vigilar un
    // terminal que ya no existe.
    if result.ok && last_address(state).as_deref() == Some(address.as_str()) {
        set_last_address(state, None);
    }
    Ok(result)
}

fn watch_action(
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
    action: &str,
) -> Result<ToolResult, DelegateError> {
    let address = required_text(args, "address", action)?;
    set_last_address(state, Some(address.clone()));
    (callbacks.remember_task)(&address, "", &user_text(callbacks));
    (callbacks.watch)(&address);
    Ok(ToolResult::ok(json!({
        "address": address,
        "watching": true,
        "message": "Avisaré cuando el agente termine o se bloquee.",
    })))
}

/// Argumentos ya validados de `run`.
struct RunArgs {
    agent: String,
    task: String,
    steps: Vec<Step>,
    background: bool,
    gui: bool,
    close: bool,
    notify: bool,
    deliver: bool,
    cwd: Option<String>,
    name: Option<String>,
    space: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    input_from: Option<String>,
}

fn parse_run_args(
    herdr: &Herdr,
    callbacks: &HerdrCallbacks,
    args: &Map<String, Value>,
) -> Result<RunArgs, DelegateError> {
    let action = "run";
    let agent = required_text(args, "agent", action)?;
    // DECISION: la tarea es opcional; sin ella solo se abre el agente.
    let task = optional_text(args, "task", action)?.unwrap_or_default();
    let steps =
        parse_steps(args.get("then").unwrap_or(&Value::Null)).map_err(ArgumentError::new)?;
    let mut wanted = vec![agent.as_str()];
    wanted.extend(
        steps
            .iter()
            .filter_map(|step| step.get("agent").map(String::as_str)),
    );
    for name in wanted {
        if herdr.resolve_agent(name).is_none() {
            return Err(ArgumentError::new(format!(
                "Agente desconocido: {}. Usa action=agents para ver los disponibles.",
                ez_core::pyvalue::repr_str(name)
            ))
            .into());
        }
    }
    let background = optional_bool(args, "background", action)?;
    let gui = optional_bool(args, "gui", action)?;
    // DECISION: avisar es lo normal. Sin aviso, una tarea larga terminaba en
    // silencio y el usuario se enteraba solo al preguntar.
    let notify_on_done = match args.get("notify_on_done") {
        None | Some(Value::Null) => true,
        Some(_) => optional_bool(args, "notify_on_done", action)?,
    };
    let close_listening = optional_bool(args, "close_listening", action)?;
    let cwd = optional_text(args, "cwd", action)?;
    let name = optional_text(args, "name", action)?;
    let space = optional_text(args, "space", action)?;
    let model = optional_text(args, "model", action)?;
    let effort = optional_text(args, "effort", action)?;
    let input_from = optional_text(args, "input_from", action)?;
    let text = user_text(callbacks);
    let deliver = optional_bool(args, "deliver_answer", action)?
        || (wants_deliver(&text) && (!task.is_empty() || !steps.is_empty()));
    let notify = notify_on_done || wants_notify(&text) || deliver;
    let close = close_listening || wants_close(&text);
    Ok(RunArgs {
        agent,
        task,
        steps,
        background,
        gui,
        close,
        notify,
        deliver,
        cwd,
        name,
        space,
        model,
        effort,
        input_from,
    })
}

/// `_handle_run`: lanza el agente y, con `then`, encadena el resto del flujo.
fn handle_run(
    herdr: &Arc<Herdr>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    args: &Map<String, Value>,
) -> Result<ToolResult, DelegateError> {
    let run = parse_run_args(herdr, callbacks, args)?;
    let effective_task = match run.input_from.clone() {
        Some(source) => match apply_input_from(herdr, callbacks, &run, &source)? {
            RunStart::Done(result) => return Ok(result),
            RunStart::Launch(task) => task,
        },
        None => run.task.clone(),
    };
    launch(herdr, callbacks, state, &run, &effective_task)
}

enum RunStart {
    Launch(String),
    Done(ToolResult),
}

/// `input_from`: la salida de otro agente como contexto, o la cadena si sigue trabajando.
fn apply_input_from(
    herdr: &Herdr,
    callbacks: &HerdrCallbacks,
    run: &RunArgs,
    source: &str,
) -> Result<RunStart, DelegateError> {
    let source_state = agent_state(herdr, source);
    if source_state == "missing" {
        return Ok(RunStart::Done(ToolResult::err(format!(
            "No encuentro el agente de origen {source}."
        ))));
    }
    if source_state == "blocked" || source_state == "error" {
        return Ok(RunStart::Done(ToolResult::err(format!(
            "El agente de origen {source} está {source_state}; atiéndelo antes de encadenar."
        ))));
    }
    if source_state == "working" {
        (callbacks.enqueue_chain)(pending_chain(run, source));
        return Ok(RunStart::Done(ToolResult::ok(json!({
            "chained": true,
            "source": source,
            "agent": run.agent,
            "task": run.task,
            "model": run.model.clone().unwrap_or_default(),
            "effort": run.effort.clone().unwrap_or_default(),
            "message": format!(
                "Cuando termine {source}, lanzaré {} con su salida.",
                run.agent
            ),
        }))));
    }
    let Some(output) = read_output(herdr, source) else {
        return Ok(RunStart::Done(ToolResult::err(format!(
            "No pude leer la salida de {source}."
        ))));
    };
    let packet = build_packet(
        &answer_or_tail(&output, ""),
        None,
        Some(&|text| herdr.save_context(text)),
    );
    Ok(RunStart::Launch(compose_handoff(
        &run.task,
        &packet_text(&packet),
    )))
}

fn pending_chain(run: &RunArgs, source: &str) -> Value {
    let mut spec = json!({
        "source": source,
        "agent": run.agent,
        "task": run.task,
        "cwd": run.cwd,
        "name": run.name,
        "background": run.background,
        "gui": run.gui,
        "space": run.space,
        "model": run.model.clone().unwrap_or_default(),
        "effort": run.effort.clone().unwrap_or_default(),
        "notify_on_done": run.notify && run.steps.is_empty(),
        "notify_final": run.notify,
        "deliver_answer": run.deliver && run.steps.is_empty(),
        "deliver_final": run.deliver,
    });
    if !run.steps.is_empty() {
        if let Value::Object(map) = &mut spec {
            map.insert("then".to_string(), steps_value(&run.steps));
        }
    }
    spec
}

fn steps_value(steps: &[Step]) -> Value {
    Value::Array(
        steps
            .iter()
            .map(|step| {
                Value::Object(
                    step.iter()
                        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                        .collect(),
                )
            })
            .collect(),
    )
}

fn chain_options<'a>(
    run: &'a RunArgs,
    notify: bool,
    deliver: bool,
    chain_id: &'a str,
    flow: Option<&'a [String]>,
) -> ChainOptions<'a> {
    ChainOptions {
        cwd: run.cwd.as_deref(),
        space: run.space.as_deref(),
        notify,
        deliver,
        chain_id,
        flow,
    }
}

fn launch(
    herdr: &Arc<Herdr>,
    callbacks: &HerdrCallbacks,
    state: &SharedState,
    run: &RunArgs,
    task: &str,
) -> Result<ToolResult, DelegateError> {
    let chain_id = if run.steps.is_empty() {
        String::new()
    } else {
        Uuid::new_v4().simple().to_string()
    };
    if !run.steps.is_empty() {
        // El siguiente agente arranca ya, en paralelo, no al terminar este.
        (callbacks.prewarm_chain)(chain_spec(
            "",
            &run.steps,
            &chain_options(run, false, false, &chain_id, None),
        ));
    }
    let context = LaunchContext {
        callbacks: callbacks.clone(),
        agent: run.agent.clone(),
        // La tarea que se recuerda es la del usuario, no la del traspaso.
        task: run.task.clone(),
        steps: run.steps.clone(),
        cwd: run.cwd.clone(),
        name: run.name.clone(),
        space: run.space.clone(),
        notify: run.notify,
        deliver: run.deliver,
        chain_id,
    };
    let mut params = RunAgent {
        agent: &run.agent,
        task,
        cwd: run.cwd.as_deref(),
        name: run.name.as_deref(),
        confirmed: true,
        background: run.background,
        gui: run.gui,
        space: run.space.as_deref(),
        model: run.model.as_deref(),
        effort: run.effort.as_deref(),
        ..RunAgent::default()
    };
    if callbacks.async_launch {
        let ready = context.clone();
        params.on_ready = Some(Box::new(move |outcome| after_ready(&ready, &outcome)));
    }
    let result = herdr.run_agent(driver(herdr), params)?;
    let outcome = wrapped(result.clone());
    if !outcome.ok {
        return Ok(outcome);
    }
    let address = launched_address(&result);
    if address.is_empty() {
        return Ok(outcome);
    }
    set_last_address(state, Some(address));
    if !result.get("launching").is_some_and(truthy) {
        after_ready(&context, &result);
    }
    if run.close {
        (callbacks.request_close)();
    }
    Ok(outcome)
}

fn launched_address(result: &Map<String, Value>) -> String {
    for key in ["address", "terminal"] {
        if let Some(value) = result.get(key).filter(|value| truthy(value)) {
            return py_strip(&python_str(value)).to_string();
        }
    }
    String::new()
}

/// Lo que `after_ready` necesita del lanzamiento, ya clonado para el hilo async.
#[derive(Clone)]
struct LaunchContext {
    callbacks: HerdrCallbacks,
    agent: String,
    task: String,
    steps: Vec<Step>,
    cwd: Option<String>,
    name: Option<String>,
    space: Option<String>,
    notify: bool,
    deliver: bool,
    chain_id: String,
}

/// Se ejecuta cuando el agente ya arrancó y recibió su tarea.
fn after_ready(context: &LaunchContext, outcome: &Map<String, Value>) {
    let ready_address = text_field(outcome, "address");
    if !outcome.get("ok").is_some_and(truthy) {
        (context.callbacks.launch_failed)(
            &ready_address,
            &context.agent,
            &text_field(outcome, "error"),
        );
        return;
    }
    if !context.steps.is_empty() {
        let flow = vec![context
            .name
            .clone()
            .unwrap_or_else(|| context.agent.clone())];
        let options = ChainOptions {
            cwd: context.cwd.as_deref(),
            space: context.space.as_deref(),
            notify: context.notify,
            deliver: context.deliver,
            chain_id: &context.chain_id,
            flow: Some(&flow),
        };
        (context.callbacks.enqueue_chain)(chain_spec(&ready_address, &context.steps, &options));
    } else if context.notify && !context.task.is_empty() {
        (context.callbacks.remember_task)(&ready_address, &context.agent, &context.task);
        if context.deliver {
            (context.callbacks.deliver_answer)(&ready_address);
        }
        (context.callbacks.watch)(&ready_address);
    }
}

/// `_agent_state`: estado del agente, o «missing» si no se puede leer.
fn agent_state(herdr: &Herdr, address: &str) -> String {
    match herdr.get_agent(address) {
        Ok(agent) => py_strip(&text_or_empty(agent.get("state")))
            .to_lowercase()
            .to_string(),
        Err(_) => "missing".to_string(),
    }
}

/// `_read_output`: `read_full` no recorta a 6000 caracteres.
fn read_output(herdr: &Herdr, address: &str) -> Option<String> {
    herdr.read_full(address, 1500).ok()
}

#[cfg(test)]
mod tests;
