//! Port of `src/ezagent/agent/memory.py 1-562` (F3-09a).
//!
//! Constantes, alias, normalización de texto, detección de secretos y el
//! resultado uniforme de las operaciones de memoria.

use std::sync::OnceLock;

use ez_core::pyvalue::{collapse_spaces, py_rstrip, py_strip};
use regex::Regex;
use serde_json::{json, Map, Value};

pub const USER_LIMIT: usize = 1375;
pub const MEMORY_LIMIT: usize = 2200;
pub const MAX_ENTRY_CHARS: usize = 200;
pub const RECALL_FRAGMENT_CHARS: usize = 300;
pub const RETENTION_DAYS: i64 = 180;
pub const DEFAULT_RECALL_LIMIT: i64 = 5;

pub fn kind_file(kind: &str) -> Option<&'static str> {
    match kind {
        "user" => Some("USER.md"),
        "memory" => Some("MEMORY.md"),
        _ => None,
    }
}

pub fn kind_limit(kind: &str) -> Option<usize> {
    match kind {
        "user" => Some(USER_LIMIT),
        "memory" => Some(MEMORY_LIMIT),
        _ => None,
    }
}

/// Acepta `user`/`memory` y sus alias en español; `None` si no vale.
pub fn normalize_kind(kind: &str) -> Option<&'static str> {
    match py_strip(kind).to_lowercase().as_str() {
        "user" | "usuario" | "perfil" | "user.md" | "user_md" => Some("user"),
        "memory" | "memoria" | "hechos" | "memory.md" | "memory_md" => Some("memory"),
        _ => None,
    }
}

fn fold_char(character: char) -> char {
    match character {
        'á' | 'à' | 'Á' | 'À' => 'a',
        'é' | 'è' | 'É' | 'È' => 'e',
        'í' | 'ì' | 'ï' | 'Í' | 'Ì' | 'Ï' => 'i',
        'ó' | 'ò' | 'Ó' | 'Ò' => 'o',
        'ú' | 'ù' | 'ü' | 'Ú' | 'Ù' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ë' | 'Ë' => 'e',
        other => other,
    }
}

/// Minúsculas y sin acentos, conservando la longitud del texto.
pub fn fold_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| fold_char(character).to_lowercase())
        .collect()
}

/// Normaliza un dato de memoria a una sola línea con espacios simples.
pub fn normalize_entry(text: &str) -> String {
    collapse_spaces(text)
}

/// Normaliza para comparar duplicados (sin mayúsculas, acentos ni punto final).
pub fn normalize_for_compare(value: &str) -> String {
    let mut folded = py_strip(&fold_text(&normalize_entry(value))).to_string();
    while let Some(last) = folded.chars().next_back() {
        if !".,;:!?¿¡".contains(last) {
            break;
        }
        let cut = folded.len() - last.len_utf8();
        folded = py_rstrip(&folded[..cut]).to_string();
    }
    folded
}

/// Recorta un texto para mensajes y resúmenes de confirmación.
pub fn summarize_text(value: &str, limit: usize) -> String {
    let text = normalize_entry(value);
    let length = text.chars().count();
    if length <= limit {
        return text;
    }
    let kept: String = py_rstrip(
        &text
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>(),
    )
    .to_string();
    format!("{kept}…")
}

