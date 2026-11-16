//! Port of `src/ezagent/llm_providers.py` (F2-03).
//!
//! Presets, alias, capacidades y lista de modelos. La resolución de perfiles
//! (`llm.json`, entorno, catálogo del menú) vive en `profiles`.

use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

mod profiles;
mod urls;

pub use profiles::{
    config_path, load_provider_catalog, resolve_llm, save_provider_profile, CatalogEntry,
    LlmConfig, ResolveArgs, SaveArgs,
};
pub use urls::{is_local_endpoint, urlsplit, InvalidUrl, UrlParts};

pub const API_STYLES: [&str; 2] = ["openai", "anthropic"];
pub const CUSTOM_PROVIDER: &str = "custom";
pub const OPENCODE_BASE_URL: &str = "https://opencode.ai/inference/go/openai/v1";
pub const DEFAULT_LLM_MODEL: &str = "deepseek-v4.1-flash";

/// `ProviderSpec`: un preset de proveedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    pub api_key_env: &'static str,
    pub default_model: &'static str,
    pub api_style: &'static str,
}

const fn spec(
    name: &'static str,
    label: &'static str,
    base_url: &'static str,
    api_key_env: &'static str,
    default_model: &'static str,
    api_style: &'static str,
) -> ProviderSpec {
    ProviderSpec {
        name,
        label,
        base_url,
        api_key_env,
        default_model,
        api_style,
    }
}

/// `PROVIDERS`, en el mismo orden que el Python (el menú lo recorre así).
pub const PROVIDERS: [ProviderSpec; 9] = [
    spec(
        "opencode",
        "OpenCode Go",
        OPENCODE_BASE_URL,
        "OPENCODE_API_KEY",
        DEFAULT_LLM_MODEL,
        "openai",
    ),
    spec(
        "openai",
        "OpenAI (GPT, Codex)",
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
        "gpt-4.1-mini",
        "openai",
    ),
    spec(
        "anthropic",
        "Claude (Anthropic)",
        "https://api.anthropic.com/v1",
        "ANTHROPIC_API_KEY",
        "claude-sonnet-5-5",
        "anthropic",
    ),
    spec(
        "gemini",
        "Gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai/",
        "GEMINI_API_KEY",
        "gemini-2.5-flash",
        "openai",
    ),
    spec(
        "groq",
        "Groq",
        "https://api.groq.com/openai/v1",
        "GROQ_API_KEY",
        "llama-3.3-70b-versatile",
        "openai",
    ),
    spec(
        "cerebras",
        "Cerebras",
        "https://api.cerebras.ai/v1",
        "CEREBRAS_API_KEY",
        "gpt-oss-120b",
        "openai",
    ),
    spec(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "DEEPSEEK_API_KEY",
        "deepseek-chat",
        "openai",
    ),
    spec(
        "openrouter",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
        "openai/gpt-4.1-mini",
        "openai",
    ),
    spec(
        CUSTOM_PROVIDER,
        "Personalizado",
        "",
        "LLM_API_KEY",
        "",
        "openai",
    ),
];

