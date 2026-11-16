//! Auditoría sin pantallas (§10.15): tipo, opción, primer token y su hash,
//! ruta redactada y decisión. Nunca se guarda el texto de la pantalla.

use std::fmt::Write as _;

use ez_core::pyvalue::{head_chars, py_split};
use ez_core::redact::redact;
use serde_json::{Map, Value};

use super::dialogs::positive_option;
use super::{Decision, Dialog, DialogKind};

const OPTION_CHARS: usize = 60;
const HASH_CHARS: usize = 16;

fn kind_label(kind: DialogKind) -> &'static str {
    match kind {
        DialogKind::Exec => "exec",
        DialogKind::Edit => "edit",
        DialogKind::Trust => "trust",
        DialogKind::Other => "other",
        DialogKind::None => "none",
    }
}

/// Texto de la opción que se aprobó, si la decisión la eligió.
fn chosen_text(dialog: &Dialog) -> Option<&str> {
    let first = dialog.approve_once.as_ref()?.first()?;
    if !matches!(first.as_str(), "up" | "down" | "enter") {
        if let Some((_, text)) = dialog.options.iter().find(|(key, _)| key == first) {
            return Some(text.as_str());
        }
    }
    dialog
        .options
        .iter()
        .map(|(_, text)| text.as_str())
        .find(|text| positive_option(text))
}

fn option_value(dialog: &Dialog, decision: &Decision) -> Value {
    if !matches!(decision, Decision::Approve(_)) {
        return Value::Null;
    }
    chosen_text(dialog)
        .map(|text| Value::String(head_chars(&redact(text), OPTION_CHARS).to_string()))
        .unwrap_or(Value::Null)
}

fn command_value(command: Option<&str>) -> Value {
    command
        .and_then(|command| py_split(command).next())
        .map(|token| Value::String(redact(token)))
        .unwrap_or(Value::Null)
}

fn hash_value(command: Option<&str>) -> Value {
    command
        .map(|command| Value::String(hash16(command)))
        .unwrap_or(Value::Null)
}

fn path_value(dialog: &Dialog) -> Value {
    dialog
        .path
        .as_deref()
        .map(|path| Value::String(redact(path)))
        .unwrap_or(Value::Null)
}

fn decision_label(decision: &Decision) -> String {
    match decision {
        Decision::Approve(sequence) => format!("approve:{}", sequence.join(",")),
        Decision::Ask(reason) => format!("ask:{reason}"),
        Decision::Ignore => "ignore".to_string(),
    }
}

/// Resumen de una sola línea que va a `bitacora.jsonl` y a la auditoría.
///
/// DECISION: se devuelve JSON porque la bitácora es JSONL y así el campo
/// `detail` lleva exactamente estos datos, sin extracto de pantalla.
pub fn audit_summary(dialog: &Dialog, decision: &Decision) -> String {
    let mut row = Map::new();
    row.insert(
        "kind".to_string(),
        Value::String(kind_label(dialog.kind).to_string()),
    );
    row.insert("option".to_string(), option_value(dialog, decision));
    row.insert(
        "command".to_string(),
        command_value(dialog.command.as_deref()),
    );
    row.insert(
        "command_hash".to_string(),
        hash_value(dialog.command.as_deref()),
    );
    row.insert("path".to_string(), path_value(dialog));
    row.insert(
        "decision".to_string(),
        Value::String(decision_label(decision)),
    );
    Value::Object(row).to_string()
}

/// Primeros 16 hex del SHA-256 del texto.
fn hash16(text: &str) -> String {
    sha256(text.as_bytes()).iter().take(HASH_CHARS / 2).fold(
        String::with_capacity(HASH_CHARS),
        |mut acc, byte| {
            write!(&mut acc, "{byte:02x}").expect("escribir en un String no falla");
            acc
        },
    )
}

/// SHA-256 del crate `sha2` del workspace.
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

#[cfg(test)]
pub(crate) fn sha256_hex(text: &str) -> String {
    sha256(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            write!(&mut acc, "{byte:02x}").expect("escribir en un String no falla");
            acc
        })
}