struct Patterns {
    bullet: Regex,
    token: Regex,
    digit: Regex,
    password_word: Regex,
    assigned_secret: Regex,
    key_formats: [(Regex, &'static str); 9],
    card_sequence: Regex,
    card_context: Regex,
    dni: Regex,
    nie: Regex,
    id_context: Regex,
    iban: Regex,
    url: Regex,
    md_link: Regex,
    domain: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        bullet: Regex::new(r"^\s*-\s*\[(\d{4}-\d{2}-\d{2})\]\s*(.*)$").expect("patrón válido"),
        token: Regex::new(r"[^\W_]+").expect("patrón válido"),
        digit: Regex::new(r"^\d$").expect("patrón válido"),
        password_word: Regex::new(r"(?i)\b(?:contraseñas?|contrasenas?|passwords?|passwd|pwd)")
            .expect("patrón válido"),
        assigned_secret: Regex::new(
            r"(?i)\b(?:(?:api[\s_-]?keys?|tokens?|secrets?|secretos?|claves?)\s*(?:es|son|:|=|=>)\s*\S+|claves?\s+de\s+(?:api|acceso|seguridad|wifi)\b|(?:pass|pin)\s*[:=]\s*\S+)",
        )
        .expect("patrón válido"),
        key_formats: [
            (
                Regex::new(r"\bsk-[A-Za-z0-9_-]{8,}\b").expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}\b")
                    .expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b")
                    .expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"(?i)\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{8,}").expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}")
                    .expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").expect("patrón válido"),
                "claves o tokens",
            ),
            (
                Regex::new(r"\bAIza[0-9A-Za-z_-]{30,}\b").expect("patrón válido"),
                "claves o tokens",
            ),
        ],
        card_sequence: Regex::new(r"(?:\d[ -]?){12,18}\d").expect("patrón válido"),
        card_context: Regex::new(r"(?i)\btarjetas?\b[^\n]{0,40}?(?:\d[ -]?){12,18}\d")
            .expect("patrón válido"),
        dni: Regex::new(r"\d{8}[A-Za-z]").expect("patrón válido"),
        nie: Regex::new(r"[XYZxyz]\d{7}[A-Za-z]").expect("patrón válido"),
        id_context: Regex::new(
            r"(?i)\b(?:dni|nie|documento(?: nacional)? de identidad|c[eé]dula|pasaporte|passport)\b[^\n]{0,30}?\b[A-Z]{0,2}\d{6,9}[A-Za-z]?\b",
        )
        .expect("patrón válido"),
        iban: Regex::new(
            r"(?i)\b[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]{4}){2,7}(?:[ ]?[A-Z0-9]{1,3})?\b",
        )
        .expect("patrón válido"),
        url: Regex::new(r"(?i)(?:https?://|ftp://|\bwww\.)\S+").expect("patrón válido"),
        md_link: Regex::new(r"(?i)\[[^\]]*\]\((?:https?://|www\.)[^)]+\)")
            .expect("patrón válido"),
        domain: Regex::new(
            r"(?i)\b(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:com|org|net|io|es|co|gov|edu|info|dev|ai)\b",
        )
        .expect("patrón válido"),
    })
}

fn is_digit(character: char) -> bool {
    patterns().digit.is_match(&character.to_string())
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// `(?<!\d)` o `(?<!\w)`: el regex de Rust no admite lookbehind.
fn left_bounded(text: &str, start: usize, word: bool) -> bool {
    match text[..start].chars().next_back() {
        None => true,
        Some(character) if word => !is_word(character),
        Some(character) => !is_digit(character),
    }
}

fn right_bounded(text: &str, end: usize, word: bool) -> bool {
    match text[end..].chars().next() {
        None => true,
        Some(character) if word => !is_word(character),
        Some(character) => !is_digit(character),
    }
}

fn has_word_bounded(pattern: &Regex, text: &str) -> bool {
    pattern.find_iter(text).any(|found| {
        left_bounded(text, found.start(), true) && right_bounded(text, found.end(), true)
    })
}

fn luhn_ok(digits: &[u32]) -> bool {
    let mut total = 0;
    for (index, number) in digits.iter().rev().enumerate() {
        let mut number = *number;
        if index % 2 == 1 {
            number *= 2;
            if number > 9 {
                number -= 9;
            }
        }
        total += number;
    }
    total % 10 == 0
}

fn has_card_number(text: &str) -> bool {
    let found = patterns().card_sequence.find_iter(text).any(|candidate| {
        if !left_bounded(text, candidate.start(), false)
            || !right_bounded(text, candidate.end(), false)
        {
            return false;
        }
        let digits: Vec<u32> = candidate
            .as_str()
            .chars()
            .filter_map(|character| character.to_digit(10))
            .collect();
        (13..=19).contains(&digits.len()) && luhn_ok(&digits)
    });
    found || patterns().card_context.is_match(text)
}

/// Heurística: URLs, dominios o enlaces Markdown propios de contenido web.
pub fn contains_web_content(text: &str) -> bool {
    let current = patterns();
    current.url.is_match(text) || current.md_link.is_match(text) || current.domain.is_match(text)
}

/// Devuelve la categoría sensible detectada, o `None` si no hay ninguna.
pub fn find_sensitive_text(text: &str) -> Option<&'static str> {
    let current = patterns();
    if current.password_word.is_match(text) {
        return Some("contraseñas");
    }
    if current.assigned_secret.is_match(text) {
        return Some("claves o tokens");
    }
    if current
        .key_formats
        .iter()
        .any(|(pattern, _)| pattern.is_match(text))
    {
        return Some("claves o tokens");
    }
    if has_card_number(text) {
        return Some("números de tarjeta");
    }
    if has_word_bounded(&current.dni, text)
        || has_word_bounded(&current.nie, text)
        || current.id_context.is_match(text)
    {
        return Some("documentos de identidad");
    }
    if current.iban.is_match(text) {
        return Some("datos bancarios (IBAN)");
    }
    None
}

