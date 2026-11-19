//! Agente falso que habla el protocolo de terminal de EzAgent (ECO-8).
//!
//! Corre dentro de una terminal de `hc`, informa su estado con OSC 7501 y
//! obedece una línea inicial: la de un subagente (lee sus instrucciones y
//! entrega su informe con `ez report`) o la del agente principal (ejecuta un
//! guion de comandos `ez`). No necesita modelos de lenguaje.

mod instructions;
mod process;
mod report;
mod script;
mod state;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Uso del binario `fake-ws-agent`.
pub const USAGE: &str = "\
fake-ws-agent: agente falso de EzAgent sobre una terminal de hc.

Uso:
  fake-ws-agent --exec
  fake-ws-agent --verify
  fake-ws-agent --main RUTA_GUION

  --exec    subagente ejecutor: escribe fake/<id>.txt y entrega su informe.
  --verify  subagente verificador: entrega su informe sin tocar el proyecto.
  --main    agente principal: sigue el guion TOML de comandos `ez`.
";

/// Modo de operación del agente falso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Subagente que cambia el proyecto.
    Executor,
    /// Subagente que solo verifica.
    Verifier,
    /// Agente principal con un guion TOML.
    Main(PathBuf),
}

/// Argumentos ya validados del binario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Modo elegido.
    pub mode: Mode,
}

/// Analiza los argumentos de línea de comandos.
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut mode = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--exec" if mode.is_none() => mode = Some(Mode::Executor),
            "--verify" if mode.is_none() => mode = Some(Mode::Verifier),
            "--main" if mode.is_none() => {
                let path = args
                    .get(index + 1)
                    .ok_or_else(|| "--main necesita la ruta del guion.".to_string())?;
                mode = Some(Mode::Main(PathBuf::from(path)));
                index += 1;
            }
            other => return Err(format!("argumento desconocido o repetido: {other}")),
        }
        index += 1;
    }
    let mode = mode.ok_or_else(|| "falta --exec, --verify o --main.".to_string())?;
    Ok(Args { mode })
}

/// Bucle de terminal: anuncia `idle`, atiende líneas y vuelve a `idle`.
/// Ningún candado de E/S queda tomado mientras se ejecuta un subproceso.
pub fn run(args: &[String]) -> Result<(), String> {
    let parsed = parse(args)?;
    announce("idle")?;
    prompt()?;
    while let Some(line) = next_line()? {
        announce("working")?;
        let result = handle(&parsed.mode, &line);
        announce("idle")?;
        prompt()?;
        result?;
    }
    Ok(())
}

fn next_line() -> Result<Option<String>, String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .map_err(|error| format!("no pude leer stdin: {error}"))?;
    Ok((read > 0).then_some(line))
}

fn announce(status: &str) -> Result<(), String> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    state::announce(&mut out, status)
}

fn handle(mode: &Mode, line: &str) -> Result<(), String> {
    match mode {
        Mode::Main(path) if line.starts_with("Eres el agente principal") => script::execute(path),
        Mode::Main(_) => Ok(()),
        Mode::Executor | Mode::Verifier if line.contains("Lee ") && line.contains(" y síguelo") => {
            subagent(mode, line)
        }
        _ => Ok(()),
    }
}

fn subagent(mode: &Mode, first_line: &str) -> Result<(), String> {
    let path = instructions::path_from_first_line(first_line)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("no pude leer {}: {error}", path.display()))?;
    let report_path = instructions::report_path(&text)?;
    if *mode == Mode::Executor {
        let id = instructions::fish_id(&text).unwrap_or_else(|| "dev".to_string());
        let folder = PathBuf::from("fake");
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("no pude crear fake/: {error}"))?;
        let file = folder.join(format!("{id}.txt"));
        std::fs::write(&file, format!("{id} estuvo aquí.\n"))
            .map_err(|error| format!("no pude escribir {}: {error}", file.display()))?;
    }
    report::write(&report_path)?;
    report::send(&report_path, instructions::asks_mutations(&text))
}

fn prompt() -> Result<(), String> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(b"fake listo> ")
        .map_err(|error| format!("no pude escribir el prompt: {error}"))?;
    out.flush()
        .map_err(|error| format!("no pude vaciar stdout: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_mode() {
        assert_eq!(parse(&["--exec".into()]).unwrap().mode, Mode::Executor);
        assert_eq!(parse(&["--verify".into()]).unwrap().mode, Mode::Verifier);
        assert_eq!(
            parse(&["--main".into(), "guion.toml".into()]).unwrap().mode,
            Mode::Main(PathBuf::from("guion.toml"))
        );
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["--exec".into(), "--verify".into()]).is_err());
        assert!(parse(&["--main".into()]).is_err());
        assert!(parse(&["--otro".into()]).is_err());
    }

    #[test]
    fn ignores_lines_that_are_not_for_this_agent() {
        assert!(handle(&Mode::Executor, "hola").is_ok());
        assert!(handle(&Mode::Main(PathBuf::from("x")), "hola").is_ok());
    }
}
