//! Helpers de valores y cadenas de Python que comparten todos los crates.
//!
//! Son los que el contrato (`AGENTS.rust.md` §9) pide portar con exactitud:
//! `bool(x)`, `str(x)`, `str.strip()`, `str.split()` y los cortes por carácter.

pub use crate::config::{int_value, python_int, python_str, truthy};

/// `str.isspace()` por carácter: el espacio Unicode más `\x1c`-`\x1f`.
pub fn is_py_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

/// `str.strip()`.
pub fn py_strip(value: &str) -> &str {
    value.trim_matches(is_py_space)
}

/// `str.rstrip()`.
pub fn py_rstrip(value: &str) -> &str {
    value.trim_end_matches(is_py_space)
}

/// `str.lstrip()`.
pub fn py_lstrip(value: &str) -> &str {
    value.trim_start_matches(is_py_space)
}

/// `str.split()` sin argumentos: trozos no vacíos separados por espacio.
pub fn py_split(value: &str) -> impl Iterator<Item = &str> {
    value.split(is_py_space).filter(|part| !part.is_empty())
}

/// `" ".join(value.split())`.
pub fn collapse_spaces(value: &str) -> String {
    py_split(value).collect::<Vec<_>>().join(" ")
}

/// `value[:count]` contando puntos de código.
pub fn head_chars(value: &str, count: usize) -> &str {
    match value.char_indices().nth(count) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

/// `value[-count:]` contando puntos de código.
pub fn tail_chars(value: &str, count: usize) -> &str {
    let total = value.chars().count();
    if total <= count {
        return value;
    }
    match value.char_indices().nth(total - count) {
        Some((index, _)) => &value[index..],
        None => value,
    }
}

/// `len(value)`.
pub fn char_len(value: &str) -> usize {
    value.chars().count()
}

/// `str(value or "")` para un valor opcional del JSON.
pub fn text_or_empty(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => String::new(),
    }
}

/// `str.splitlines()`: separa en `\n`, `\r`, `\r\n`, `\v`, `\f`, `\x1c`-`\x1e`,
/// `\x85`, ` ` y ` `, sin conservar los separadores.
pub fn py_splitlines(value: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = value.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        let breaks = matches!(
            character,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if !breaks {
            continue;
        }
        lines.push(&value[start..index]);
        let mut end = index + character.len_utf8();
        if character == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
            chars.next();
            end += 1;
        }
        start = end;
    }
    if start < value.len() {
        lines.push(&value[start..]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_count_code_points() {
        assert_eq!(head_chars("añb", 2), "añ");
        assert_eq!(tail_chars("añb", 2), "ñb");
        assert_eq!(tail_chars("ab", 5), "ab");
        assert_eq!(head_chars("ab", 0), "");
    }

    #[test]
    fn splits_like_python() {
        assert_eq!(collapse_spaces("  a \x1c b\n\tc "), "a b c");
        assert_eq!(py_strip("\x1f x \x1e"), "x");
    }
}