/// Sugerencias de reserva por proveedor: `(id, etiqueta)`.
pub const MODEL_CATALOG: [(&str, &[(&str, &str)]); 8] = [
    (
        "opencode",
        &[
            ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash · rápido"),
            ("deepseek-v4.1", "DeepSeek V4.1"),
        ],
    ),
    (
        "openai",
        &[
            ("gpt-4.1-mini", "GPT-4.1 mini"),
            ("gpt-4.1", "GPT-4.1"),
            ("gpt-4o-mini", "GPT-4o mini"),
            ("gpt-4o", "GPT-4o"),
            ("o4-mini", "o4-mini · razonamiento"),
        ],
    ),
    (
        "groq",
        &[
            ("llama-3.3-70b-versatile", "Llama 3.3 70B · versátil"),
            ("llama-3.1-8b-instant", "Llama 3.1 8B · instantáneo"),
            ("deepseek-r1-distill-llama-70b", "DeepSeek R1 Distill 70B"),
        ],
    ),
    (
        "anthropic",
        &[
            ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ("claude-opus-5-5", "Claude Opus 5.5"),
            ("claude-haiku-5-5", "Claude Haiku 5.5 · rápido"),
        ],
    ),
    (
        "gemini",
        &[
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
            ("gemini-2.5-pro", "Gemini 2.5 Pro"),
            ("gemini-2.0-flash", "Gemini 2.0 Flash"),
        ],
    ),
    (
        "cerebras",
        &[
            ("gpt-oss-120b", "GPT-OSS 120B"),
            ("llama-3.3-70b", "Llama 3.3 70B"),
        ],
    ),
    (
        "deepseek",
        &[
            ("deepseek-chat", "DeepSeek Chat"),
            ("deepseek-reasoner", "DeepSeek Reasoner · razonamiento"),
        ],
    ),
    (
        "openrouter",
        &[
            ("openai/gpt-4.1-mini", "GPT-4.1 mini · OpenRouter"),
            (
                "anthropic/claude-sonnet-5-5",
                "Claude Sonnet 5.5 · OpenRouter",
            ),
            ("google/gemini-2.5-flash", "Gemini 2.5 Flash · OpenRouter"),
            (
                "meta-llama/llama-3.3-70b-instruct",
                "Llama 3.3 70B · OpenRouter",
            ),
        ],
    ),
];

pub(crate) use crate::pyvalue::py_strip;

/// `normalize_api_style`.
pub fn normalize_api_style(value: Option<&str>) -> &'static str {
    let text = py_strip(value.unwrap_or("")).to_lowercase();
    API_STYLES
        .iter()
        .find(|style| **style == text)
        .copied()
        .unwrap_or("openai")
}

/// `normalize_provider`: alias, minúsculas y `opencode` si queda vacío.
pub fn normalize_provider(name: Option<&str>) -> String {
    let value = py_strip(name.unwrap_or("")).to_lowercase();
    let value = match value.as_str() {
        "claude" => "anthropic".to_string(),
        "google" => "gemini".to_string(),
        "open-ai" | "open_ai" => "openai".to_string(),
        _ => value,
    };
    if value.is_empty() {
        "opencode".to_string()
    } else {
        value
    }
}

/// `provider_spec`: el preset del proveedor (ya normalizado) o `None`.
pub fn provider_spec(name: Option<&str>) -> Option<&'static ProviderSpec> {
    let wanted = normalize_provider(name);
    PROVIDERS.iter().find(|spec| spec.name == wanted)
}

/// `provider_models`: modelos sugeridos (vacío para uno personalizado).
pub fn provider_models(provider: Option<&str>) -> &'static [(&'static str, &'static str)] {
    let wanted = normalize_provider(provider);
    MODEL_CATALOG
        .iter()
        .find(|(name, _)| *name == wanted)
        .map_or(&[], |(_, models)| *models)
}

/// `needs_api_key`: solo la nube exige clave; un endpoint local no.
pub fn needs_api_key(provider: Option<&str>, base_url: &str) -> bool {
    match provider_spec(provider) {
        None => false,
        Some(spec) if spec.name == CUSTOM_PROVIDER => false,
        Some(_) => !is_local_endpoint(base_url),
    }
}

/// `ModelCapabilities`: lo que un modelo/endpoint acepta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub tools: bool,
    pub temperature: bool,
    pub max_output: u32,
    pub reasoning: bool,
}

fn reasoning_name() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?:^|[/_.:-])(?:r1|reasoner|o1|o3|o4|thinking)(?:$|[/_.:-])")
            .expect("patrón de razonamiento válido")
    })
}

/// `capabilities_for`: el nombre del modelo decide el razonamiento.
pub fn capabilities_for(
    _provider: Option<&str>,
    model: Option<&str>,
    _api_style: &str,
) -> ModelCapabilities {
    // DECISION: igual que Python, `provider` y `api_style` son solo contexto.
    let name = py_strip(model.unwrap_or("")).to_lowercase();
    ModelCapabilities {
        tools: true,
        temperature: true,
        max_output: 32_000,
        reasoning: reasoning_name().is_match(&name),
    }
}

