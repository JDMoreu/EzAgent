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

/// `repr(str)`: comillas simples salvo que el texto lleve `'` y no `"`.
pub fn repr_str(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `repr(value)` para los tipos que llegan del JSON.
pub fn python_repr(value: &serde_json::Value) -> String {
    use serde_json::Value;
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64(), number.as_f64()) {
            (Some(integer), _, _) => integer.to_string(),
            (_, Some(integer), _) => integer.to_string(),
            (_, _, Some(float)) => crate::pyjson::float_repr(float),
            _ => number.to_string(),
        },
        Value::String(text) => repr_str(text),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(python_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("{}: {}", repr_str(key), python_repr(item)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// `a == b` de Python entre valores JSON: `1 == 1.0 == True`.
pub fn py_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    let numeric = |value: &Value| match value {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(f64::from(u8::from(*flag))),
        _ => None,
    };
    if let (Some(a), Some(b)) = (numeric(left), numeric(right)) {
        return a == b;
    }
    match (left, right) {
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| py_eq(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, x)| b.get(key).is_some_and(|y| py_eq(x, y)))
        }
        _ => left == right,
    }
}

/// `format(value, "g")`: seis cifras significativas, sin ceros de relleno.
pub fn format_g(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".into()
        } else if value > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let scientific = format!("{:.5e}", value);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let trim = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            text.to_string()
        }
    };
    if !(-4..6).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{}e{sign}{:02}", trim(mantissa), exponent.abs());
    }
    let decimals = (5 - exponent).max(0) as usize;
    trim(&format!("{value:.decimals$}"))
}

/// `str.casefold()`.
pub fn casefold(text: &str) -> String {
    crate::casefold::casefold(text)
}

/// Minúsculas plegadas y sin tildes: `NFKD(casefold(text))` sin marcas combinantes.
pub fn fold_accents(text: &str) -> String {
    use unicode_normalization::char::canonical_combining_class;
    use unicode_normalization::UnicodeNormalization;
    crate::casefold::casefold(text)
        .nfkd()
        .filter(|character| canonical_combining_class(*character) == 0)
        .collect()
}

/// `NFKD(text)` sin marcas combinantes (sin plegar mayúsculas).
pub fn strip_accents(text: &str) -> String {
    use unicode_normalization::char::canonical_combining_class;
    use unicode_normalization::UnicodeNormalization;
    text.nfkd()
        .filter(|character| canonical_combining_class(*character) == 0)
        .collect()
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
