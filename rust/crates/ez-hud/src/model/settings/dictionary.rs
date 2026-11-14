//! Port mínimo de `normalize_rules` de `src/ezagent/voice/stt_dictionary.py`.
//!
//! `settings.py` solo necesita validar y reescribir el diccionario editable;
//! el resto del módulo de STT vive en `ez-stt`, que este crate no declara.

use std::collections::HashSet;

use ez_core::pyvalue::{casefold, collapse_spaces, py_strip};

const MAX_CHARS: usize = 12_000;
const MAX_RULES: usize = 100;

const TOO_LONG: &str = "El diccionario admite hasta 12000 caracteres.";
const MALFORMED: &str = "Usa origen → corrección; separa las reglas con punto y coma.";
const DUPLICATE: &str = "Cada expresión de origen debe aparecer una sola vez.";
const TOO_MANY: &str = "El diccionario admite hasta 100 correcciones.";

/// Valida las reglas y devuelve la representación editable de una línea.
pub(crate) fn normalize_rules(value: &str) -> Result<String, String> {
    if value.chars().count() > MAX_CHARS {
        return Err(TOO_LONG.into());
    }
    let mut rules: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in value.split([';', '\r', '\n']) {
        if py_strip(line).is_empty() {
            continue;
        }
        let Some((source, target)) = split_arrow(line) else {
            return Err(MALFORMED.into());
        };
        if py_strip(source).is_empty() || py_strip(target).is_empty() {
            return Err(MALFORMED.into());
        }
        let source = collapse_spaces(source);
        let target = collapse_spaces(target);
        if !seen.insert(casefold(&source)) {
            return Err(DUPLICATE.into());
        }
        rules.push((source, target));
        if rules.len() > MAX_RULES {
            return Err(TOO_MANY.into());
        }
    }
    Ok(rules
        .iter()
        .map(|(source, target)| format!("{source} → {target}"))
        .collect::<Vec<_>>()
        .join("; "))
}

/// `re.split(r"→|=>", line, maxsplit=1)`: el primer separador de la línea.
fn split_arrow(line: &str) -> Option<(&str, &str)> {
    let arrow = line.find('→');
    let fat = line.find("=>");
    let (index, width) = match (arrow, fat) {
        (Some(arrow), Some(fat)) if arrow < fat => (arrow, '→'.len_utf8()),
        (Some(_), Some(fat)) => (fat, 2),
        (Some(arrow), None) => (arrow, '→'.len_utf8()),
        (None, Some(fat)) => (fat, 2),
        (None, None) => return None,
    };
    Some((&line[..index], &line[index + width..]))
}
