//! Lectura de la pantalla de la terminal y regex de sus diálogos y pasos.

use std::sync::OnceLock;

use ez_core::pyvalue::{py_splitlines, py_strip, python_str};
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value};

use crate::herdr::value_or_text;

use super::text::compiled;

pub(super) fn ps_prompt() -> &'static Regex {
    static PROMPT: OnceLock<Regex> = OnceLock::new();
    compiled(&PROMPT, r"^PS [^>]*>")
}

pub(super) fn blocking_dialog() -> &'static Regex {
    static DIALOG: OnceLock<Regex> = OnceLock::new();
    DIALOG.get_or_init(|| {
        RegexBuilder::new(
            r"(?i)(do you trust|trust this (folder|directory|project)|yes, i trust|quick safety check)",
        )
        .build()
        .expect("patrón fijo válido")
    })
}

pub(super) fn prompt_glyph() -> &'static Regex {
    static GLYPH: OnceLock<Regex> = OnceLock::new();
    compiled(&GLYPH, r"^[›❯>]\s?")
}

/// `re.search(patrón, texto, re.I)`; un patrón inválido no coincide.
///
/// DECISION: el Python propagaría `re.error` (el perfil es nuestro y está bien
/// formado); aquí un patrón roto solo deja de coincidir para no caer en pánico.
pub(super) fn search(pattern: &str, text: &str) -> bool {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map(|regex| regex.is_match(text))
        .unwrap_or(false)
}

pub(super) fn screen_lines(screen: &str) -> Vec<String> {
    py_splitlines(screen)
        .into_iter()
        .map(py_strip)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn step_text(step: &Map<String, Value>, key: &str, fallback: &str) -> String {
    value_or_text(step.get(key), fallback)
}

pub(super) fn key_list(value: Option<&Value>) -> Option<Vec<String>> {
    let items = value?.as_array()?;
    Some(items.iter().map(python_str).collect())
}

pub(super) fn truthy_key(row: &Map<String, Value>, key: &str) -> bool {
    row.get(key).is_some_and(ez_core::pyvalue::truthy)
}
