//! Comando `ez`: argumentos, rutas, cliente y códigos de salida (diseño §7).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::client::{self, ClientError};
use crate::local::{self, LocalDelegate};
use crate::protocol::{Reply, Request};
use crate::scan_cmds;

/// Manual del agente principal. Es un contrato: no se edita.
pub const SKILL: &str = include_str!("skill.md");

/// El texto cuando falta el entorno del árbitro.
pub(crate) const MISSING_ENV: &str =
    "Este comando solo funciona dentro de una terminal abierta por EzAgent (faltan EZ_URL y EZ_TOKEN).";
const DEFAULT_WAIT_SECONDS: u64 = 170;

/// Opciones que no llevan valor.
const FLAG_OPTIONS: [&str; 11] = [
    "no-notify",
    "outlive-parent",
    "hc-notes",
    "pass",
    "fail",
    "refresh",
    "local",
    "force",
    "auto",
    "same",
    "keep",
];

const USAGE: &str = "
ez — comando del espacio de trabajo de EzAgent

Uso:
  ez --skill                     Imprime el manual del agente principal
  ez --help                      Muestra esta ayuda
  ez --version                   Muestra la versión
  ez status                      Estado de la tarea y de la ronda
  ez inventory                   Agentes y modelos disponibles
  ez task new --title T --brief MD --criteria TOML
  ez team propose --file TOML
  ez round start
  ez round close
  ez delegate <papel> --brief MD
  ez send <subagente> --file MD
  ez roles                       Papeles disponibles
  ez verify
  ez close
  ez pause --reason TEXTO
  ez wait [--timeout SEGUNDOS]
  ez report --verdict pass|fail --file MD --blocking N --major N --minor N
            [--mutations-tried N] [--mutations-caught N]

Recursos e inventario (siempre locales, no hablan con el árbitro):
  ez init [--refresh] [--json]
  ez resources [--json]
  ez inventory --local [--json]
  ez usage [--json]              Uso y límites de cada agente
  ez recommend <papel> [--size s|m|l] [--json]
                                 Tipo, modelo y esfuerzo recomendados

Modo local (Herdr, sin EZ_URL ni EZ_TOKEN):
  ez delegate <papel> [--agent <tipo>] [--model <modelo>] [--effort <nivel>]
               --brief <archivo> [--name <nombre>] [--show tree|tab|right|down]
               [--cwd <carpeta>] [--no-notify] [--outlive-parent] [--force] [--json]
                                 Sin --agent lo elige la recomendación
  ez report --state terminado|fallido [--summary TEXTO] [--file INFORME]
  ez validate <nombre> --pass|--fail [--summary TEXTO] [--file INFORME]
  ez tasks [--parent <padre>] [--json]
  ez retry <nombre> [--agent T] [--model M] [--effort E] [--auto] [--same]
             [--keep]              Relanza un subagente bloqueado o fallido
  ez watch --parent <padre> --dir <raíz> [--hc <ruta>]

Opciones generales:
  --json    Imprime la respuesta completa como JSON (cualquier comando)

Códigos de salida: 0 bien, 1 rechazado, 2 uso incorrecto, 3 sin conexión.
";

/// Por qué el comando no puede seguir.
pub(crate) enum CliError {
    /// Mensaje para la persona; se acompaña del uso.
    Usage(String),
    /// Archivo que no existe, ya resuelto contra la carpeta actual.
    MissingFile(PathBuf),
    /// Falta el entorno del árbitro (código 3).
    NoEnv,
}

/// Lo que hace el comando: hablar con el árbitro o delegar en Herdr.
pub(crate) enum Command {
    /// Una orden para el árbitro (la de siempre).
    Arbiter(Request),
    /// Una delegación en el modo local (Herdr).
    Local(LocalDelegate),
    /// Una orden del estado de los subagentes en el modo local (Herdr).
    LocalState(local::LocalState),
    /// `ez retry`: relanzar un subagente bloqueado o fallido.
    LocalRetry(local::LocalRetry),
    /// Recursos, inventario o `ez init` (siempre local).
    Scan(scan_cmds::ScanCommand),
}

mod parsed;

pub(crate) use parsed::Parsed;

