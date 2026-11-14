//! Port of `src/ezagent/voice/agent_prelaunch.py` (F4-10).
//!
//! Preparativos previos al lanzamiento de agentes que guardan estado propio.
//! OpenCode recuerda en `model.json` la última variante (esfuerzo) elegida para
//! cada modelo y esa elección tiene prioridad sobre la configuración con la que
//! se lanza; por eso EzAgent le da un directorio de estado propio
//! (`XDG_STATE_HOME`) con la variante pedida ya escrita. mcode y Codex exponen
//! sus modelos desde archivos de configuración y se leen aquí.

use std::path::{Path, PathBuf};

use ez_core::dirs::home;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyloads::loads;
use ez_core::pyvalue::{py_strip, python_str, text_or_empty, truthy};
use regex::{Captures, Regex};
use serde_json::{json, Map, Value};

/// Entorno de `os.environ.get(name) or <fallback>`: una variable vacía no cuenta.
fn env_path(name: &str, fallback: PathBuf) -> PathBuf {
    match std::env::var_os(name) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => fallback,
    }
}

/// `opencode_state_root()`.
pub fn opencode_state_root() -> PathBuf {
    let base = env_path("LOCALAPPDATA", home().join(".local").join("state"));
    base.join("EzAgent").join("opencode-state")
}

/// Escribe `model.json` y devuelve las variables de entorno a fijar.
///
/// `model_ref` es `proveedor/modelo`. Con `effort` vacío no se fija ninguna
/// variante. Si no se puede escribir, devuelve un mapa vacío (el lanzamiento
/// sigue con el estado normal de OpenCode).
pub fn prepare_opencode_state(model_ref: &str, effort: &str) -> Map<String, Value> {
    prepare_opencode_state_at(&opencode_state_root(), model_ref, effort)
}

fn prepare_opencode_state_at(root: &Path, model_ref: &str, effort: &str) -> Map<String, Value> {
    let Some((provider, model)) = model_ref.split_once('/') else {
        return Map::new();
    };
    if provider.is_empty() || model.is_empty() {
        return Map::new();
    }
    match write_opencode_state(root, provider, model, model_ref, effort) {
        Ok(()) => {
            let mut env = Map::new();
            env.insert(
                "XDG_STATE_HOME".to_string(),
                Value::String(root.to_string_lossy().into_owned()),
            );
            env
        }
        Err(error) => {
            tracing::warn!("No pude preparar el estado de OpenCode: {}", error);
            Map::new()
        }
    }
}

fn write_opencode_state(
    root: &Path,
    provider: &str,
    model: &str,
    model_ref: &str,
    effort: &str,
) -> std::io::Result<()> {
    let target = root.join("opencode").join("model.json");
    let mut variant = Map::new();
    if !effort.is_empty() {
        variant.insert(model_ref.to_string(), Value::String(effort.to_string()));
    }
    let data = json!({
        "recent": [{"providerID": provider, "modelID": model}],
        "favorite": [],
        "variant": Value::Object(variant),
    });
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = dumps_with(&data, &DumpOptions::utf8());
    let tmp = target.with_file_name(format!(
        "{}.tmp",
        target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &target)
}

// --------------------------------------------------------------------- mcode
pub const MCODE_EFFORTS: [&str; 6] = ["default", "low", "medium", "high", "xhigh", "max"];

fn effort_pattern() -> Regex {
    // `expect` sobre una constante: el patrón no depende de datos del usuario.
    Regex::new(r"(?m)^(defaultModelThinking:[ \t]*\r?\n[ \t]+effort:[ \t]*)\S+")
        .expect("patrón de esfuerzo de mcode válido")
}

/// `mcode_config_path()`.
pub fn mcode_config_path() -> PathBuf {
    env_path("MINIMAX_DATA_DIR", home().join(".minimax")).join("config.yaml")
}

/// Modelos `proveedor/modelo` declarados en el config.yaml de mcode.
pub fn discover_mcode_models() -> Vec<String> {
    discover_mcode_models_at(&mcode_config_path())
}

fn discover_mcode_models_at(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut refs = Vec::new();
    for (provider, models) in parse_provider_models(&text) {
        for model in models {
            refs.push(format!("{provider}/{model}"));
        }
    }
    refs
}

/// Fija `defaultModelThinking.effort` en el config.yaml de mcode.
///
/// mcode no tiene un flag de esfuerzo para su TUI: lo lee de ese ajuste (el
/// mismo que escribe su selector /model). Es una edición de una sola línea,
/// sin reescribir el YAML (tiene anclas); antes se guarda una copia
/// `config.yaml.ezagent-bak` la primera vez. Devuelve `false` si no pudo.
pub fn set_mcode_effort(effort: &str) -> bool {
    if !MCODE_EFFORTS.contains(&effort) {
        return false;
    }
    set_mcode_effort_at(&mcode_config_path(), effort)
}

fn set_mcode_effort_at(path: &Path, effort: &str) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!("No pude fijar el esfuerzo de mcode: {}", error);
            return false;
        }
    };
    let next = next_config_text(&text, effort);
    if next == text {
        return true;
    }
    match replace_config(path, &text, &next) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!("No pude fijar el esfuerzo de mcode: {}", error);
            false
        }
    }
}

