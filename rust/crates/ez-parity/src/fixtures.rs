//! Lector de fixtures JSONL y comparador de resultados (F1-01).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Una línea de `parity/fixtures/<dominio>/*.jsonl` (§4.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureCase {
    pub case: String,
    #[serde(rename = "fn")]
    pub function: String,
    pub input: Value,
    pub output: Value,
    pub error: Option<String>,
}

/// Carga completa del archivo. Fallar aquí es fallo de la tarea, no del caso.
pub fn load(path: &Path) -> Result<Vec<FixtureCase>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut cases = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let case: FixtureCase = serde_json::from_str(line)
            .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
        cases.push(case);
    }
    Ok(cases)
}

/// Todos los casos de un dominio (varios archivos), ordenados por `case`.
pub fn load_domain(dir: &Path) -> Result<Vec<FixtureCase>, String> {
    let mut cases = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            cases.extend(load(&path)?);
        }
    }
    cases.sort_by(|left, right| left.case.cmp(&right.case));
    Ok(cases)
}

/// Compara el JSON producido por Rust con el del fixture, como texto ordenado.
pub fn same_json(produced: &Value, expected: &Value) -> bool {
    produced == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_counts_cases() {
        let dir = std::env::temp_dir().join("ez-parity-fixtures-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.jsonl");
        std::fs::write(
            &path,
            "{\"case\": \"demo/0002\", \"fn\": \"b\", \"input\": 1, \"output\": 2, \"error\": null}\n\
             {\"case\": \"demo/0001\", \"fn\": \"a\", \"input\": null, \"output\": null, \"error\": \"boom\"}\n",
        )
        .unwrap();
        let cases = load(&path).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].function, "b");
        assert_eq!(cases[1].error.as_deref(), Some("boom"));
        std::fs::remove_file(&path).unwrap();
    }
}
