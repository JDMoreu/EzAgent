//! Carga mínima de `.env`, como `python-dotenv`.

use std::path::Path;

/// `.env` mínimo de `python-dotenv` (`load_dotenv(env_path)`).
// DECISION: sin la crate `dotenvy` (no está declarada en `ez-app`): claves
// `K=V`, comentarios `#` y comillas simples o dobles. No expande variables.
fn load_env(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || std::env::var_os(key).is_some() {
            continue;
        }
        let value = parse_env_value(value.trim());
        std::env::set_var(key, value);
    }
}

pub(crate) fn parse_env_value(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    {
        return inner
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|text| text.strip_suffix('\''))
    {
        return inner.to_string();
    }
    match value.find(" #") {
        Some(index) => value[..index].trim_end().to_string(),
        None => value.to_string(),
    }
}

pub(super) fn load_env_candidates() {
    let candidates = [
        super::super::core::resource_root().join(".env"),
        std::env::current_dir().unwrap_or_default().join(".env"),
    ];
    // DECISION: el caso `sys.frozen` del Python no existe en Rust; se usan las
    // dos rutas que sí aplican (raíz de recursos y directorio actual).
    if let Some(path) = candidates.into_iter().find(|path| path.exists()) {
        load_env(&path);
    }
}
