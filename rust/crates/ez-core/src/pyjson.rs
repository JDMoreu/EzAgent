//! `json.dumps` de Python, carácter por carácter (F2-01).
//!
//! Reproduce separadores por defecto (`", "`, `": "`), `indent`, `sort_keys`,
//! `ensure_ascii` y el formato de flotantes de Python (`1e-05`, `1.0`, `-0.0`).

use indexmap::IndexMap;
use serde_json::Value;

/// Opciones de `json.dumps` que usa el proyecto.
#[derive(Debug, Clone)]
pub struct DumpOptions {
    pub indent: Option<usize>,
    pub sort_keys: bool,
    pub ensure_ascii: bool,
    pub item_separator: String,
    pub key_separator: String,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self {
            indent: None,
            sort_keys: false,
            ensure_ascii: true,
            item_separator: ", ".to_string(),
            key_separator: ": ".to_string(),
        }
    }
}

impl DumpOptions {
    /// `indent` cambia los separadores como en Python: `","` y `": "`.
    pub fn with_indent(mut self, indent: usize) -> Self {
        self.indent = Some(indent);
        self.item_separator = ",".to_string();
        self.key_separator = ": ".to_string();
        self
    }
}

/// `json.dumps(value)` con las opciones por defecto de Python.
pub fn dumps(value: &Value) -> String {
    dumps_with(value, &DumpOptions::default())
}

pub fn dumps_with(value: &Value, options: &DumpOptions) -> String {
    let mut text = String::new();
    write_value(value, options, 0, &mut text);
    text
}

fn write_value(value: &Value, options: &DumpOptions, level: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => write_number(number, out),
        Value::String(text) => write_string(text, options.ensure_ascii, out),
        Value::Array(items) => write_array(items, options, level, out),
        Value::Object(map) => write_object(map, options, level, out),
    }
}

fn write_number(number: &serde_json::Number, out: &mut String) {
    if let Some(value) = number.as_i64() {
        out.push_str(&value.to_string());
    } else if let Some(value) = number.as_u64() {
        out.push_str(&value.to_string());
    } else if let Some(value) = number.as_f64() {
        out.push_str(&float_repr(value));
    } else {
        out.push_str(&number.to_string());
    }
}

fn write_array(items: &[Value], options: &DumpOptions, level: usize, out: &mut String) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(&options.item_separator);
        }
        newline_and_indent(options, level + 1, out);
        write_value(item, options, level + 1, out);
    }
    newline_and_indent(options, level, out);
    out.push(']');
}

fn write_object(map: &serde_json::Map<String, Value>, options: &DumpOptions, level: usize, out: &mut String) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }
    let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
    if options.sort_keys {
        pairs.sort_by(|left, right| left.0.cmp(right.0));
    }
    out.push('{');
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push_str(&options.item_separator);
        }
        newline_and_indent(options, level + 1, out);
        write_string(key, options.ensure_ascii, out);
        out.push_str(&options.key_separator);
        write_value(value, options, level + 1, out);
    }
    newline_and_indent(options, level, out);
    out.push('}');
}

fn newline_and_indent(options: &DumpOptions, level: usize, out: &mut String) {
    let Some(indent) = options.indent else {
        return;
    };
    out.push('\n');
    for _ in 0..(indent * level) {
        out.push(' ');
    }
}

fn write_string(text: &str, ensure_ascii: bool, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(character),
            other => {
                if !ensure_ascii && other as u32 >= 0x20 {
                    out.push(other);
                } else {
                    write_escape(other, out);
                }
            }
        }
    }
    out.push('"');
}

fn write_escape(character: char, out: &mut String) {
    let code = character as u32;
    if code < 0x10000 {
        out.push_str(&format!("\\u{code:04x}"));
        return;
    }
    // Python escribe pares subrogados para el plano astral.
    let value = code - 0x10000;
    let high = 0xD800 + (value >> 10);
    let low = 0xDC00 + (value & 0x3FF);
    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
}

/// `repr(float)` de Python: dígitos más cortos que reconstruyen el valor,
/// notación fija si el exponente está en -4..16 y científica fuera de ahí.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        return format!("{sign}0.0");
    }

    // `{:e}` de Rust ya da los dígitos más cortos que reconstruyen el valor.
    let scientific = format!("{magnitude:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .map(|(mantissa, exponent)| (mantissa.to_string(), exponent.parse::<i32>().unwrap_or(0)))
        .unwrap_or((scientific.clone(), 0));
    let digits: String = mantissa.chars().filter(|character| *character != '.').collect();

    if (-4..16).contains(&exponent) {
        let body = fixed_notation(&digits, exponent);
        format!("{sign}{body}")
    } else {
        let body = scientific_notation(&digits, exponent);
        format!("{sign}{body}")
    }
}

fn fixed_notation(digits: &str, exponent: i32) -> String {
    if exponent >= 0 {
        let integer_digits = exponent as usize + 1;
        if digits.len() <= integer_digits {
            let mut text = digits.to_string();
            text.push_str(&"0".repeat(integer_digits - digits.len()));
            text.push_str(".0");
            return text;
        }
        let (integer, fraction) = digits.split_at(integer_digits);
        format!("{integer}.{fraction}")
    } else {
        let zeros = "0".repeat((-exponent - 1) as usize);
        format!("0.{zeros}{digits}")
    }
}

fn scientific_notation(digits: &str, exponent: i32) -> String {
    let (head, tail) = digits.split_at(1);
    let mantissa = if tail.is_empty() { head.to_string() } else { format!("{head}.{tail}") };
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.abs())
}

/// Objeto ordenado por inserción, para no perder el orden de `dict`.
pub type OrderedObject = IndexMap<String, Value>;
