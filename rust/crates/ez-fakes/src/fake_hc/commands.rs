//! Implementación de cada subcomando de `fake-hc`.

use std::path::Path;

use serde_json::{json, Value};

use super::{executable_name, json_terminal, Reply, State, Terminal};
use crate::fake_agent;

/// Resultado de un comando: respuesta y si el estado cambió.
pub struct Outcome {
    pub reply: Reply,
    pub changed: bool,
}

impl Outcome {
    fn read_only(reply: Reply) -> Self {
        Self {
            reply,
            changed: false,
        }
    }

    fn mutated(reply: Reply) -> Self {
        Self {
            reply,
            changed: true,
        }
    }
}

fn not_found(address: &str) -> Outcome {
    Outcome::read_only(Reply::json_error(
        1,
        &json!({
            "error": {
                "kind": "not_found",
                "message": format!("No existe la terminal {address}."),
            }
        }),
    ))
}

fn bad_usage(message: &str) -> Outcome {
    Outcome::read_only(Reply::json_error(
        2,
        &json!({"ok": false, "error": message}),
    ))
}

fn positional(args: &[String]) -> Vec<&str> {
    args.iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(String::as_str)
        .collect()
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_string());
        }
        if arg == flag {
            return args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

fn split_after_double_dash(args: &[String]) -> (Vec<String>, Vec<String>) {
    match args.iter().position(|arg| arg == "--") {
        Some(index) => (args[..index].to_vec(), args[index + 1..].to_vec()),
        None => (args.to_vec(), Vec::new()),
    }
}

fn parse_argv(text: Option<String>) -> Vec<String> {
    text.and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_else(|| vec!["powershell.exe".to_string()])
}

fn ok_with(address: &str, extra: Value) -> Reply {
    let mut value = json!({"ok": true, "address": address});
    if let (Some(object), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        object.extend(extra.clone());
    }
    Reply::json(&value)
}

fn list(state: &mut State, args: &[String]) -> Outcome {
    let space = flag_value(args, "--space");
    let mut changed = false;
    for terminal in &mut state.terminals {
        changed |= observe(terminal);
    }
    let terminals: Vec<Value> = state
        .terminals
        .iter()
        .filter(|terminal| space.as_deref().is_none_or(|space| terminal.space == space))
        .map(json_terminal)
        .collect();
    Outcome {
        reply: Reply::json(&json!({"terminals": terminals})),
        changed,
    }
}

fn spaces(state: &State) -> Outcome {
    let mut names = state.spaces.clone();
    for terminal in &state.terminals {
        if !names.iter().any(|name| name == &terminal.space) {
            names.push(terminal.space.clone());
        }
    }
    let spaces: Vec<Value> = names
        .iter()
        .map(|name| {
            let tabs: Vec<Value> = state
                .terminals
                .iter()
                .filter(|terminal| &terminal.space == name)
                .map(|terminal| {
                    json!({
                        "name": terminal.name,
                        "terminal": terminal.address,
                        "address": terminal.address,
                        "where": terminal.where_,
                    })
                })
                .collect();
            json!({"name": name, "tabs": tabs})
        })
        .collect();
    Outcome::read_only(Reply::json(&json!({"spaces": spaces})))
}

fn tabs(state: &State) -> Outcome {
    let tabs: Vec<Value> = state
        .terminals
        .iter()
        .map(|terminal| {
            json!({
                "name": terminal.name,
                "terminal": terminal.address,
                "address": terminal.address,
                "space": terminal.space,
                "where": terminal.where_,
            })
        })
        .collect();
    Outcome::read_only(Reply::json(&json!({"tabs": tabs})))
}

fn get(state: &mut State, args: &[String]) -> Outcome {
    let values = positional(args);
    let Some(address) = values.first() else {
        return bad_usage("get necesita una dirección");
    };
    let address = address.to_string();
    let Some(terminal) = state.find_mut(&address) else {
        return not_found(&address);
    };
    let changed = observe(terminal);
    Outcome {
        reply: Reply::json(&json_terminal(terminal)),
        changed,
    }
}

/// Momento actual en milisegundos UNIX (suficiente para el guion).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

fn finish(terminal: &mut Terminal) -> bool {
    if terminal.state != "working" {
        return false;
    }
    terminal.state = String::from("idle");
    if !terminal.conclusion.trim().is_empty() {
        terminal.screen.push_str("\n● ");
        terminal.screen.push_str(terminal.conclusion.trim());
    }
    true
}

/// Observa la pantalla: un trabajo guionizado termina por tiempo o por lecturas.
fn observe(terminal: &mut Terminal) -> bool {
    if terminal.state != "working" {
        return false;
    }
    if let Some(limit) = terminal.done_after_ms {
        if terminal.started_at.is_none() {
            // El primer sondeo arranca el reloj: el vigía ve «working» primero.
            terminal.started_at = Some(now_ms());
            return true;
        }
        let started = terminal.started_at.unwrap_or_default();
        if now_ms().saturating_sub(started) >= limit {
            return finish(terminal);
        }
    }
    let Some(done_after) = terminal.done_after_reads else {
        return false;
    };
    terminal.reads += 1;
    if terminal.reads < done_after {
        return true;
    }
    finish(terminal)
}

fn read(state: &mut State, args: &[String]) -> Outcome {
    let Some(address) = positional(args).first().map(|value| value.to_string()) else {
        return bad_usage("read necesita una dirección");
    };
    let Some(terminal) = state.find_mut(&address) else {
        // `read` no es un comando JSON: su salida se usa como pantalla.
        return Outcome::read_only(Reply::error(
            1,
            &format!("No existe la terminal {address}."),
        ));
    };
    let changed = observe(terminal);
    let limit = flag_value(args, "--lines")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);
    let mut lines: Vec<&str> = terminal.screen.split('\n').collect();
    if let Some(limit) = limit {
        if lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }
    }
    Outcome {
        reply: Reply::ok(format!("{}\n", lines.join("\n"))),
        changed,
    }
}