/// `"<base>.ezagent-bak"` + `<base>.tmp` y reemplazo atómico.
fn replace_config(path: &Path, original: &str, next: &str) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let backup = path.with_file_name(format!("{name}.ezagent-bak"));
    if !backup.exists() {
        // `open(..., newline="")`: se conservan los finales de línea originales.
        std::fs::write(&backup, original.as_bytes())?;
    }
    let tmp = path.with_file_name(format!("{name}.tmp"));
    std::fs::write(&tmp, next.as_bytes())?;
    std::fs::rename(&tmp, path)
}

/// Reemplaza el esfuerzo o agrega `defaultModelThinking` al final del texto.
fn next_config_text(text: &str, effort: &str) -> String {
    let pattern = effort_pattern();
    if pattern.is_match(text) {
        return pattern
            .replace(text, |captures: &Captures<'_>| {
                format!("{}{}", &captures[1], effort)
            })
            .into_owned();
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let separator = if text.ends_with('\n') { "" } else { newline };
    format!("{text}{separator}defaultModelThinking:{newline}  effort: {effort}{newline}")
}

// --------------------------------------------------------------------- codex
/// `codex_home()`.
pub fn codex_home() -> PathBuf {
    env_path("CODEX_HOME", home().join(".codex"))
}

/// Modelos visibles de Codex y sus niveles de esfuerzo.
///
/// Codex descarga con la cuenta del usuario un catálogo (`models_cache.json`)
/// donde cada modelo declara `supported_reasoning_levels`.
pub fn discover_codex_models() -> Map<String, Value> {
    discover_codex_models_at(&codex_home())
}

fn discover_codex_models_at(folder: &Path) -> Map<String, Value> {
    let text = std::fs::read_to_string(folder.join("models_cache.json")).ok();
    let data = text.and_then(|text| loads(&text).ok());
    let mut table = Map::new();
    let Some(Value::Object(data)) = data else {
        return table;
    };
    let Some(Value::Array(models)) = data.get("models") else {
        return table;
    };
    for model in models {
        let Some(model) = model.as_object() else {
            continue;
        };
        if model.get("visibility").and_then(Value::as_str) != Some("list") {
            continue;
        }
        let slug = py_strip(&text_or_empty(model.get("slug"))).to_string();
        let levels: Vec<Value> = match model.get("supported_reasoning_levels") {
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    let level = match item {
                        Value::Object(map) => map.get("effort").unwrap_or(&Value::Null),
                        other => other,
                    };
                    Value::String(python_str(level))
                })
                .collect(),
            _ => Vec::new(),
        };
        if !slug.is_empty() && !levels.is_empty() {
            table.insert(slug, Value::Array(levels));
        }
    }
    table
}

// ---------------------------------------------------------- opencode default
/// Quita comentarios `//` y `/* */` y comas finales, respetando las cadenas.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        let character = chars[index];
        if in_string {
            out.push(character);
            if character == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 1;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
            out.push(character);
        } else if chars[index..].starts_with(&['/', '/']) {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        } else if chars[index..].starts_with(&['/', '*']) {
            let end = (index + 2..chars.len().saturating_sub(1))
                .find(|start| chars[*start] == '*' && chars[start + 1] == '/');
            index = match end {
                Some(position) => position + 2,
                None => chars.len(),
            };
            continue;
        } else {
            out.push(character);
        }
        index += 1;
    }
    // `re.sub(r",(\s*[}\]])", r"\1", ...)`: igual que el Python, también mira
    // dentro de las cadenas (quirk reproducido).
    let trailing = Regex::new(r",(\s*[}\]])").expect("patrón de comas finales válido");
    trailing.replace_all(&out, "$1").into_owned()
}

