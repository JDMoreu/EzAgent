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

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const ROUND: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// DECISION: `sha2` no es dependencia de `ez-herdr` y no se puede añadir; el
/// diseño pide SHA-256 de 16 hex, así que se implementa aquí (sin dependencias).
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = INITIAL;
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for chunk in message.as_chunks::<64>().0 {
        state = compress(state, chunk);
    }
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn compress(mut state: [u32; 8], chunk: &[u8]) -> [u32; 8] {
    let mut words = [0u32; 64];
    for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
        words[index] = u32::from_be_bytes(*bytes);
    }
    for index in 16..64 {
        let first = words[index - 15];
        let second = words[index - 2];
        let s0 = first.rotate_right(7) ^ first.rotate_right(18) ^ (first >> 3);
        let s1 = second.rotate_right(17) ^ second.rotate_right(19) ^ (second >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
    for index in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(ROUND[index])
            .wrapping_add(words[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
    state
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
