//! Resolución de proveedor/modelo/clave para `VoiceConfig::apply_env`.
//!
//! Delega en `llm_providers::resolve_llm` (F2-03), que porta el Python.

use crate::llm_providers::{self, ResolveArgs};

/// Lo que `resolve_llm` de `llm_providers.py` devuelve y `apply_env` usa.
pub(super) struct ResolvedLlm {
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) api_style: String,
    pub(super) source: &'static str,
}

pub(super) fn resolve_llm(
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    api_key: Option<&str>,
    api_style: Option<&str>,
) -> Result<ResolvedLlm, String> {
    let args = ResolveArgs {
        provider,
        model,
        base_url,
        api_key,
        api_style,
        ..ResolveArgs::default()
    };
    let resolved = llm_providers::resolve_llm(&args)?;
    Ok(ResolvedLlm {
        provider: resolved.provider,
        model: resolved.model,
        base_url: resolved.base_url,
        api_key: resolved.api_key,
        api_style: resolved.api_style.to_string(),
        source: resolved.source,
    })
}

pub(super) fn env_text(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