/// Modelo por defecto de OpenCode (`proveedor/modelo`) o "" si no se sabe.
pub fn opencode_default_model() -> String {
    let config_dir = env_path(
        "OPENCODE_CONFIG_DIR",
        home().join(".config").join("opencode"),
    );
    let mut candidates = vec![std::env::var("OPENCODE_CONFIG").unwrap_or_default()];
    candidates.push(
        config_dir
            .join("opencode.jsonc")
            .to_string_lossy()
            .into_owned(),
    );
    candidates.push(
        config_dir
            .join("opencode.json")
            .to_string_lossy()
            .into_owned(),
    );
    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        if let Some(model) = default_model_from(&text) {
            return model;
        }
    }
    String::new()
}

fn default_model_from(text: &str) -> Option<String> {
    let data = loads(&strip_jsonc(text)).ok()?;
    let model = data.as_object()?.get("model")?;
    match model {
        Value::String(text) if text.contains('/') => {
            Some(text.split('#').next().unwrap_or(text).to_string())
        }
        Value::Object(spec) => {
            let provider = spec.get("providerID");
            let name = spec.get("model");
            if provider.is_some_and(truthy) && name.is_some_and(truthy) {
                return Some(format!(
                    "{}/{}",
                    python_str(provider.unwrap_or(&Value::Null)),
                    python_str(name.unwrap_or(&Value::Null))
                ));
            }
            None
        }
        _ => None,
    }
}

// ------------------------------------------------------- mini-lector de YAML
/// Extrae los nombres de modelo de `provider.<proveedor>.models` de un YAML de
/// bloque, conservando el orden de aparición.
///
/// shortcut: solo entiende mapas de bloque (clave: valor por líneas); un
/// config.yaml con mapas en flujo (`provider: {x: …}`) lista cero modelos, que
/// es el mismo efecto que un YAML ilegible. Cambiar a `serde_yaml` cuando
/// ez-herdr pueda declararlo.
fn parse_provider_models(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    for raw in text.lines() {
        let line = strip_yaml_comment(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }
        if trimmed == "-" || trimmed.starts_with("- ") {
            continue;
        }
        let Some((key, _value)) = split_key(trimmed) else {
            continue;
        };
        let depth = stack.len();
        let in_provider = stack.first().is_some_and(|(_, name)| name == "provider");
        if depth == 0 && key == "provider" {
            stack.push((indent, key));
        } else if depth == 1 && in_provider {
            out.push((key.clone(), Vec::new()));
            stack.push((indent, key));
        } else if depth == 2 && in_provider && key == "models" {
            stack.push((indent, key));
        } else if depth == 3 && in_provider && stack[2].1 == "models" {
            if let Some(last) = out.last_mut() {
                last.1.push(key.clone());
            }
            stack.push((indent, key));
        } else {
            stack.push((indent, key));
        }
    }
    out
}

/// Quita el comentario `#` de una línea YAML sin tocar las cadenas.
fn strip_yaml_comment(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut after_space = true;
    for character in line.chars() {
        match quote {
            Some(current) => {
                out.push(character);
                if character == current {
                    quote = None;
                }
            }
            None => {
                if character == '#' && after_space {
                    break;
                }
                if character == '"' || character == '\'' {
                    quote = Some(character);
                }
                out.push(character);
                after_space = character == ' ' || character == '\t';
            }
        }
    }
    out
}

/// Separa `clave: resto`; admite claves entre comillas.
fn split_key(trimmed: &str) -> Option<(String, String)> {
    for quote in ['"', '\''] {
        if let Some(rest) = trimmed.strip_prefix(quote) {
            let end = rest.find(quote)?;
            let after = rest[end + 1..].trim_start().strip_prefix(':')?;
            return Some((rest[..end].to_string(), after.trim().to_string()));
        }
    }
    let colon = trimmed.find(':')?;
    let key = py_strip(&trimmed[..colon]);
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), trimmed[colon + 1..].trim().to_string()))
}

#[cfg(test)]
mod tests;