fn build(
    parsed: &mut Parsed,
    cwd: &Path,
    arbiter: bool,
    partial_env: bool,
) -> Result<Command, CliError> {
    let command = parsed.word("Falta el comando. Pasa --help para ver el uso.")?;
    match command.as_str() {
        "status" => parsed.finish("status", json!({})),
        // DECISION: `--json` solo controla la salida; el inventario viaja en `data`.
        // Con `--local` el inventario se mide en esta máquina y no va al árbitro.
        "inventory" => {
            if parsed.flag("local") {
                scan_cmds::build_inventory(parsed)
            } else {
                parsed.finish("inventory", json!({}))
            }
        }
        "init" => scan_cmds::build_init(parsed),
        "resources" => scan_cmds::build_resources(parsed),
        "usage" => scan_cmds::build_usage(parsed),
        "recommend" => scan_cmds::build_recommend(parsed),
        "verify" => parsed.finish("verify", json!({})),
        "close" => parsed.finish("task.close", json!({})),
        "pause" => {
            let reason = parsed.required("reason")?;
            parsed.finish("pause", json!({ "reason": reason }))
        }
        "wait" => {
            let timeout = parsed.number("timeout")?.unwrap_or(DEFAULT_WAIT_SECONDS);
            parsed.finish("wait", json!({ "timeout": timeout }))
        }
        "task" => build_task(parsed, cwd),
        "team" => build_team(parsed, cwd),
        "round" => build_round(parsed),
        "delegate" => match local::build_delegate(parsed, cwd, arbiter, partial_env)? {
            local::Delegate::Arbiter(request) => Ok(Command::Arbiter(request)),
            local::Delegate::Local(request) => Ok(Command::Local(request)),
        },
        "roles" => parsed.finish("roles", json!({})),
        "send" => build_send(parsed, cwd),
        "report" => build_report(parsed, cwd, arbiter, partial_env),
        "validate" => build_state(parsed, cwd, arbiter, partial_env, local::build_validate),
        "tasks" => build_state(parsed, cwd, arbiter, partial_env, local::build_tasks),
        "retry" => build_retry(parsed, arbiter, partial_env),
        "watch" => build_state(parsed, cwd, arbiter, partial_env, local::build_watch),
        other => Err(CliError::Usage(format!("Comando desconocido: {other}."))),
    }
}

/// Analiza una orden del estado de los subagentes (solo en el modo local).
fn build_state(
    parsed: &mut Parsed,
    cwd: &Path,
    arbiter: bool,
    partial_env: bool,
    build: fn(&mut Parsed, &Path) -> Result<local::LocalState, CliError>,
) -> Result<Command, CliError> {
    // DECISION: en una tarea del árbitro estos comandos no existen: el
    // árbitro lleva su propio estado.
    if arbiter {
        return Err(CliError::Usage(
            "En una tarea del árbitro el estado de los subagentes lo lleva el árbitro.".to_string(),
        ));
    }
    if partial_env {
        return Err(CliError::NoEnv);
    }
    Ok(Command::LocalState(build(parsed, cwd)?))
}

/// `ez retry`: solo en el modo local (el árbitro lleva su propio estado).
fn build_retry(parsed: &mut Parsed, arbiter: bool, partial_env: bool) -> Result<Command, CliError> {
    if arbiter {
        return Err(CliError::Usage(
            "En una tarea del árbitro el estado de los subagentes lo lleva el árbitro.".to_string(),
        ));
    }
    if partial_env {
        return Err(CliError::NoEnv);
    }
    Ok(Command::LocalRetry(local::build_retry(parsed)?))
}

/// `ez report`: la del árbitro o la del modo local (§13).
///
/// Con `--state` es la del modo local (el subagente declara cómo terminó);
/// sin él, `report` sigue siendo la orden de siempre para el árbitro.
fn build_report(
    parsed: &mut Parsed,
    cwd: &Path,
    arbiter: bool,
    partial_env: bool,
) -> Result<Command, CliError> {
    if !parsed.has("state") {
        return Ok(Command::Arbiter(build_arbiter_report(parsed, cwd)?));
    }
    if arbiter {
        return Err(CliError::Usage(
            "En una tarea del árbitro el estado de los subagentes lo lleva el árbitro.".to_string(),
        ));
    }
    if partial_env {
        return Err(CliError::NoEnv);
    }
    Ok(Command::LocalState(local::build_local_report(parsed, cwd)?))
}