/// `match_model`: completa un id sin prefijo si la coincidencia es inequívoca.
pub fn match_model(model: Option<&str>, available: &[String]) -> String {
    let wanted = py_strip(model.unwrap_or(""));
    if wanted.is_empty() || available.is_empty() || available.iter().any(|item| item == wanted) {
        return wanted.to_string();
    }
    let lowered = wanted.to_lowercase();
    let exact: Vec<&String> = available
        .iter()
        .filter(|item| item.to_lowercase() == lowered)
        .collect();
    if let [only] = exact.as_slice() {
        return (*only).clone();
    }
    let tail = format!("/{lowered}");
    let suffix: Vec<&String> = available
        .iter()
        .filter(|item| item.to_lowercase().ends_with(&tail))
        .collect();
    match suffix.as_slice() {
        [only] => (*only).clone(),
        _ => wanted.to_string(),
    }
}

/// `models_url`: `GET /models` de un endpoint.
pub fn models_url(base_url: &str, api_style: &str) -> String {
    let base = py_strip(base_url).trim_end_matches('/');
    if normalize_api_style(Some(api_style)) == "anthropic" && !base.ends_with("/v1") {
        return format!("{base}/v1/models");
    }
    format!("{base}/models")
}

/// Modelos que no sirven para conversar (audio, imágenes, embeddings…).
const NON_CHAT_HINTS: [&str; 14] = [
    "embed",
    "whisper",
    "tts",
    "dall-e",
    "moderation",
    "transcribe",
    "image",
    "audio",
    "realtime",
    "guard",
    "rerank",
    "search",
    "sora",
    "orpheus",
];

/// `fetch_models`: pide la lista real a la API (llamada de red: usar en un hilo).
///
/// Los errores llevan el mismo texto que Python y nunca la clave.
pub fn fetch_models(
    base_url: &str,
    api_key: &str,
    api_style: &str,
    timeout: Duration,
    limit: usize,
) -> Result<Vec<String>, String> {
    let style = normalize_api_style(Some(api_style));
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent("EzAgent-Voice/0.1")
        .build()
        .map_err(|_| "No pude pedir los modelos: HTTPError".to_string())?;
    let mut url = models_url(base_url, style);
    if style == "anthropic" {
        url.push_str("?limit=1000");
    }
    let mut request = client.get(url);
    if style == "anthropic" {
        request = request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    } else if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .map_err(|error| format!("No pude pedir los modelos: {}", error_class(&error)))?;
    let status = response.status().as_u16();
    if status >= 400 {
        return Err(format!("La lista de modelos respondió {status}."));
    }
    let payload: Value = response
        .json()
        .map_err(|_| "La lista de modelos no es JSON.".to_string())?;
    Ok(chat_model_ids(&payload, limit))
}

/// Nombre de la excepción de `httpx` que corresponde al error de `reqwest`.
fn error_class(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        if error.is_connect() {
            "ConnectTimeout"
        } else {
            "ReadTimeout"
        }
    } else if error.is_connect() {
        "ConnectError"
    } else if error.is_request() {
        "UnsupportedProtocol"
    } else {
        "ReadError"
    }
}

