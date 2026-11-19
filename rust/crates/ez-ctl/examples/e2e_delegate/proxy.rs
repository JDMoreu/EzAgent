//! El lanzador: el ejemplo hace de `ez` para el agente falso.
//!
//! El agente falso entrega su estado con `ez report`. Como `hc run` no pasa
//! el entorno al hijo (`hc run` no tiene `--env`), el subagente no puede
//! saber dónde está el registro de subagentes ni qué `hc` usar. El lanzador
//! resuelve eso: es el mismo binario `ez`, ejecutado con el entorno temporal
//! de la prueba.
//!
//! El lanzador se usa solo cuando el primer argumento es un comando de `ez`
//! (`report`); el resto de las veces el ejemplo corre la prueba.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::proc::{self, EnvVars};

/// Los comandos de `ez` que el lanzador reconoce.
const COMMANDS: [&str; 4] = ["report", "validate", "tasks", "watch"];

/// Si el ejemplo está acting como lanzador, corre `ez` y devuelve su código.
///
/// `None` si no es el modo lanzador (entonces el ejemplo corre la prueba).
pub fn maybe_run(args: &[String]) -> Option<i32> {
    let command = args.get(1)?;
    if !COMMANDS.contains(&command.as_str()) {
        return None;
    }
    Some(run_ez(&args[1..]))
}

/// Corre `ez` con el entorno de la prueba.
fn run_ez(args: &[String]) -> i32 {
    let Some(config) = read_config() else {
        eprintln!("[EzAgent · aviso] el lanzador no encontró su configuración.");
        return 1;
    };
    let Some(ez) = config.get("ez").and_then(Value::as_str).map(PathBuf::from) else {
        eprintln!("[EzAgent · aviso] el lanzador no sabe dónde está `ez`.");
        return 1;
    };
    let mut env = EnvVars::default();
    if let Some(Value::Object(pairs)) = config.get("env") {
        for (name, value) in pairs {
            if let Some(value) = value.as_str() {
                env.set(name, value);
            }
        }
    }
    env.remove("EZ_URL");
    env.remove("EZ_TOKEN");
    let output = proc::run(&ez, args, &env, None);
    // DECISION: si el registro del subagente todavía no existe (el comando
    // `ez delegate` lo escribe después de entregar el encargo), se reintenta
    // unos segundos: el subagente declara su estado nada más recibirlo.
    if output.code == 1 && output.message().contains("no tiene una tarea de EzAgent") {
        return retry(&ez, args, &env, output);
    }
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    output.code
}

/// Reintenta `ez report` mientras el registro del subagente no exista.
fn retry(ez: &Path, args: &[String], env: &EnvVars, first: proc::Output) -> i32 {
    let mut output = first;
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        output = proc::run(ez, args, env, None);
        if output.code != 1 || !output.message().contains("no tiene una tarea de EzAgent") {
            break;
        }
    }
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    output.code
}

/// La configuración del lanzador (`proxy.json`, en la carpeta de la prueba).
fn read_config() -> Option<Value> {
    let folder = std::env::current_dir().ok()?;
    let mut candidates = vec![folder.join("proxy.json")];
    if let Some(parent) = folder.parent() {
        candidates.push(parent.join("proxy.json"));
    }
    for candidate in candidates {
        let Ok(text) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            return Some(value);
        }
    }
    None
}
