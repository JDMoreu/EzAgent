//! Descubrimiento de whisper.cpp y modelos locales (F4-03).
//!
//! Port de `whisper_home`, `whisper_models_dir`, `whisper_model_path` y
//! `available_whisper_models` de `src/ezagent/voice/stt.py`.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_core::dirs;
use ez_core::pyvalue::{py_strip, text_or_empty};

const WHISPER_ORDER: [&str; 11] = [
    "ggml-tiny-q5_1",
    "ggml-tiny-q8_0",
    "ggml-base-q5_1",
    "ggml-base-q8_0",
    "ggml-small-q5_1",
    "ggml-small-q8_0",
    "ggml-medium-q5_0",
    "ggml-medium-q8_0",
    "ggml-large-v3-q5_0",
    "ggml-large-v3-turbo-q5_0",
    "ggml-large-v3-turbo-q8_0",
];

fn whisper_label(key: &str) -> Option<&'static str> {
    match key {
        "ggml-tiny-q5_1" => Some("Whisper Tiny · la más rápida"),
        "ggml-tiny-q8_0" => Some("Whisper Tiny q8 · muy rápida"),
        "ggml-base-q5_1" => Some("Whisper Base · rápida"),
        "ggml-base-q8_0" => Some("Whisper Base q8 · rápida"),
        "ggml-small-q5_1" => Some("Whisper Small · equilibrada"),
        "ggml-small-q8_0" => Some("Whisper Small q8 · equilibrada"),
        "ggml-medium-q5_0" => Some("Whisper Medium · precisa"),
        "ggml-medium-q8_0" => Some("Whisper Medium q8 · precisa"),
        "ggml-large-v3-q5_0" => Some("Whisper Large v3 · máxima precisión"),
        "ggml-large-v3-turbo-q5_0" => Some("Whisper Large v3 Turbo q5 · más ligera"),
        "ggml-large-v3-turbo-q8_0" => Some("Whisper Large v3 Turbo q8 · recomendada"),
        _ => None,
    }
}

/// `Path(value).expanduser()`. `~usuario` queda sin expandir.
fn expand_user(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home();
    }
    match value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        Some(rest) => dirs::home().join(rest),
        None => PathBuf::from(value),
    }
}

fn env_text(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn local_app_data() -> PathBuf {
    env_text("LOCALAPPDATA")
        .map_or_else(|| dirs::home().join("AppData").join("Local"), PathBuf::from)
}

/// Primer candidato que sea carpeta; si ninguno, `None`.
pub(crate) fn pick_home(
    explicit: Option<PathBuf>,
    configured: Option<PathBuf>,
    fallback: PathBuf,
) -> Option<PathBuf> {
    explicit
        .filter(|path| path.is_dir())
        .or_else(|| configured.filter(|path| path.is_dir()))
        .or_else(|| fallback.is_dir().then_some(fallback))
}

/// Carpeta con `whisper-server.exe` y `models/`. Nunca lanza.
pub fn whisper_home() -> Option<PathBuf> {
    let explicit = env_text("EZAGENT_WHISPER_HOME").map(|value| expand_user(&value));
    let configured = match VoiceConfig::load(None) {
        Ok(config) => {
            let text = py_strip(&text_or_empty(config.get("whisper_home"))).to_string();
            (!text.is_empty()).then(|| expand_user(&text))
        }
        // Python: la autodetección nunca rompe; una config rota devuelve None.
        Err(_) => return None,
    };
    let fallback = local_app_data().join("EzAgent").join("whisper");
    pick_home(explicit, configured, fallback)
}

/// Carpeta de modelos: `EZAGENT_WHISPER_MODELS` o `whisper_home()/models`.
pub fn whisper_models_dir() -> Option<PathBuf> {
    if let Some(explicit) = env_text("EZAGENT_WHISPER_MODELS") {
        let root = expand_user(&explicit);
        if root.is_dir() {
            return Some(root);
        }
    }
    let home = whisper_home()?;
    let models = home.join("models");
    models.is_dir().then_some(models)
}

fn model_names(text: &str) -> [String; 2] {
    if text.ends_with(".bin") {
        [text.to_string(), text.to_string()]
    } else {
        [text.to_string(), format!("{text}.bin")]
    }
}

pub(crate) fn model_path_in(text: &str, folder: Option<&Path>) -> Option<PathBuf> {
    let direct = expand_user(text);
    if direct.is_file() {
        return Some(direct);
    }
    let folder = folder?;
    for name in model_names(text) {
        let candidate = folder.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Resuelve un nombre de modelo o una ruta a un `.bin` de whisper.cpp.
pub fn whisper_model_path(model: &str) -> Option<PathBuf> {
    let text = py_strip(model);
    if text.is_empty() {
        return None;
    }
    model_path_in(text, whisper_models_dir().as_deref())
}

fn model_rank(key: &str) -> usize {
    WHISPER_ORDER
        .iter()
        .position(|name| *name == key)
        .unwrap_or(WHISPER_ORDER.len())
}

fn default_label(key: &str) -> String {
    let base = key.strip_prefix("ggml-").unwrap_or(key);
    format!("Whisper {}", base.replace('-', " "))
}

fn finish_models(mut found: Vec<(String, String)>, current: &str) -> Vec<(String, String)> {
    let current = py_strip(current);
    if !current.is_empty() && !found.iter().any(|(key, _)| key == current) {
        let label = whisper_label(current).map_or_else(|| current.to_string(), str::to_string);
        found.push((current.to_string(), label));
    }
    found.sort_by(|left, right| {
        model_rank(&left.0)
            .cmp(&model_rank(&right.0))
            .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
    });
    found
}

pub(crate) fn available_in(folder: Option<&Path>, current: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    if let Some(folder) = folder {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return finish_models(found, current);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_bin = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"));
            if !is_bin || !path.is_file() {
                continue;
            }
            let Some(key) = path.file_stem() else {
                continue;
            };
            let key = key.to_string_lossy().into_owned();
            if found.iter().any(|(existing, _)| existing == &key) {
                continue;
            }
            let label = whisper_label(&key).map_or_else(|| default_label(&key), str::to_string);
            found.push((key, label));
        }
    }
    finish_models(found, current)
}

/// Modelos instalados, listos para una fila de selección del menú.
pub fn available_whisper_models(current: &str) -> Vec<(String, String)> {
    available_in(whisper_models_dir().as_deref(), current)
}
