//! Comando `ez`: argumentos, rutas, cliente y códigos de salida (diseño §7).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::client::{self, ClientError};
use crate::protocol::{Reply, Request};

/// Manual del elefante. Es contrato del elefante: no se edita.
pub const SKILL: &str = include_str!("skill.md");

const MISSING_ENV: &str =
    "Este comando solo funciona dentro de una terminal abierta por EzAgent (faltan EZ_URL y EZ_TOKEN).";
const DEFAULT_WAIT_SECONDS: u64 = 170;

const USAGE: &str = "\
ez — comando del espacio de trabajo de EzAgent

Uso:
  ez --skill                     Imprime el manual del elefante
  ez --help                      Muestra esta ayuda
  ez --version                   Muestra la versión
  ez status                      Estado de la tarea y de la ronda
  ez inventory [--json]          Agentes y modelos disponibles
  ez task new --title T --brief MD --criteria TOML
  ez team propose --file TOML
  ez round start
  ez round close
  ez fish <papel> --brief MD
  ez send <pez> --file MD
  ez verify
  ez close
  ez pause --reason TEXTO
  ez wait [--timeout SEGUNDOS]
  ez report --verdict pass|fail --file MD [--blocking N] [--major N] [--minor N]
            [--mutations-tried N] [--mutations-caught N]

Opciones generales:
  --json    Imprime la respuesta completa del árbitro como JSON

Códigos de salida: 0 bien, 1 rechazado, 2 uso incorrecto, 3 sin conexión.
";

enum CliError {
    /// Mensaje para la persona; se acompaña del uso.
    Usage(String),
    /// Archivo que no existe, ya resuelto contra la carpeta actual.
    MissingFile(PathBuf),
}

struct Parsed {
    words: Vec<String>,
    options: Vec<(String, String)>,
    json: bool,
}

impl Parsed {
    fn parse(args: &[String]) -> Result<Parsed, String> {
        let mut parsed = Parsed {
            words: Vec::new(),
            options: Vec::new(),
            json: false,
        };
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--json" {
                parsed.json = true;
                index += 1;
                continue;
            }
            if let Some(name) = arg.strip_prefix("--") {
                let value = args.get(index + 1).filter(|value| !value.starts_with("--"));
                let Some(value) = value else {
                    return Err(format!("Falta el valor de --{name}."));
                };
                parsed.options.push((name.to_string(), value.clone()));
                index += 2;
                continue;
            }
            parsed.words.push(arg.clone());
            index += 1;
        }
        Ok(parsed)
    }

    fn word(&mut self, missing: &str) -> Result<String, CliError> {
        if self.words.is_empty() {
            return Err(CliError::Usage(missing.to_string()));
        }
        Ok(self.words.remove(0))
    }

    fn take(&mut self, name: &str) -> Option<String> {
        let index = self.options.iter().position(|(key, _)| key == name)?;
        Some(self.options.remove(index).1)
    }

    fn required(&mut self, name: &str) -> Result<String, CliError> {
        self.take(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CliError::Usage(format!("Falta --{name}.")))
    }

    fn number(&mut self, name: &str) -> Result<Option<u64>, CliError> {
        let Some(value) = self.take(name) else {
            return Ok(None);
        };
        let number = value
            .trim()
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("--{name} debe ser un número entero.")))?;
        Ok(Some(number))
    }

    fn finish(&mut self, command: &str, args: Value) -> Result<Request, CliError> {
        if let Some(word) = self.words.first() {
            return Err(CliError::Usage(format!("Argumento de más: {word}.")));
        }
        if let Some((name, _)) = self.options.first() {
            return Err(CliError::Usage(format!("Opción desconocida: --{name}.")));
        }
        Ok(Request {
            command: command.to_string(),
            args,
        })
    }
}

fn build(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let command = parsed.word("Falta el comando. Pasa --help para ver el uso.")?;
    match command.as_str() {
        "status" => parsed.finish("status", json!({})),
        "inventory" => {
            // DECISION: `--json` es general para la salida y además viaja en los args.
            let json = parsed.json;
            parsed.finish("inventory", json!({ "json": json }))
        }
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
        "fish" => build_fish(parsed, cwd),
        "send" => build_send(parsed, cwd),
        "report" => build_report(parsed, cwd),
        other => Err(CliError::Usage(format!("Comando desconocido: {other}."))),
    }
}

fn build_task(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
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

fn build_team(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let subcommand = parsed.word("Falta el subcomando de team (propose).")?;
    if subcommand != "propose" {
        return Err(CliError::Usage(format!(
            "Subcomando de team desconocido: {subcommand}."
        )));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    parsed.finish("team.propose", json!({ "file": file }))
}

fn build_round(parsed: &mut Parsed) -> Result<Request, CliError> {
    let subcommand = parsed.word("Falta el subcomando de round (start o close).")?;
    match subcommand.as_str() {
        "start" => parsed.finish("round.start", json!({})),
        "close" => parsed.finish("round.close", json!({})),
        other => Err(CliError::Usage(format!(
            "Subcomando de round desconocido: {other}."
        ))),
    }
}

fn build_fish(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let role = parsed.word("Falta el papel del pez.")?;
    if role.trim().is_empty() {
        return Err(CliError::Usage(
            "El papel del pez no puede estar vacío.".to_string(),
        ));
    }
    let brief = existing(&parsed.required("brief")?, cwd)?;
    parsed.finish("fish.new", json!({ "role": role, "brief": brief }))
}

fn build_send(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let fish = parsed.word("Falta el pez.")?;
    if fish.trim().is_empty() {
        return Err(CliError::Usage(
            "El nombre del pez no puede estar vacío.".to_string(),
        ));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    parsed.finish("fish.send", json!({ "fish": fish, "file": file }))
}

fn build_report(parsed: &mut Parsed, cwd: &Path) -> Result<Request, CliError> {
    let verdict = parsed.required("verdict")?;
    if verdict != "pass" && verdict != "fail" {
        return Err(CliError::Usage(
            "--verdict debe ser pass o fail.".to_string(),
        ));
    }
    let file = existing(&parsed.required("file")?, cwd)?;
    let blocking = parsed.number("blocking")?.unwrap_or(0);
    let major = parsed.number("major")?.unwrap_or(0);
    let minor = parsed.number("minor")?.unwrap_or(0);
    let tried = parsed.number("mutations-tried")?;
    let caught = parsed.number("mutations-caught")?;
    parsed.finish(
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
    let request = match build(&mut parsed, cwd) {
        Ok(request) => request,
        Err(CliError::Usage(detail)) => return usage_error(err, Some(&detail)),
        Err(CliError::MissingFile(path)) => {
            let _ = writeln!(err, "No existe el archivo: {}", path.display());
            return 2;
        }
    };
    let (Some(url), Some(token)) = (non_empty(env("EZ_URL")), non_empty(env("EZ_TOKEN"))) else {
        let _ = writeln!(err, "{MISSING_ENV}");
        return 3;
    };
    let timeout = Duration::from_secs(timeout_seconds(&request));
    match client::send(&url, &token, &request, timeout) {
        Ok(reply) => {
            print_reply(out, &reply, parsed.json);
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

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}