fn build_task(parsed: &mut Parsed, cwd: &Path) -> Result<Command, CliError> {
    let subcommand = parsed.word("Falta el subcomando de task (new).")?;
    if subcommand != "new" {
        return Err(CliError::Usage(format!(
            "Subcomando de task desconocido: {subcommand}."
        )));
    }
    let title = parsed.required("title")?;
    let brief = existing(&parsed.required("brief")?, cwd)?;
    let criteria = existing(&parsed.required("criteria")?, cwd)?;
    parsed.finish(
        "task.new",
        json!({ "title": title, "brief": brief, "criteria": criteria }),
    )
}

fn build_team(parsed: &mut Parsed, cwd: &Path) -> Result<Command, CliError> {
    let subcommand = parsed.word("Falta el subcomando de team (propose).")?;
    if subcommand != "propose" {
        return Err(CliError::Usage(format!(
            "Subcomando de team desconocido: {subcommand}."
        )));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    parsed.finish("team.propose", json!({ "file": file }))
}

fn build_round(parsed: &mut Parsed) -> Result<Command, CliError> {
    let subcommand = parsed.word("Falta el subcomando de round (start o close).")?;
    match subcommand.as_str() {
        "start" => parsed.finish("round.start", json!({})),
        "close" => parsed.finish("round.close", json!({})),
        other => Err(CliError::Usage(format!(
            "Subcomando de round desconocido: {other}."
        ))),
    }
}

fn build_send(parsed: &mut Parsed, cwd: &Path) -> Result<Command, CliError> {
    let fish = parsed.word("Falta el subagente.")?;
    if fish.trim().is_empty() {
        return Err(CliError::Usage(
            "El nombre del subagente no puede estar vacío.".to_string(),
        ));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    parsed.finish("fish.send", json!({ "fish": fish, "file": file }))
}

fn build_arbiter_report(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let verdict = parsed.required("verdict")?;
    if verdict != "pass" && verdict != "fail" {
        return Err(CliError::Usage(
            "--verdict debe ser pass o fail.".to_string(),
        ));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    let blocking = parsed.required_number("blocking")?;
    let major = parsed.required_number("major")?;
    let minor = parsed.required_number("minor")?;
    let tried = parsed.number("mutations-tried")?;
    let caught = parsed.number("mutations-caught")?;
    parsed.finish_request(
        "report",
        json!({
            "verdict": verdict,
            "file": file,
            "blocking": blocking,
            "major": major,
            "minor": minor,
            "mutations_tried": tried,
            "mutations_caught": caught,
        }),
    )
}

fn existing(value: &str, cwd: &Path) -> Result<String, CliError> {
    let path = cwd.join(value);
    // DECISION: basta con que la ruta exista; el árbitro decide si es un archivo válido.
    if !path.exists() {
        return Err(CliError::MissingFile(path));
    }
    Ok(path.to_string_lossy().into_owned())
}

fn special(args: &[String], out: &mut dyn Write) -> Option<i32> {
    if args.len() != 1 {
        return None;
    }
    match args[0].as_str() {
        "--skill" => {
            let _ = out.write_all(SKILL.as_bytes());
            Some(0)
        }
        "--help" => {
            let _ = out.write_all(USAGE.as_bytes());
            Some(0)
        }
        "--version" => {
            let _ = writeln!(out, "{}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        _ => None,
    }
}

fn usage_error(err: &mut dyn Write, detail: Option<&str>) -> i32 {
    if let Some(detail) = detail {
        let _ = writeln!(err, "{detail}");
    }
    let _ = err.write_all(USAGE.as_bytes());
    2
}

fn timeout_seconds(request: &Request) -> u64 {
    match request.command.as_str() {
        "wait" => request
            .args
            .get("timeout")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_WAIT_SECONDS)
            .saturating_add(15),
        "verify" | "round.close" | "task.close" => 3600,
        _ => 120,
    }
}

fn print_reply(out: &mut dyn Write, reply: &Reply, json: bool) {
    if json {
        if let Ok(text) = serde_json::to_string(reply) {
            let _ = writeln!(out, "{text}");
            return;
        }
    }
    let _ = writeln!(out, "{}", reply.message);
}

/// La delegación en Herdr: una línea legible (o el mapa con `--json`).
fn print_delegate(out: &mut dyn Write, err: &mut dyn Write, row: &Map<String, Value>, json: bool) {
    if json {
        if let Ok(text) = serde_json::to_string(row) {
            let _ = writeln!(out, "{text}");
        }
        return;
    }
    let _ = writeln!(out, "{}", local::summary(row));
    for warning in local::warnings(row) {
        let _ = writeln!(err, "Aviso: {warning}");
    }
}

/// La orden va al árbitro (el modo de siempre).
fn send_to_arbiter(
    request: &Request,
    url: Option<String>,
    token: Option<String>,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let (Some(url), Some(token)) = (url, token) else {
        let _ = writeln!(err, "{MISSING_ENV}");
        return 3;
    };
    let timeout = Duration::from_secs(timeout_seconds(request));
    match client::send(&url, &token, request, timeout) {
        Ok(reply) => {
            print_reply(out, &reply, json);
            if reply.ok {
                0
            } else {
                1
            }
        }
        Err(ClientError::Unreachable) => {
            let _ = writeln!(
                err,
                "No pude conectar con EzAgent en {url}. ¿Está abierta la ventana de EzAgent?"
            );
            3
        }
        Err(ClientError::Unauthorized(message)) => {
            let _ = writeln!(err, "{message}");
            3
        }
        Err(ClientError::Failed(message)) => {
            // DECISION: cualquier estado distinto de 200 o 401 cuenta como rechazo.
            let _ = writeln!(err, "{message}");
            1
        }
    }
}

/// La delegación en Herdr: sin árbitro, el subagente cuelga de esta terminal.
fn delegate_in_herdr(
    request: &LocalDelegate,
    env: &dyn Fn(&str) -> Option<String>,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let Some(parent) = non_empty(env("HC_TERMINAL_ID")) else {
        let _ = writeln!(err, "{}", local::MISSING_TERMINAL);
        return 3;
    };
    match local::delegate(request, &parent, env) {
        Ok(row) => {
            print_delegate(out, err, &row, json);
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.message());
            error.code()
        }
    }
}

/// Punto de entrada de `ez`; `argv` incluye el nombre del programa.
///
/// Devuelve el código de salida: 0 bien, 1 rechazado, 2 uso incorrecto, 3 sin
/// conexión o token rechazado.
pub fn main_with(
    argv: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let args = argv.get(1..).unwrap_or(&[]);
    if let Some(code) = special(args, out) {
        return code;
    }
    let mut parsed = match Parsed::parse(args) {
        Ok(parsed) => parsed,
        Err(detail) => return usage_error(err, Some(&detail)),
    };
    let url = non_empty(env("EZ_URL"));
    let token = non_empty(env("EZ_TOKEN"));
    // DECISION: el modo árbitro necesita las dos variables; con una sola, el
    // entorno está a medias y el comando avisa (código 3).
    let arbiter = url.is_some() && token.is_some();
    let partial_env = url.is_some() != token.is_some();
    let command = match build(&mut parsed, cwd, arbiter, partial_env) {
        Ok(command) => command,
        Err(CliError::Usage(detail)) => return usage_error(err, Some(&detail)),
        Err(CliError::MissingFile(path)) => {
            let _ = writeln!(err, "No existe el archivo: {}", path.display());
            return 2;
        }
        Err(CliError::NoEnv) => {
            let _ = writeln!(err, "{MISSING_ENV}");
            return 3;
        }
    };
    match command {
        Command::Arbiter(request) => send_to_arbiter(&request, url, token, parsed.json, out, err),
        Command::Local(request) => delegate_in_herdr(&request, env, parsed.json, out, err),
        Command::LocalState(state) => local::run_state(&state, env, parsed.json, out, err),
        Command::LocalRetry(command) => local::run_retry(&command, env, parsed.json, out, err),
        Command::Scan(scan) => scan_cmds::run(&scan, env, cwd, parsed.json, out, err),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}