fn send(state: &mut State, args: &[String]) -> Outcome {
    let Some(address) = positional(args).first().map(|value| value.to_string()) else {
        return bad_usage("send necesita una dirección");
    };
    let text = args
        .iter()
        .skip_while(|arg| arg.as_str() != address)
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if text.trim().is_empty() {
        return bad_usage("send necesita el texto");
    }
    let Some(terminal) = state.find_mut(&address) else {
        return not_found(&address);
    };
    apply_text(terminal, &text);
    Outcome::mutated(ok_with(&address, json!({"message": "Mensaje enviado."})))
}

fn put(state: &mut State, args: &[String]) -> Outcome {
    let values = positional(args);
    let (Some(address), Some(path)) = (values.first(), values.get(1)) else {
        return bad_usage("put necesita dirección y ruta");
    };
    if !Path::new(*path).is_file() {
        return Outcome::read_only(Reply::json_error(
            1,
            &json!({"ok": false, "error": format!("No encuentro el archivo {path}.")}),
        ));
    }
    let Some(terminal) = state.find_mut(address) else {
        return not_found(address);
    };
    terminal.sent.push((*path).to_string());
    Outcome::mutated(Reply::json(&json!({
        "ok": true,
        "address": address,
        "path": path,
    })))
}

fn keys(state: &mut State, args: &[String]) -> Outcome {
    let Some(address) = positional(args).first().map(|value| value.to_string()) else {
        return bad_usage("keys necesita una dirección");
    };
    let pressed: Vec<String> = args
        .iter()
        .skip_while(|arg| arg.as_str() != address)
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .collect();
    if pressed.is_empty() {
        return bad_usage("keys necesita al menos una tecla");
    }
    let Some(terminal) = state.find_mut(&address) else {
        return not_found(&address);
    };
    terminal.keys.extend(pressed.clone());
    if pressed.iter().any(|key| key == "esc") {
        terminal.selector.clear();
    }
    if pressed.iter().any(|key| key == "ctrl+t") {
        const LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
        let level = LEVELS[terminal.variant_index % LEVELS.len()];
        terminal.variant_index += 1;
        terminal
            .screen
            .push_str(&format!("\nSwitched variant to {level}"));
    }
    if pressed.iter().any(|key| key == "enter") && !terminal.pending_input.is_empty() {
        let text = std::mem::take(&mut terminal.pending_input);
        apply_text(terminal, &text);
    }
    Outcome::mutated(Reply::json(&json!({
        "ok": true,
        "address": address,
        "keys": pressed,
    })))
}

fn kill(state: &mut State, args: &[String]) -> Outcome {
    let Some(address) = positional(args).first().map(|value| value.to_string()) else {
        return bad_usage("kill necesita una dirección");
    };
    if state.remove(&address).is_none() {
        return not_found(&address);
    }
    Outcome::mutated(Reply::json(&json!({
        "ok": true,
        "address": address,
        "message": "Terminal terminado.",
    })))
}

fn is_agent_launch(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    let name = executable_name(first);
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    const AGENTS: [&str; 7] = [
        "opencode",
        "claude",
        "claude-code",
        "codex",
        "mcode",
        "minimax",
        "pi",
    ];
    if AGENTS.contains(&name) || text.contains("fake-agent") {
        return Some(name.to_string());
    }
    None
}

