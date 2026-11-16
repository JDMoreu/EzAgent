//! Arnés de compatibilidad de datos (F7-03).
//!
//! `parity/compat/run_compat.py` lo usa para leer y escribir los artefactos del
//! perfil por las APIs reales del port (no por SQL a mano) y comparar el
//! resultado con el Python del `.venv`. No es parte de la app: no imprime nada
//! (el resultado va al archivo que indica `--out`) y no se instala.
//!
//! Comandos: `dump-*` leen, `save-*` escriben en la copia temporal del perfil,
//! `append-logs` añade una línea de auditoría y otra de latencia, y `serve`
//! levanta el servidor del enjambre para las pruebas de enjambre mixto.

#[path = "ezagent_compat/artifacts.rs"]
mod artifacts;
#[path = "ezagent_compat/serve.rs"]
mod serve;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Value};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let command = argv.get(1).cloned().unwrap_or_default();
    let flags = parse_flags(&argv[2..]);
    let Some(out) = flags.get("out").map(PathBuf::from) else {
        std::process::exit(2);
    };
    if command == "serve" {
        let code = if serve::run(&flags, &out).is_ok() {
            0
        } else {
            1
        };
        std::process::exit(code);
    }
    let written = match artifacts::run(&command, &flags) {
        Ok(value) => write_json(&out, &value),
        Err(error) => {
            let _ = write_json(&out, &json!({ "error": error }));
            Err(())
        }
    };
    std::process::exit(if written.is_ok() { 0 } else { 1 });
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<(), ()> {
    let text = serde_json::to_vec(value).map_err(|_| ())?;
    std::fs::write(path, text).map_err(|_| ())
}

/// Pares `--nombre valor`; un flag sin valor vale `""`.
fn parse_flags(args: &[String]) -> BTreeMap<String, String> {
    let mut flags = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let Some(name) = args[index].strip_prefix("--") else {
            index += 1;
            continue;
        };
        let value = args.get(index + 1).cloned().unwrap_or_default();
        flags.insert(name.to_string(), value);
        index += 2;
    }
    flags
}
