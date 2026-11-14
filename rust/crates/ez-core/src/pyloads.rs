//! `json.loads` de Python 3.12 con sus mensajes de error (F2-03).
//!
//! `serde_json` rechaza los mismos textos pero con otras palabras; los avisos
//! al usuario («llm.json inválido en …: Expecting property name enclosed in
//! double quotes: line 1 column 3 (char 2)») llevan el texto de Python tal
//! cual, así que el analizador sigue al escáner en C de `json.decoder`.

use serde_json::{Map, Number, Value};

const WHITESPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// `json.JSONDecodeError`: mensaje, posición (en caracteres) y su texto final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyJsonError {
    pub message: String,
    pub pos: usize,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for PyJsonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: line {} column {} (char {})",
            self.message, self.line, self.column, self.pos
        )
    }
}

/// `json.loads(text)`.
///
/// DECISION: `NaN`, `Infinity` y `-Infinity` se aceptan como en Python pero
/// quedan como `null` (`serde_json::Value` no guarda flotantes no finitos);
/// los sustitutos sueltos de `\ud800` quedan como U+FFFD.
pub fn loads(text: &str) -> Result<Value, PyJsonError> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() == Some(&'\u{feff}') {
        return Err(error(
            &chars,
            "Unexpected UTF-8 BOM (decode using utf-8-sig)",
            0,
        ));
    }
    let parser = Parser { chars: &chars };
    let start = parser.skip_ws(0);
    let (value, end) = parser.value(start)?;
    let end = parser.skip_ws(end);
    if end != chars.len() {
        return Err(error(&chars, "Extra data", end));
    }
    Ok(value)
}

fn error(chars: &[char], message: &str, pos: usize) -> PyJsonError {
    let before = &chars[..pos.min(chars.len())];
    let line = before.iter().filter(|c| **c == '\n').count() + 1;
    let column = match before.iter().rposition(|c| *c == '\n') {
        Some(last) => pos - last,
        None => pos + 1,
    };
    PyJsonError {
        message: message.to_string(),
        pos,
        line,
        column,
    }
}

struct Parser<'a> {
    chars: &'a [char],
}

impl Parser<'_> {
    fn at(&self, index: usize) -> Option<char> {
        self.chars.get(index).copied()
    }

    fn fail<T>(&self, message: &str, pos: usize) -> Result<T, PyJsonError> {
        Err(error(self.chars, message, pos))
    }

    fn skip_ws(&self, mut index: usize) -> usize {
        while self.at(index).is_some_and(|c| WHITESPACE.contains(&c)) {
            index += 1;
        }
        index
    }

    fn starts_with(&self, index: usize, word: &str) -> bool {
        word.chars()
            .enumerate()
            .all(|(offset, c)| self.at(index + offset) == Some(c))
    }

    /// `scan_once`: `Err("Expecting value")` cuando no hay un valor en `index`.
    fn value(&self, index: usize) -> Result<(Value, usize), PyJsonError> {
        match self.at(index) {
            Some('"') => self
                .string(index + 1)
                .map(|(s, end)| (Value::String(s), end)),
            Some('{') => self.object(index + 1),
            Some('[') => self.array(index + 1),
            Some('n') if self.starts_with(index, "null") => Ok((Value::Null, index + 4)),
            Some('t') if self.starts_with(index, "true") => Ok((Value::Bool(true), index + 4)),
            Some('f') if self.starts_with(index, "false") => Ok((Value::Bool(false), index + 5)),
            Some('N') if self.starts_with(index, "NaN") => Ok((Value::Null, index + 3)),
            Some('I') if self.starts_with(index, "Infinity") => Ok((Value::Null, index + 8)),
            Some('-') if self.starts_with(index, "-Infinity") => Ok((Value::Null, index + 9)),
            _ => self.number(index),
        }
    }

    fn digits(&self, mut index: usize) -> usize {
        while self.at(index).is_some_and(|c| c.is_ascii_digit()) {
            index += 1;
        }
        index
    }

    fn number(&self, start: usize) -> Result<(Value, usize), PyJsonError> {
        let mut index = start;
        if self.at(index) == Some('-') {
            index += 1;
        }
        match self.at(index) {
            Some('1'..='9') => index = self.digits(index),
            Some('0') => index += 1,
            _ => return self.fail("Expecting value", start),
        }
        let mut is_float = false;
        if self.at(index) == Some('.') && self.at(index + 1).is_some_and(|c| c.is_ascii_digit()) {
            index = self.digits(index + 1);
            is_float = true;
        }
        if matches!(self.at(index), Some('e' | 'E')) {
            let mut probe = index + 1;
            if matches!(self.at(probe), Some('+' | '-')) {
                probe += 1;
            }
            if self.at(probe).is_some_and(|c| c.is_ascii_digit()) {
                index = self.digits(probe);
                is_float = true;
            }
        }
        let text: String = self.chars[start..index].iter().collect();
        Ok((number_value(&text, is_float), index))
    }

    fn array(&self, mut index: usize) -> Result<(Value, usize), PyJsonError> {
        let mut values = Vec::new();
        index = self.skip_ws(index);
        if self.at(index) == Some(']') {
            return Ok((Value::Array(values), index + 1));
        }
        loop {
            let (value, end) = self.value(index)?;
            values.push(value);
            index = self.skip_ws(end);
            match self.at(index) {
                Some(']') => return Ok((Value::Array(values), index + 1)),
                Some(',') => index = self.skip_ws(index + 1),
                _ => return self.fail("Expecting ',' delimiter", index),
            }
        }
    }

    fn object(&self, mut index: usize) -> Result<(Value, usize), PyJsonError> {
        let mut map = Map::new();
        index = self.skip_ws(index);
        if self.at(index) == Some('}') {
            return Ok((Value::Object(map), index + 1));
        }
        loop {
            if self.at(index) != Some('"') {
                return self.fail("Expecting property name enclosed in double quotes", index);
            }
            let (key, end) = self.string(index + 1)?;
            index = self.skip_ws(end);
            if self.at(index) != Some(':') {
                return self.fail("Expecting ':' delimiter", index);
            }
            index = self.skip_ws(index + 1);
            let (value, end) = self.value(index)?;
            map.insert(key, value);
            index = self.skip_ws(end);
            match self.at(index) {
                Some('}') => return Ok((Value::Object(map), index + 1)),
                Some(',') => index = self.skip_ws(index + 1),
                _ => return self.fail("Expecting ',' delimiter", index),
            }
        }
    }

    /// `scanstring`: `start` apunta justo después de la comilla de apertura.
    fn string(&self, start: usize) -> Result<(String, usize), PyJsonError> {
        let begin = start - 1;
        let mut out = String::new();
        let mut index = start;
        loop {
            let Some(c) = self.at(index) else {
                return self.fail("Unterminated string starting at", begin);
            };
            match c {
                '"' => return Ok((out, index + 1)),
                '\\' => index = self.escape(index + 1, begin, &mut out)?,
                c if (c as u32) < 0x20 => return self.fail("Invalid control character at", index),
                c => {
                    out.push(c);
                    index += 1;
                }
            }
        }
    }

    /// Procesa lo que sigue a `\\`; devuelve la posición siguiente.
    fn escape(&self, next: usize, begin: usize, out: &mut String) -> Result<usize, PyJsonError> {
        let Some(c) = self.at(next) else {
            return self.fail("Unterminated string starting at", begin);
        };
        let simple = match c {
            '"' => Some('"'),
            '\\' => Some('\\'),
            '/' => Some('/'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            _ => None,
        };
        if let Some(simple) = simple {
            out.push(simple);
            return Ok(next + 1);
        }
        if c != 'u' {
            return self.fail("Invalid \\escape", next - 1);
        }
        let digits_start = next + 1;
        let end = digits_start + 4;
        if end >= self.chars.len() {
            return self.fail("Invalid \\uXXXX escape", next);
        }
        let mut code = self.hex4(digits_start, next)?;
        let mut after = end;
        if (0xD800..0xDC00).contains(&code)
            && end + 6 < self.chars.len()
            && self.at(end) == Some('\\')
            && self.at(end + 1) == Some('u')
        {
            let low = self.hex4(end + 2, end + 1)?;
            if (0xDC00..0xE000).contains(&low) {
                code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                after = end + 6;
            }
        }
        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
        Ok(after)
    }

    fn hex4(&self, start: usize, error_pos: usize) -> Result<u32, PyJsonError> {
        let mut code = 0;
        for offset in 0..4 {
            let digit = self.at(start + offset).and_then(|c| c.to_digit(16));
            match digit {
                Some(digit) => code = code * 16 + digit,
                None => return self.fail("Invalid \\uXXXX escape", error_pos),
            }
        }
        Ok(code)
    }
}