/// Aplica texto tecleado: arranque de agente, comando interno o tarea.
fn apply_text(terminal: &mut Terminal, text: &str) {
    terminal.sent.push(text.to_string());
    terminal.reads = 0;
    if let Some(agent) = is_agent_launch(text) {
        terminal.agent = agent.clone();
        terminal.state = String::from("idle");
        terminal.screen = fake_agent::launch_screen(&agent, text);
        return;
    }
    if let Some(model) = text.strip_prefix("/model ") {
        terminal
            .screen
            .push_str(&format!("\nSwitched model to {}", model.trim()));
        return;
    }
    if let Some(effort) = text.strip_prefix("/effort ") {
        terminal
            .screen
            .push_str(&format!("\nSwitched variant to {}", effort.trim()));
        return;
    }
    if text == "/models" {
        terminal.screen.push_str("\nSelect model");
        terminal.selector = String::from("model");
        return;
    }
    if text == "/effort" {
        terminal.screen.push_str("\nSelect variant");
        terminal.selector = String::from("effort");
        return;
    }
    if terminal.selector == "model" && !text.starts_with('/') {
        terminal
            .screen
            .push_str(&format!("\nSwitched model to {}", text.trim()));
        terminal.selector.clear();
        return;
    }
    terminal.state = String::from("working");
    terminal.selector.clear();
    terminal.started_at = Some(now_ms());
    terminal.screen.push_str("\n❯ ");
    terminal.screen.push_str(text);
}

fn run(state: &mut State, args: &[String]) -> Outcome {
    let (flags, argv) = split_after_double_dash(args);
    let name = flag_value(&flags, "--name").unwrap_or_else(|| "agente".to_string());
    let folder = flag_value(&flags, "--cwd").unwrap_or_default();
    let label = flag_value(&flags, "--label").unwrap_or_default();
    let agent = label
        .strip_prefix("agent=")
        .map(str::to_string)
        .or_else(|| argv.first().map(|arg| executable_name(arg)))
        .unwrap_or_else(|| "agente".to_string());
    let terminal = state.create(&name, &folder, &argv, "main");
    let address = terminal.address.clone();
    let mut terminal = terminal;
    terminal.agent = agent.clone();
    terminal.state = String::from("idle");
    terminal.screen = fake_agent::launch_screen(&agent, "");
    if let Some(slot) = state.find_mut(&address) {
        *slot = terminal;
    }
    Outcome::mutated(Reply::json(&json!({
        "ok": true,
        "address": address,
        "terminal": address,
        "tab": state.find(&address).map(|terminal| terminal.tab.clone()),
        "space": "main",
    })))
}

fn tab_new(state: &mut State, args: &[String]) -> Outcome {
    let name = flag_value(args, "--name").unwrap_or_else(|| "terminal".to_string());
    let folder = flag_value(args, "--folder").unwrap_or_default();
    let argv = parse_argv(flag_value(args, "--argv"));
    let space = flag_value(args, "--space").unwrap_or_else(|| "main".to_string());
    let terminal = state.create(&name, &folder, &argv, &space);
    let address = terminal.address;
    let reply = json!({
        "ok": true,
        "terminal": address.clone(),
        "address": address,
        "tab": terminal.tab,
        "space": terminal.space,
    });
    Outcome::mutated(Reply::json(&reply))
}

fn focus(state: &State, args: &[String]) -> Outcome {
    let address = flag_value(args, "--terminal")
        .or_else(|| positional(args).first().map(|value| value.to_string()));
    let Some(address) = address else {
        return bad_usage("agent.focus necesita --terminal");
    };
    if state.find(&address).is_none() {
        return not_found(&address);
    }
    Outcome::read_only(Reply::json(&json!({
        "ok": true,
        "terminal": address,
        "focused": true,
    })))
}

fn do_command(state: &mut State, args: &[String]) -> Outcome {
    let Some(subcommand) = args.first() else {
        return bad_usage("do necesita un subcomando");
    };
    match subcommand.as_str() {
        "tab.new" => tab_new(state, &args[1..]),
        "agent.focus" => focus(state, &args[1..]),
        other => bad_usage(&format!("subcomando desconocido: do {other}")),
    }
}

/// Ejecuta `command` sobre el estado; el llamador guarda si `changed`.
pub fn execute(state: &mut State, command: &str, args: &[String]) -> Outcome {
    match command {
        "ls" => list(state, args),
        "spaces" => spaces(state),
        "tabs" => tabs(state),
        "get" => get(state, args),
        "read" => read(state, args),
        "send" => send(state, args),
        "put" => put(state, args),
        "keys" => keys(state, args),
        "kill" => kill(state, args),
        "run" => run(state, args),
        "do" => do_command(state, args),
        other => bad_usage(&format!("comando desconocido: {other}")),
    }
}