/// `_BULLET_RE`: fecha y cuerpo de una viñeta.
pub(crate) fn bullet_capture(line: &str) -> Option<(&str, &str)> {
    let captures = patterns().bullet.captures(line)?;
    let date = captures.get(1)?.as_str();
    let body = captures.get(2).map_or("", |found| found.as_str());
    Some((date, body))
}

/// `_TOKEN_RE.findall(text)`: palabras, sin guiones bajos ni puntuación.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    patterns()
        .token
        .find_iter(text)
        .map(|found| found.as_str().to_string())
        .collect()
}

fn push<T>(data: &mut Map<String, Value>, key: &str, value: T)
where
    T: Into<Value>,
{
    data.insert(key.to_string(), value.into());
}

/// Resultado uniforme de las operaciones de memoria.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryResult {
    pub status: String,
    pub message: String,
    pub kind: Option<String>,
    pub text: Option<String>,
    pub proposal: Option<String>,
    pub removed: i64,
    pub files: Vec<String>,
    pub backup: Option<String>,
    pub line_count: i64,
}

impl MemoryResult {
    pub fn new(status: &str, message: &str) -> Self {
        Self {
            status: status.to_string(),
            message: message.to_string(),
            ..Self::default()
        }
    }

    pub fn saved(&self) -> bool {
        self.status == "saved"
    }

    pub fn to_dict(&self) -> Value {
        let mut data = Map::new();
        push(&mut data, "status", self.status.clone());
        push(&mut data, "message", self.message.clone());
        if let Some(kind) = &self.kind {
            push(&mut data, "kind", kind.clone());
        }
        if let Some(text) = &self.text {
            push(&mut data, "text", text.clone());
        }
        if let Some(proposal) = &self.proposal {
            push(&mut data, "proposal", proposal.clone());
        }
        if self.removed != 0 {
            push(&mut data, "removed", self.removed);
        }
        if !self.files.is_empty() {
            push(&mut data, "files", json!(self.files));
        }
        if let Some(backup) = &self.backup {
            push(&mut data, "backup", backup.clone());
        }
        if self.line_count != 0 {
            push(&mut data, "line_count", self.line_count);
        }
        Value::Object(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_and_lowercase() {
        assert_eq!(fold_text("MAÑANA café"), "manana cafe");
        assert_eq!(normalize_entry("  hola \n  mundo "), "hola mundo");
        assert_eq!(normalize_for_compare("Hola, mundo."), "hola, mundo");
        assert_eq!(summarize_text("abcdefghij", 5), "abcd…");
        assert_eq!(summarize_text("abc", 5), "abc");
    }

    #[test]
    fn normalizes_kind_aliases() {
        assert_eq!(normalize_kind(" Usuario "), Some("user"));
        assert_eq!(normalize_kind("MEMORY.md"), Some("memory"));
        assert_eq!(normalize_kind("otro"), None);
    }

    #[test]
    fn detects_sensitive_categories() {
        assert_eq!(
            find_sensitive_text("mi contraseña es 1"),
            Some("contraseñas")
        );
        assert_eq!(
            find_sensitive_text("token: abc123"),
            Some("claves o tokens")
        );
        assert_eq!(
            find_sensitive_text("sk-abcdefgh12345678"),
            Some("claves o tokens")
        );
        assert_eq!(
            find_sensitive_text("tarjeta 4111 1111 1111 1111"),
            Some("números de tarjeta")
        );
        assert_eq!(
            find_sensitive_text("mi DNI 12345678Z"),
            Some("documentos de identidad")
        );
        assert_eq!(
            find_sensitive_text("IBAN ES91 2100 0418 4502 0005 1332"),
            Some("datos bancarios (IBAN)")
        );
        assert_eq!(find_sensitive_text("prefiero tokens cortos"), None);
    }

    #[test]
    fn detects_web_content() {
        assert!(contains_web_content("mira https://example.com/x"));
        assert!(contains_web_content("mira example.org"));
        assert!(contains_web_content("[aquí](https://example.com)"));
        assert!(!contains_web_content("reunión en la oficina"));
    }

    #[test]
    fn result_dict_skips_empty_fields() {
        let result = MemoryResult {
            status: "saved".into(),
            message: "Guardado.".into(),
            kind: Some("user".into()),
            ..MemoryResult::default()
        };
        assert_eq!(
            result.to_dict(),
            json!({"status": "saved", "message": "Guardado.", "kind": "user"})
        );
        assert!(MemoryResult::new("saved", "x").saved());
    }

    #[test]
    fn bullet_capture_requires_date() {
        assert_eq!(
            bullet_capture("- [2026-01-02] hola"),
            Some(("2026-01-02", "hola"))
        );
        assert_eq!(bullet_capture("- sin fecha"), None);
    }
}