fn number_value(text: &str, is_float: bool) -> Value {
    if !is_float {
        if let Ok(value) = text.parse::<i64>() {
            return Value::from(value);
        }
        if let Ok(value) = text.parse::<u64>() {
            return Value::from(value);
        }
    }
    // DECISION: un entero que no cabe en 64 bits se guarda como flotante.
    text.parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map_or(Value::Null, Value::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> String {
        loads(text).expect_err("debe fallar").to_string()
    }

    #[test]
    fn reports_python_messages() {
        assert_eq!(
            message("{ no json"),
            "Expecting property name enclosed in double quotes: line 1 column 3 (char 2)"
        );
        assert_eq!(message(""), "Expecting value: line 1 column 1 (char 0)");
        assert_eq!(
            message("[1 2]"),
            "Expecting ',' delimiter: line 1 column 4 (char 3)"
        );
        assert_eq!(
            message("{\"a\" 1}"),
            "Expecting ':' delimiter: line 1 column 6 (char 5)"
        );
        assert_eq!(message("1 2"), "Extra data: line 1 column 3 (char 2)");
        assert_eq!(
            message("\"abc"),
            "Unterminated string starting at: line 1 column 1 (char 0)"
        );
        assert_eq!(
            message("\"a\\x\""),
            "Invalid \\escape: line 1 column 3 (char 2)"
        );
        assert_eq!(
            message("[\n  1,\n  oops]"),
            "Expecting value: line 3 column 3 (char 9)"
        );
        assert_eq!(
            message("\"a\nb\""),
            "Invalid control character at: line 1 column 3 (char 2)"
        );
    }

    #[test]
    fn parses_values() {
        let value = loads(" {\"a\": [1, 2.5, -0, true, null, \"x\\u00e9\\n\"], \"a\": 3} ")
            .expect("JSON válido");
        assert_eq!(value, serde_json::json!({"a": 3}));
        let value = loads("[1e2, \"\\ud83d\\ude00\"]").expect("JSON válido");
        assert_eq!(value, serde_json::json!([100.0, "\u{1f600}"]));
        assert!(loads("\u{feff}{}").is_err());
    }
}
