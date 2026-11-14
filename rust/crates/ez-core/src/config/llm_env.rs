//! Resolución de proveedor/modelo/clave para `VoiceConfig::apply_env`.
//!
//! Reproduce `resolve_llm` de `llm_providers.py` mientras F2-03 no lo porte a
//! `ez_core::llm_providers`.

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::dirs;

const OPENCODE_BASE_URL: &str = "https://opencode.ai/inference/go/openai/v1";
const DEFAULT_LLM_MODEL: &str = "deepseek-v4.1-flash";

/// Lo que `resolve_llm` de `llm_providers.py` devuelve y `apply_env` usa.
pub(super) struct ResolvedLlm {
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) api_style: String,
    pub(super) source: &'static str,
}

// DECISION: F2-03 porta `resolve_llm` completo en `llm_providers.rs`; mientras
// ese módulo siga vacío, `apply_env` resuelve aquí lo mismo (presets, entorno y
// `llm.json`) para que `load` se comporte como el Python. Al aterrizar F2-03
// basta con reemplazar esta función por la suya.
pub(super) fn resolve_llm(
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    api_key: Option<&str>,
    api_style: Option<&str>,
) -> Result<ResolvedLlm, String> {
    let data = load_llm_file()?;
    let profiles = data
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let env_provider = env_text("LLM_PROVIDER").filter(|text| !text.is_empty());
    let selected = normalize_provider(
        provider
            .or(env_provider.as_deref())
            .or_else(|| data.get("provider").and_then(Value::as_str))
            .unwrap_or("opencode"),
    );
    let (spec_base, spec_key_env, spec_model, spec_style) = preset(&selected);
    let profile = profile_for(&profiles, &selected);
    let mut env_base = env_text("LLM_BASE_URL").filter(|text| !text.is_empty());
    let mut env_model = env_text("LLM_MODEL").filter(|text| !text.is_empty());
    if selected != "opencode" {
        if env_base
            .as_deref()
            .is_some_and(|value| value.trim_end_matches('/') == OPENCODE_BASE_URL)
        {
            env_base = None;
        }
        if env_model.as_deref() == Some(DEFAULT_LLM_MODEL) {
            env_model = None;
        }
    }
    let env_key = env_text("LLM_API_KEY").filter(|text| !text.is_empty());
    let resolved_base = [
        base_url.filter(|text| !text.is_empty()),
        env_base.as_deref(),
        profile
            .and_then(|profile| profile.get("base_url"))
            .and_then(Value::as_str),
        data.get("base_url").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .next()
    .unwrap_or(spec_base);
    let resolved_model = [
        model.filter(|text| !text.is_empty()),
        env_model.as_deref(),
        profile
            .and_then(|profile| profile.get("model"))
            .and_then(Value::as_str),
        data.get("model").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .next()
    .unwrap_or(spec_model);
    let spec_key = env_text(spec_key_env).filter(|text| !text.is_empty());
    let resolved_key = [
        api_key.filter(|text| !text.is_empty()),
        env_key.as_deref(),
        profile
            .and_then(|profile| profile.get("api_key"))
            .and_then(Value::as_str),
        data.get("api_key").and_then(Value::as_str),
        spec_key.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next()
    .unwrap_or("");
    if resolved_base.trim().is_empty() {
        return Err(format!(
            "El proveedor '{selected}' necesita LLM_BASE_URL o base_url en llm.json."
        ));
    }
    let env_style = env_text("LLM_API_STYLE").filter(|text| !text.is_empty());
    let style = api_style
        .filter(|text| !text.is_empty())
        .or(env_style.as_deref())
        .or_else(|| {
            profile
                .and_then(|profile| profile.get("api_style"))
                .and_then(Value::as_str)
        })
        .unwrap_or(spec_style);
    let from_env = ["LLM_PROVIDER", "LLM_MODEL", "LLM_BASE_URL", "LLM_API_KEY"]
        .iter()
        .any(|name| env_text(name).is_some_and(|text| !text.is_empty()));
    Ok(ResolvedLlm {
        provider: selected,
        model: resolved_model.trim().to_string(),
        base_url: resolved_base.trim().to_string(),
        api_key: resolved_key.trim().to_string(),
        api_style: normalize_api_style(style).to_string(),
        source: if from_env {
            "env"
        } else if data.is_empty() {
            "defaults"
        } else {
            "file"
        },
    })
}

/// `(base_url, api_key_env, default_model, api_style)` de los presets.
fn preset(name: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match name {
        "opencode" => (
            OPENCODE_BASE_URL,
            "OPENCODE_API_KEY",
            DEFAULT_LLM_MODEL,
            "openai",
        ),
        "openai" => (
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
            "gpt-4.1-mini",
            "openai",
        ),
        "anthropic" => (
            "https://api.anthropic.com/v1",
            "ANTHROPIC_API_KEY",
            "claude-sonnet-5-5",
            "anthropic",
        ),
        "gemini" => (
            "https://generativelanguage.googleapis.com/v1beta/openai/",
            "GEMINI_API_KEY",
            "gemini-2.5-flash",
            "openai",
        ),
        "groq" => (
            "https://api.groq.com/openai/v1",
            "GROQ_API_KEY",
            "llama-3.3-70b-versatile",
            "openai",
        ),
        "cerebras" => (
            "https://api.cerebras.ai/v1",
            "CEREBRAS_API_KEY",
            "gpt-oss-120b",
            "openai",
        ),
        "deepseek" => (
            "https://api.deepseek.com/v1",
            "DEEPSEEK_API_KEY",
            "deepseek-chat",
            "openai",
        ),
        "openrouter" => (
            "https://openrouter.ai/api/v1",
            "OPENROUTER_API_KEY",
            "openai/gpt-4.1-mini",
            "openai",
        ),
        "custom" => ("", "LLM_API_KEY", "", "openai"),
        // Un proveedor desconocido se trata como endpoint personalizado.
        _ => ("", "LLM_API_KEY", "", "openai"),
    }
}

fn normalize_provider(name: &str) -> String {
    let value = name.trim().to_lowercase();
    let alias = match value.as_str() {
        "claude" => "anthropic",
        "google" => "gemini",
        "open-ai" | "open_ai" => "openai",
        other => other,
    };
    if alias.is_empty() {
        "opencode".to_string()
    } else {
        alias.to_string()
    }
}

fn normalize_api_style(value: &str) -> &'static str {
    if value.trim().to_lowercase() == "anthropic" {
        "anthropic"
    } else {
        "openai"
    }
}

fn profile_for<'a>(
    profiles: &'a Map<String, Value>,
    selected: &str,
) -> Option<&'a Map<String, Value>> {
    if let Some(Value::Object(profile)) = profiles.get(selected) {
        return Some(profile);
    }
    for (name, raw) in profiles {
        if normalize_provider(name) != selected {
            continue;
        }
        if let Value::Object(profile) = raw {
            return Some(profile);
        }
    }
    None
}

fn llm_config_path() -> PathBuf {
    if let Some(explicit) = env_text("EZ_LLM_CONFIG").filter(|text| !text.is_empty()) {
        return expand_home(&explicit);
    }
    dirs::llm_config_path()
}

fn expand_home(path: &str) -> PathBuf {
    let Some(rest) = path.strip_prefix('~') else {
        return PathBuf::from(path);
    };
    dirs::home().join(rest.trim_start_matches(['/', '\\']))
}

fn load_llm_file() -> Result<Map<String, Value>, String> {
    let path = llm_config_path();
    if !path.exists() {
        if env_text("EZ_LLM_CONFIG").is_some_and(|text| !text.is_empty()) {
            return Err(format!("No existe EZ_LLM_CONFIG: {}", path.display()));
        }
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("llm.json inválido en {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("llm.json inválido en {}: {error}", path.display()))?;
    Ok(match value {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

pub(super) fn env_text(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
