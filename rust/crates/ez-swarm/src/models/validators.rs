//! Validators of `models.py`: flatten, clip and normalize untrusted text.
//!
//! The [`PyArg`] trait models Python's `object` parameters so the messages stay
//! identical for `None`, numbers and lists.

use ez_core::config::{python_int, python_str, truthy};
use ez_core::pyvalue::{
    casefold, char_len, collapse_spaces, head_chars, py_rstrip, py_strip, python_repr, repr_str,
};
use serde_json::Value;

use super::{SwarmError, MAX_REF};

/// Argument accepted by the validators: Python receives `object`.
///
/// `text` is `str(value or "")` and `repr` is `repr(value)`; they keep the
/// messages identical when the caller passes `None`, a number or a list.
pub trait PyArg {
    /// `str(value or "")`.
    fn text(&self) -> String;
    /// `repr(value)`.
    fn repr(&self) -> String;
}

impl PyArg for str {
    fn text(&self) -> String {
        self.to_string()
    }

    fn repr(&self) -> String {
        repr_str(self)
    }
}

impl PyArg for String {
    fn text(&self) -> String {
        self.clone()
    }

    fn repr(&self) -> String {
        repr_str(self)
    }
}

impl PyArg for Value {
    fn text(&self) -> String {
        if truthy(self) {
            python_str(self)
        } else {
            String::new()
        }
    }

    fn repr(&self) -> String {
        python_repr(self)
    }
}

/// `" ".join(str(value or "").replace("\x00", " ").split())`.
pub fn flat(value: &(impl PyArg + ?Sized)) -> String {
    collapse_spaces(&value.text().replace('\0', " "))
}

/// One-line text clipped to `limit` with «…» when needed.
///
/// Returns the text and whether it was clipped.
pub fn clip(value: &(impl PyArg + ?Sized), limit: usize) -> (String, bool) {
    let text = flat(value);
    if char_len(&text) <= limit {
        return (text, false);
    }
    // DECISION: every limit in the Python code is a constant > 0; with 0 the
    // port returns only «…» instead of Python's negative slice.
    let head = py_rstrip(head_chars(&text, limit.saturating_sub(1)));
    (format!("{head}…"), true)
}

/// Required text, clipped to `limit`.
pub fn required(
    value: &(impl PyArg + ?Sized),
    limit: usize,
    what: &str,
) -> Result<String, SwarmError> {
    let (text, _) = clip(value, limit);
    if text.is_empty() {
        return Err(SwarmError::new(format!("Falta {what}.")));
    }
    Ok(text)
}

/// Member name with the default description «nombre».
pub fn member_name(value: &(impl PyArg + ?Sized)) -> Result<String, SwarmError> {
    member_name_as(value, "nombre")
}

/// Normalized member name; `what` is used in the error text.
pub fn member_name_as(value: &(impl PyArg + ?Sized), what: &str) -> Result<String, SwarmError> {
    let text = py_strip(&value.text()).to_lowercase();
    if !valid_name(&text) {
        return Err(SwarmError::new(format!(
            "El {what} {} no es válido: usa a-z, 0-9, «-» o «_».",
            repr_str(&text)
        )));
    }
    Ok(text)
}

fn valid_name(name: &str) -> bool {
    let count = char_len(name);
    if !(1..=32).contains(&count) {
        return false;
    }
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        })
}

/// Normalized role; empty means «anyone».
pub fn role_name(value: &(impl PyArg + ?Sized)) -> Result<String, SwarmError> {
    let text = py_strip(&value.text()).to_lowercase();
    if text.is_empty() {
        Ok(String::new())
    } else {
        member_name_as(&text, "rol")
    }
}

/// Task number: accepts «T3», «t3» or «3».
pub fn task_id(value: &(impl PyArg + ?Sized)) -> Result<i64, SwarmError> {
    let raw = value.text();
    let text = py_strip(&raw);
    let text = text.trim_start_matches(['T', 't']);
    // shortcut: Python's `str.isdigit()` also accepts superscripts that `int()`
    // then rejects; only decimal digits (`Nd`) reach this function in practice.
    let digits = !text.is_empty()
        && text
            .chars()
            .all(|character| python_int(character.encode_utf8(&mut [0; 4])).is_some());
    match (digits, python_int(text)) {
        (true, Some(number)) => Ok(number),
        _ => Err(SwarmError::new(format!(
            "Número de tarea no válido: {}.",
            value.repr()
        ))),
    }
}

