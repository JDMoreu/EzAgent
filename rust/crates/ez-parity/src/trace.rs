//! Normalizador y comparador de trazas de reproducción (§4.5).

use std::sync::OnceLock;

use regex::Regex;

const UUID: &str = r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";
const ISO_TS: &str = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?";
const EPOCH: &str = r"\b1[6-9]\d{8}(?:\.\d+)?\b";
const PORT: &str = r"(?:127\.0\.0\.1|localhost|0\.0\.0\.0):\d+";
const WIN_TEMP: &str = r#"[A-Za-z]:\\Users\\[^\\\s]+\\AppData\\Local\\Temp\\[^\s"]*"#;
const POSIX_TEMP: &str = r#"/tmp/[^\s"]*"#;

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("patrón interno válido")
}

fn rules() -> &'static [(&'static str, OnceLock<Regex>); 6] {
    static RULES: OnceLock<[(&'static str, OnceLock<Regex>); 6]> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            (ISO_TS, OnceLock::new()),
            (EPOCH, OnceLock::new()),
            (UUID, OnceLock::new()),
            (PORT, OnceLock::new()),
            (WIN_TEMP, OnceLock::new()),
            (POSIX_TEMP, OnceLock::new()),
        ]
    })
}

/// Quita marcas de tiempo, UUID, puertos y rutas temporales de una línea.
pub fn normalize_line(line: &str) -> String {
    let mut text = line.to_string();
    for (index, (source, slot)) in rules().iter().enumerate() {
        let regex = slot.get_or_init(|| pattern(source));
        let replacement = match index {
            3 => "HOST:PORT",
            4 | 5 => "<tmp>/",
            _ => "<ts>",
        };
        text = regex.replace_all(&text, replacement).to_string();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize(trace: &str) -> Vec<String> {
    trace
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_line)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct TraceDiff {
    pub index: usize,
    pub expected: String,
    pub produced: String,
}

/// Primera diferencia en orden causal; `None` si son iguales.
pub fn compare(expected: &[String], produced: &[String]) -> Option<TraceDiff> {
    for (index, want) in expected.iter().enumerate() {
        match produced.get(index) {
            Some(got) if got == want => continue,
            Some(got) => {
                return Some(TraceDiff {
                    index,
                    expected: want.clone(),
                    produced: got.clone(),
                })
            }
            None => {
                return Some(TraceDiff {
                    index,
                    expected: want.clone(),
                    produced: "<fin>".to_string(),
                })
            }
        }
    }
    if produced.len() > expected.len() {
        return Some(TraceDiff {
            index: expected.len(),
            expected: "<fin>".to_string(),
            produced: produced[expected.len()].clone(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_volatile_values() {
        let line = "2025-11-13T10:00:00.123Z turn ts=1735689600.5 id=3f2b8c1e-1111-2222-3333-444455556666 url=http://127.0.0.1:58948/inference";
        let normalized = normalize_line(line);
        assert!(!normalized.contains("58948"));
        assert!(!normalized.contains("2025-11-13"));
        assert!(!normalized.contains("3f2b8c1e"));
        assert!(normalized.contains("HOST:PORT"));
    }

    #[test]
    fn same_trace_passes_and_different_fails() {
        let id = "3f2b8c1e-1111-2222-3333-444455556666";
        let other_id = "9a8b7c6d-1111-2222-3333-444455556666";
        let expected = normalize(&format!("turn start id={id}\nspeak hola\n"));
        let produced = normalize(&format!("turn start id={other_id}\nspeak hola\n"));
        assert_eq!(compare(&expected, &produced), None);
        let other = normalize(&format!("turn start id={id}\nspeak adios\n"));
        assert_eq!(compare(&expected, &other).map(|diff| diff.index), Some(1));
    }

    #[test]
    fn extra_line_is_reported() {
        let expected = normalize("a\n");
        let produced = normalize("a\nb\n");
        assert_eq!(
            compare(&expected, &produced).map(|diff| diff.index),
            Some(1)
        );
    }
}
