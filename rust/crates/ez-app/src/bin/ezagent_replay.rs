//! CLI del arnés de reproducción (`--scenario`, `--all`, `--trace-out`, `--list`).
//!
//! Como `parity/replay/run_py.py`, cada escenario corre en su propio proceso:
//! el perfil temporal y los servicios falsos no se mezclan entre corridas.
//! `--verify` (dos corridas por escenario, byte a byte) también es del Python.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use ez_app::replay::{run_path, select, write_trace, Paths};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(std::io::stdout(), "{error}");
            ExitCode::from(1)
        }
    }
}

/// Opciones de la línea de comandos.
struct Options {
    selection: Vec<String>,
    all: bool,
    verify: bool,
    list: bool,
    trace_out: Option<PathBuf>,
    /// Minutos de la prueba de fondo (F7-05): repite los escenarios en un
    /// proceso y pinta el HUD fuera de pantalla.
    soak: Option<f64>,
}

fn run() -> Result<u8, String> {
    let options = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())?;
    let paths = Paths::discover()?;
    if options.list {
        for (stem, name) in ez_app::replay::list(&paths)? {
            let _ = writeln!(std::io::stdout(), "{stem}: {name}");
        }
        return Ok(0);
    }
    let selection = if options.all {
        Vec::new()
    } else {
        options.selection
    };
    let scenarios = select(&paths, &selection)?;
    if scenarios.is_empty() {
        return Err("no hay escenarios".to_string());
    }
    if let Some(minutes) = options.soak {
        return ez_app::replay::soak::run(&paths, &scenarios, minutes);
    }
    if options.verify {
        return verify_all(&scenarios);
    }
    if options.trace_out.is_some() && scenarios.len() != 1 {
        return Err("--trace-out solo vale con un escenario".to_string());
    }
    if scenarios.len() > 1 {
        return run_each(&scenarios);
    }
    run_one(&paths, &scenarios[0], options.trace_out.as_deref())
}

/// Argumentos del CLI (`--help` imprime y sale con 0).
fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        selection: Vec::new(),
        all: false,
        verify: false,
        list: false,
        trace_out: None,
        soak: None,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--scenario" => {
                options.selection.push(value(args, index, "--scenario")?);
                index += 2;
            }
            "--trace-out" => {
                options.trace_out = Some(PathBuf::from(value(args, index, "--trace-out")?));
                index += 2;
            }
            "--all" => {
                options.all = true;
                index += 1;
            }
            "--verify" => {
                options.verify = true;
                index += 1;
            }
            "--soak" => {
                let raw = value(args, index, "--soak")?;
                options.soak = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("--soak espera minutos: {raw}"))?,
                );
                index += 2;
            }
            "--list" => {
                options.list = true;
                index += 1;
            }
            "--help" | "-h" => {
                let _ = writeln!(std::io::stdout(), "{USAGE}");
                index += 1;
            }
            other => {
                return Err(format!(
                    "argumento desconocido: {other}
{USAGE}"
                ))
            }
        }
    }
    Ok(options)
}

fn value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("{flag} necesita un valor"))
}

fn run_one(
    paths: &Paths,
    path: &std::path::Path,
    trace_out: Option<&std::path::Path>,
) -> Result<u8, String> {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    match run_path(path, paths, trace_out) {
        Ok(outcome) => {
            let status = if outcome.errors.is_empty() {
                "ok".to_string()
            } else {
                "FALLOS".to_string()
            };
            let mut stdout = std::io::stdout();
            let _ = writeln!(
                stdout,
                "{stem}: {status} ({} líneas, {:.1} s) -> {}",
                outcome.lines.len(),
                outcome.seconds,
                outcome.trace_path.display()
            );
            for error in &outcome.errors {
                let _ = writeln!(stdout, "  - {error}");
            }
            Ok(u8::from(!outcome.errors.is_empty()))
        }
        Err(error) => Err(format!("{stem}: ERROR {error}")),
    }
}

/// Un proceso por escenario, como `--all` del Python.
fn run_each(scenarios: &[PathBuf]) -> Result<u8, String> {
    let exe = std::env::current_exe().map_err(|error| format!("sin ejecutable: {error}"))?;
    let mut failures = 0;
    for path in scenarios {
        let stem = path
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let status = Command::new(&exe)
            .arg("--scenario")
            .arg(&stem)
            .status()
            .map_err(|error| format!("no pude lanzar {stem}: {error}"))?;
        if !status.success() {
            failures += 1;
        }
    }
    Ok(u8::from(failures > 0))
}

/// Dos corridas por escenario, en procesos distintos, byte a byte.
fn verify_all(scenarios: &[PathBuf]) -> Result<u8, String> {
    let exe = std::env::current_exe().map_err(|error| format!("sin ejecutable: {error}"))?;
    let tmp = std::env::temp_dir().join(format!("ezreplay-verify-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|error| format!("sin carpeta temporal: {error}"))?;
    let mut failures = 0;
    for path in scenarios {
        let stem = path
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let mut traces: Vec<Vec<u8>> = Vec::new();
        for run in [1, 2] {
            let out = tmp.join(format!("{stem}-{run}.jsonl"));
            let status = Command::new(&exe)
                .arg("--scenario")
                .arg(&stem)
                .arg("--trace-out")
                .arg(&out)
                .status()
                .map_err(|error| format!("no pude lanzar {stem}: {error}"))?;
            if !status.success() || !out.exists() {
                let _ = writeln!(std::io::stdout(), "{stem}: la corrida {run} falló");
                failures += 1;
                break;
            }
            traces.push(std::fs::read(&out).map_err(|error| format!("sin traza: {error}"))?);
        }
        if traces.len() == 2 {
            let twin = traces[0] == traces[1];
            let _ = writeln!(
                std::io::stdout(),
                "{stem}: {}",
                if twin { "idéntica" } else { "DISTINTA" }
            );
            if !twin {
                failures += 1;
            }
        }
    }
    let _ = write_trace;
    Ok(u8::from(failures > 0))
}

const USAGE: &str =
    "Uso: ezagent-replay [--scenario <id>]... [--all] [--trace-out <ruta>] [--verify] [--soak <minutos>] [--list]";