fn chat_model_ids(payload: &Value, limit: usize) -> Vec<String> {
    let items = match payload {
        Value::Object(map) => map.get("data").cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    let mut ids: Vec<String> = Vec::new();
    for item in items.as_array().into_iter().flatten() {
        let ident = match item {
            Value::Object(map) => map.get("id").cloned().unwrap_or(Value::Null),
            other => other.clone(),
        };
        let text = if crate::config::truthy(&ident) {
            py_strip(&crate::config::python_str(&ident)).to_string()
        } else {
            String::new()
        };
        let text = text.strip_prefix("models/").unwrap_or(&text).to_string();
        let lowered = text.to_lowercase();
        let chat = !NON_CHAT_HINTS.iter().any(|hint| lowered.contains(hint));
        if !text.is_empty() && chat && !ids.contains(&text) {
            ids.push(text);
        }
    }
    // `sorted(set[str])` ordena por punto de código, no por bytes UTF-8.
    ids.sort_by(|left, right| left.chars().cmp(right.chars()));
    ids.truncate(limit);
    ids
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod extra_tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ez-llm-extra-{}-{label}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("llm.json")
    }

    #[test]
    fn presets_and_catalog_follow_the_python_tables() {
        let mut environments: Vec<&str> = Vec::new();
        for spec in PROVIDERS.iter() {
            assert_eq!(spec.name, normalize_provider(Some(spec.name)).as_str());
            assert!(API_STYLES.contains(&spec.api_style), "{}", spec.name);
            if spec.name != CUSTOM_PROVIDER {
                assert!(spec.base_url.starts_with("https://"), "{}", spec.name);
                assert!(!spec.default_model.is_empty(), "{}", spec.name);
                assert!(spec.api_key_env.ends_with("_API_KEY"), "{}", spec.name);
                assert!(!environments.contains(&spec.api_key_env));
                environments.push(spec.api_key_env);
                let models = provider_models(Some(spec.name));
                assert!(
                    models.iter().any(|(id, _)| *id == spec.default_model),
                    "{}",
                    spec.name
                );
                let mut ids: Vec<&str> = models.iter().map(|(id, _)| *id).collect();
                ids.sort_unstable();
                ids.dedup();
                assert_eq!(ids.len(), models.len(), "{}", spec.name);
            }
        }
        assert_eq!(
            provider_spec(Some("claude")).map(|spec| spec.name),
            Some("anthropic")
        );
        assert_eq!(
            provider_models(Some("claude")),
            provider_models(Some("anthropic"))
        );
        assert!(provider_models(Some("custom")).is_empty());
        assert!(provider_models(Some("nada")).is_empty());
    }

    #[test]
    fn saving_named_profiles_keeps_the_others_and_no_temp_files() {
        let file = scratch("saved");
        let save = |label: &str, model: &str| {
            save_provider_profile(&SaveArgs {
                provider: "custom",
                model,
                base_url: "http://localhost:1234/v1",
                api_key: "",
                api_style: "",
                profile_name: label,
                config_file: Some(&file),
            })
            .expect("perfil guardado")
        };
        assert_eq!(save("Rápido", "fast"), "custom");
        assert_eq!(save("Preciso", "careful"), "custom");
        let catalog = load_provider_catalog(Some(&HashMap::new()), Some(&file)).expect("catálogo");
        assert_eq!(catalog["saved:rápido"].model, "fast");
        assert_eq!(catalog["saved:preciso"].model, "careful");
        assert_eq!(catalog["saved:rápido"].provider.as_deref(), Some("custom"));
        assert_eq!(catalog["saved:rápido"].label, "Rápido");
        assert!(catalog["saved:rápido"].saved);
        save("Rápido", "updated");
        let updated = load_provider_catalog(Some(&HashMap::new()), Some(&file)).expect("catálogo");
        assert_eq!(updated["saved:rápido"].model, "updated");
        assert_eq!(updated["saved:preciso"].model, "careful");
        let entries: Vec<String> = std::fs::read_dir(file.parent().expect("carpeta"))
            .expect("carpeta")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["llm.json".to_string()]);

        std::fs::write(&file, "{corrupto").expect("archivo corrupto");
        let error = save_provider_profile(&SaveArgs {
            provider: "openai",
            model: "m",
            base_url: "https://x/v1",
            api_key: "k",
            config_file: Some(&file),
            ..SaveArgs::default()
        })
        .unwrap_err();
        assert!(error.starts_with("llm.json inválido"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&file).expect("archivo"),
            "{corrupto"
        );
    }

    #[test]
    fn resolved_values_are_trimmed_and_the_key_is_never_shown() {
        let file = scratch("resolved");
        std::fs::write(&file, "{}").expect("llm.json");
        let args = ResolveArgs {
            provider: Some("openai"),
            model: Some("  m  "),
            base_url: Some(" https://x/v1 "),
            api_key: Some(" k \n"),
            api_style: None,
            environ: Some(&HashMap::new()),
            config_file: Some(&file),
        };
        let resolved = resolve_llm(&args).expect("configuración válida");
        assert_eq!(resolved.model, "m");
        assert_eq!(resolved.api_key, "k");
        assert_eq!(resolved.base_url, "https://x/v1");
        let secret = ResolveArgs {
            api_key: Some("SUPER-SECRETA"),
            ..args
        };
        let resolved = resolve_llm(&secret).expect("configuración válida");
        assert!(!format!("{resolved:?}").contains("SUPER-SECRETA"));
    }
}
