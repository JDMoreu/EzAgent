//! Subcomando `eval`: el mismo proveedor de promptfoo que `evals/provider.py`,
//! pero sobre el código REAL en Rust (F8-02).
//!
//! La petición llega por stdin (JSON del `call_api`) y la respuesta sale por
//! stdout (JSON). El modo `context` no llama a ningún modelo; el modo `live`
//! hace una sola llamada al LLM configurado, registra las herramientas pedidas
//! sin ejecutarlas y corta antes de la segunda llamada.
//!
//! Privacidad: todo corre con un perfil de usuario temporal, así que la
//! memoria, las skills y el `system.md` reales nunca viajan al proveedor.

mod guard;
mod payload;
mod request;
mod runtime;
mod sandbox;
#[cfg(test)]
mod tests;

use std::io::{Read, Write};
use std::sync::Arc;

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_llm::messenger::{AgentRuntimeApi, AskMode, Messenger, MessengerCallbacks};
use serde_json::{json, Value};

use sandbox::Sandbox;

const USAGE: &str = "usage: ezagent-voice eval [--mode {context,live}]";

/// Modo del eval (`config.mode` del YAML de promptfoo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Solo el prompt de sistema y las herramientas; sin modelo.
    Context,
    /// Una sola llamada al modelo configurado.
    Live,
}

impl Mode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "context" => Ok(Self::Context),
            "live" => Ok(Self::Live),
            other => Err(format!(
                "argument --mode: invalid choice: '{other}' (choose from 'context', 'live')"
            )),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Help,
    Mode(Mode),
}

/// Punto de entrada del subcomando; `None` si `argv` no es `eval`.
pub fn maybe_run(argv: &[String]) -> Option<i32> {
    if argv.get(1).map(String::as_str) != Some("eval") {
        return None;
    }
    Some(run(&argv[2..]))
}

fn run(argv: &[String]) -> i32 {
    let parsed = match parse_args(argv) {
        Ok(parsed) => parsed,
        Err(error) => {
            write_stderr(&format!("{USAGE}\nezagent-voice: error: {error}\n"));
            return 2;
        }
    };
    let mode = match parsed {
        Parsed::Help => {
            write_stdout(&format!("{USAGE}\n"));
            return 0;
        }
        Parsed::Mode(mode) => mode,
    };
    match execute(mode) {
        Ok(value) => {
            write_stdout(&format!("{}\n", dumps_with(&value, &DumpOptions::utf8())));
            0
        }
        Err(error) => fail(&error, 1),
    }
}

/// Un fallo se anuncia con el mismo `{"error": "Tipo: mensaje"}` del Python.
fn fail(error: &str, code: i32) -> i32 {
    write_stdout(&format!(
        "{}\n",
        dumps_with(&json!({"error": error}), &DumpOptions::utf8())
    ));
    code
}

fn write_stdout(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn write_stderr(text: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
}

fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let mut mode = Mode::Live;
    let mut extras: Vec<String> = Vec::new();
    let mut index = 0usize;
    while index < argv.len() {
        let token = argv[index].clone();
        index += 1;
        let (name, inline) = match token.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (token.clone(), None),
        };
        if name == "-h" || name == "--help" {
            return Ok(Parsed::Help);
        }
        if name != "--mode" {
            extras.push(token);
            continue;
        }
        let value = match inline {
            Some(value) => value,
            None => {
                let value = argv
                    .get(index)
                    .ok_or_else(|| "argument --mode: expected one argument".to_string())?;
                index += 1;
                value.clone()
            }
        };
        mode = Mode::parse(&value)?;
    }
    if !extras.is_empty() {
        return Err(format!("unrecognized arguments: {}", extras.join(" ")));
    }
    Ok(Parsed::Mode(mode))
}

fn execute(mode: Mode) -> Result<Value, String> {
    let raw = read_stdin()?;
    let request = request::EvalRequest::from_json(&raw)?;
    // Perfil falso ANTES de leer la configuración: nada del usuario real entra
    // al prompt (el `_SANDBOX` de `provider.py`).
    let sandbox = Sandbox::new()?;
    match mode {
        Mode::Context => run_context(&sandbox, &request.user),
        Mode::Live => run_live(&sandbox, &request),
    }
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("OSError: {error}"))?;
    Ok(text)
}

/// `_run_context`: prompt de sistema y herramientas sin llamar a ningún modelo.
fn run_context(sandbox: &Sandbox, user: &str) -> Result<Value, String> {
    let case_dir = sandbox.case_dir()?;
    let case = runtime::Case::build(case_dir.path())?;
    let tools = case.runtime.tools_for(user);
    let result = json!({
        "system_prompt": case.runtime.system_prompt(),
        "tools": tools,
        "available_tools": payload::tool_names(&tools),
    });
    case.runtime.close();
    Ok(result)
}

/// `_run_live`: un paso del modelo real; las herramientas se simulan.
fn run_live(sandbox: &Sandbox, request: &request::EvalRequest) -> Result<Value, String> {
    let case_dir = sandbox.case_dir()?;
    let mut case = runtime::Case::build(case_dir.path())?;
    request::configure(&mut case.voice, &request.settings)?;
    let recorder = Arc::new(runtime::RecordingRuntime::new(case.runtime.clone()));
    let messenger = Messenger::new(
        case.voice.clone(),
        MessengerCallbacks::default(),
        Some(recorder as Arc<dyn AgentRuntimeApi>),
    );
    let guard = guard::install(&messenger);
    let mut messages = messenger.messages();
    messages.extend(request.history.iter().cloned());
    messenger.set_messages(messages);
    let mut failure = None;
    for item in messenger.ask_stream(&request.user, AskMode::Voice) {
        if let Err(error) = item {
            if !guard.stopped() {
                failure = Some(format!("RuntimeError: {error}"));
            }
            break;
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    let step = payload::first_step(&messenger.messages(), &request.user);
    let tools = case.runtime.tools_for(&request.user);
    let result = json!({
        "text": payload::text(&step),
        "tool_calls": payload::parse_calls(&step),
        "available_tools": payload::tool_names(&tools),
        "model": ez_core::pyvalue::text_or_empty(case.voice.get("llm_model")),
    });
    case.runtime.close();
    Ok(result)
}