/// Normalized lease path: relative, with «/» and without «..».
pub fn lease_path(value: &(impl PyArg + ?Sized)) -> Result<String, SwarmError> {
    let text = py_strip(&value.text().replace('\0', "")).replace('\\', "/");
    let mut text = text.as_str();
    while let Some(rest) = text.strip_prefix("./") {
        text = rest;
    }
    let text = text.trim_end_matches('/');
    let parts: Vec<&str> = text
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.contains(&"..") || char_len(text) > MAX_REF {
        return Err(SwarmError::new(format!(
            "Ruta de reserva no válida: {}.",
            value.repr()
        )));
    }
    Ok(casefold(&parts.join("/")))
}

/// Whether two normalized paths collide (equal or one inside the other).
pub fn overlaps(first: &str, second: &str) -> bool {
    first == second || under(first, second) || under(second, first)
}

fn under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_is_flattened_and_clipped() {
        for (value, expected) in [
            ("  hola  ", "hola"),
            ("a\n b\tc", "a b c"),
            ("a\0b", "a b"),
            ("", ""),
        ] {
            assert_eq!(flat(value), expected);
        }
        for (value, expected) in [(json!(null), ""), (json!(5), "5"), (json!(["x"]), "['x']")] {
            assert_eq!(flat(&value), expected);
        }
        assert_eq!(clip("xxxxxxxxxxx", 10), ("xxxxxxxxx…".to_string(), true));
        assert_eq!(clip("ññññ", 3), ("ññ…".to_string(), true));
        assert_eq!(required("ok", 10, "el objetivo").expect("texto"), "ok");
        let long = "x".repeat(20);
        assert_eq!(
            required(long.as_str(), 10, "x"),
            Ok("xxxxxxxxx…".to_string())
        );
        assert_eq!(
            required("", 10, "el objetivo").unwrap_err().message(),
            "Falta el objetivo."
        );
    }

    #[test]
    fn names_are_normalized_and_validated() {
        assert_eq!(member_name("  ANA  ").expect("nombre"), "ana");
        assert_eq!(member_name("a-b_c").expect("nombre"), "a-b_c");
        assert_eq!(role_name("").expect("rol"), "");
        assert_eq!(role_name("  QA  ").expect("rol"), "qa");
        assert_eq!(
            member_name("a b").unwrap_err().message(),
            "El nombre 'a b' no es válido: usa a-z, 0-9, «-» o «_»."
        );
        assert_eq!(
            member_name(&json!(null)).unwrap_err().message(),
            "El nombre '' no es válido: usa a-z, 0-9, «-» o «_»."
        );
        assert_eq!(
            role_name("x!").unwrap_err().message(),
            "El rol 'x!' no es válido: usa a-z, 0-9, «-» o «_»."
        );
    }

    #[test]
    fn task_ids_accept_prefix_and_unicode_digits() {
        for (value, expected) in [
            ("T3", 3),
            ("t3", 3),
            ("TT3", 3),
            ("0003", 3),
            ("١٢", 12),
            ("T0", 0),
        ] {
            assert_eq!(task_id(value).expect("tarea"), expected);
        }
        for value in ["", "T", "-1", "3.0", "3x"] {
            assert!(task_id(value).is_err());
        }
        assert_eq!(
            task_id("T 3").unwrap_err().message(),
            "Número de tarea no válido: 'T 3'."
        );
        assert_eq!(
            task_id(&json!(null)).unwrap_err().message(),
            "Número de tarea no válido: None."
        );
    }

    #[test]
    fn lease_paths_are_normalized_and_checked() {
        for (value, expected) in [
            ("./src/a.rs", "src/a.rs"),
            (r"src\a.rs", "src/a.rs"),
            ("SRC/A.RS", "src/a.rs"),
            ("a/./b", "a/b"),
            ("a/", "a"),
            ("././x", "x"),
            ("C:/x/y", "c:/x/y"),
            ("a\0b", "ab"),
            ("Straße/A", "strasse/a"),
        ] {
            assert_eq!(lease_path(value).expect("ruta"), expected);
        }
        for value in ["../x", "/", "", "a/b/../c"] {
            assert!(lease_path(value).is_err());
        }
        assert_eq!(
            lease_path("../x").unwrap_err().message(),
            "Ruta de reserva no válida: '../x'."
        );
        assert_eq!(
            lease_path(&json!(null)).unwrap_err().message(),
            "Ruta de reserva no válida: None."
        );
    }

    #[test]
    fn overlaps_treats_directories_as_prefixes() {
        for (first, second, expected) in [
            ("a", "a", true),
            ("a/b", "a", true),
            ("a", "a/b", true),
            ("src/a.rs", "src", true),
            ("a/b", "a/c", false),
            ("ab", "a", false),
            ("a", "", false),
            ("", "", true),
            ("A", "a", false),
        ] {
            assert_eq!(overlaps(first, second), expected);
        }
    }
}
