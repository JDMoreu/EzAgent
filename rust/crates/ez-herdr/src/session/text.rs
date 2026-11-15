//! Línea de comandos de PowerShell y teclas nombradas.
//!
//! DECISION: `command_line` y `valid_keys` son copias literales del Python
//! porque las de `herdr::text` (F4-10) son privadas al módulo `herdr` y el
//! brief de F4-11 no permite tocar `herdr.rs`.

use std::sync::OnceLock;

use ez_core::pyvalue::{py_strip, python_str};
use regex::Regex;
use serde_json::{Map, Value};

/// Teclas nombradas que acepta un paso.
pub const MAX_KEYS: usize = 12;

pub(super) fn compiled(slot: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    slot.get_or_init(|| Regex::new(pattern).expect("patrón fijo válido"))
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn plain_argument() -> &'static Regex {
    static PLAIN: OnceLock<Regex> = OnceLock::new();
    // DECISION: `$` de Python deja pasar un salto de línea final; aquí se cita
    // de más, que solo agrega comillas seguras.
    compiled(&PLAIN, r"^[A-Za-z0-9_./:=@%+,\\-]+$")
}

fn key_pattern() -> &'static Regex {
    static KEY: OnceLock<Regex> = OnceLock::new();
    compiled(
        &KEY,
        r"^(?:(?:ctrl|alt|shift)\+)*(?:[a-z0-9]|enter|esc|tab|up|down|left|right|home|end|pageup|pagedown|backspace|space|f(?:[1-9]|1[0-2]))$",
    )
}

/// Línea que se escribe en PowerShell; cita lo que no sea un argumento simple.
///
/// `env` se antepone como `$env:NOMBRE='valor'; ` (solo vive en esa shell).
/// `path_prepend` añade carpetas al principio del PATH de esa shell.
pub fn command_line(
    argv: &[String],
    env: &Map<String, Value>,
    path_prepend: Option<&[String]>,
) -> String {
    let mut prefix = String::new();
    for (name, value) in env {
        prefix.push_str(&format!("$env:{name}={}; ", ps_quote(&python_str(value))));
    }
    for folder in path_prepend.unwrap_or(&[]) {
        prefix.push_str(&format!(
            "$env:PATH={}+$env:PATH; ",
            ps_quote(&format!("{folder};"))
        ));
    }
    let parts: Vec<String> = argv
        .iter()
        .map(|part| {
            if plain_argument().is_match(part) {
                part.clone()
            } else {
                ps_quote(part)
            }
        })
        .collect();
    prefix + &parts.join(" ")
}

/// Teclas nombradas (enter, esc, ctrl+t...) o None si alguna no es válida.
pub fn valid_keys(keys: &[String]) -> Option<Vec<String>> {
    if keys.is_empty() || keys.len() > MAX_KEYS {
        return None;
    }
    let cleaned: Vec<String> = keys
        .iter()
        .map(|key| py_strip(key).to_lowercase())
        .collect();
    if cleaned.iter().all(|key| key_pattern().is_match(key)) {
        Some(cleaned)
    } else {
        None
    }
}

/// `valid_keys` con el valor tal cual llega del JSON (lista o cualquier cosa).
#[doc(hidden)]
pub fn valid_keys_value(keys: &Value) -> Option<Vec<String>> {
    let items = keys.as_array()?;
    if items.is_empty() || items.len() > MAX_KEYS {
        return None;
    }
    let cleaned: Vec<String> = items
        .iter()
        .map(|item| py_strip(&python_str(item)).to_lowercase())
        .collect();
    if cleaned.iter().all(|key| key_pattern().is_match(key)) {
        Some(cleaned)
    } else {
        None
    }
}
