//! Utilidades compartidas: comparación JSON, entorno por caso y archivos.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub(super) const ENV_NAMES: [&str; 8] = [
    "HOME",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "CODEX_HOME",
    "MINIMAX_DATA_DIR",
    "OPENCODE_CONFIG",
    "OPENCODE_CONFIG_DIR",
];

/// Sustituye `<tmp>` (marcador del oráculo) por la carpeta temporal real.
pub(super) fn unmark(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("<tmp>", temp)),
        Value::Array(items) => Value::Array(items.iter().map(|item| unmark(item, temp)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), unmark(value, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn json_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(actual), Value::Number(expected)) => {
            match (actual.as_f64(), expected.as_f64()) {
                (Some(actual), Some(expected)) => {
                    (actual - expected).abs() <= 1e-9 * actual.abs().max(expected.abs()).max(1.0)
                }
                _ => actual == expected,
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| json_eq(actual, expected))
        }
        (Value::Object(actual), Value::Object(expected)) => {
            actual.len() == expected.len()
                && actual.iter().all(|(key, value)| {
                    expected.get(key).is_some_and(|other| json_eq(value, other))
                })
        }
        _ => left == right,
    }
}

fn slice_chars(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// Primer punto donde dos JSON difieren (para el informe de fallos).
pub(super) fn first_diff(left: &Value, right: &Value) -> String {
    if json_eq(left, right) {
        return String::new();
    }
    match (left, right) {
        (Value::String(actual), Value::String(expected)) => {
            let position = actual
                .chars()
                .zip(expected.chars())
                .position(|(left, right)| left != right)
                .unwrap_or(actual.chars().count().min(expected.chars().count()));
            let start = position.saturating_sub(60);
            format!(
                "texto@{position}: …{}… / …{}…",
                slice_chars(actual, start, position + 60),
                slice_chars(expected, start, position + 60)
            )
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (index, (left, right)) in actual.iter().zip(expected).enumerate() {
                if !json_eq(left, right) {
                    return format!("[{index}].{}", first_diff(left, right));
                }
            }
            format!("longitud {} != {}", actual.len(), expected.len())
        }
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, value) in actual {
                match expected.get(key) {
                    Some(other) if json_eq(value, other) => continue,
                    Some(other) => return format!("{key}.{}", first_diff(value, other)),
                    None => return format!("clave de más: {key}"),
                }
            }
            for key in expected.keys() {
                if !actual.contains_key(key) {
                    return format!("falta clave: {key}");
                }
            }
            "objetos distintos".to_string()
        }
        _ => format!("{left:?} != {right:?}"),
    }
}

/// Carpeta del caso: `<home>/cases/<caso con «/» → «_»>`.
pub(super) fn case_root(home: &Path, case: &str) -> PathBuf {
    home.join("cases").join(case.replace('/', "_"))
}

/// Aplica el entorno del caso (las claves ausentes usan el valor por defecto).
pub(super) fn set_env(root: &Path, home: &Path, overrides: Option<&Value>) {
    let text = root.display().to_string();
    let defaults = [
        ("HOME", text.clone()),
        ("USERPROFILE", text.clone()),
        ("LOCALAPPDATA", text.clone()),
        ("APPDATA", text.clone()),
        ("CODEX_HOME", root.join("codex").display().to_string()),
        (
            "MINIMAX_DATA_DIR",
            root.join("minimax").display().to_string(),
        ),
        (
            "OPENCODE_CONFIG_DIR",
            root.join("opencode-config").display().to_string(),
        ),
    ];
    for (name, value) in defaults {
        std::env::set_var(name, value);
    }
    std::env::remove_var("OPENCODE_CONFIG");
    let home_text = home.display().to_string();
    for (name, value) in overrides.and_then(Value::as_object).into_iter().flatten() {
        match value.as_str() {
            Some(text) => {
                let path = text.replace("<tmp>", &home_text);
                // Los marcadores del oráculo usan «/»; los procesos reales
                // reciben la ruta nativa.
                let path = if std::path::MAIN_SEPARATOR == '\\' {
                    path.replace('/', "\\")
                } else {
                    path
                };
                std::env::set_var(name, path);
            }
            None => std::env::remove_var(name),
        }
    }
}

/// Crea `<root>/<relative>` con el texto tal cual (sin traducir finales).
pub(super) fn put(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta del archivo");
    }
    std::fs::write(&path, text.as_bytes()).expect("archivo del caso");
}

/// `_snapshot`: archivos bajo la carpeta con su texto y si llevan CRLF.
pub(super) fn snapshot(root: &Path) -> Value {
    let mut files = Map::new();
    let mut stack = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                paths.push(path);
            }
        }
    }
    paths.sort();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map(|rest| rest.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let raw = std::fs::read(&path).unwrap_or_default();
        let crlf = raw.windows(2).any(|pair| pair == b"\r\n");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let mut entry = Map::new();
        entry.insert("text".to_string(), Value::String(text));
        entry.insert("crlf".to_string(), Value::Bool(crlf));
        files.insert(relative, Value::Object(entry));
    }
    Value::Object(files)
}

/// Borra el árbol temporal propio (sin borrado recursivo de la biblioteca).
pub(super) fn cleanup(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup(&path);
            let _ = std::fs::remove_dir(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}
