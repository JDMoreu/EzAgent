//! Corrección del modelo, etiqueta del proveedor y workers de red, Kokoro y
//! whisper (`_fix_model_name`, `_llm_label`, `_fetch_llm_models`,
//! `_reload_tts_async`, `_restart_whisper_async`).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;

use ez_core::llm_providers::{match_model, provider_spec, save_provider_profile, SaveArgs};
use ez_core::pyvalue::py_strip;
use serde_json::Value;

use super::super::super::core::state::SavedLlm;
use super::super::super::core::{cfg_text, log, AppEvent, VoiceApp};

impl VoiceApp {
    /// `_fix_model_name`: completa el id del modelo activo y lo guarda.
    pub fn fix_model_name(&self, provider: &str, models: &[String]) {
        let cfg = self.config();
        if provider != cfg_text(&cfg, "llm_provider", "") {
            return;
        }
        let current = cfg_text(&cfg, "llm_model", "");
        let fixed = match_model(Some(&current), models);
        if fixed == current {
            return;
        }
        log(format!(
            "Modelo «{current}» corregido a «{fixed}» según {}.",
            self.llm_label()
        ));
        self.inner
            .config
            .cfg
            .write()
            .set("llm_model", Value::String(fixed.clone()));
        let save = || -> Result<(), String> {
            save_provider_profile(&SaveArgs {
                provider,
                model: &fixed,
                base_url: &cfg_text(&cfg, "llm_base_url", ""),
                api_key: &cfg_text(&cfg, "api_key", ""),
                api_style: &cfg_text(&cfg, "llm_api_style", ""),
                profile_name: "",
                config_file: None,
            })?;
            self.inner
                .config
                .cfg
                .read()
                .save(Some(&self.inner.config.config_file))
        };
        if let Err(error) = save() {
            log(format!("No pude guardar el modelo corregido: {error}"));
        }
        *self.inner.config.saved_llm.lock() = SavedLlm {
            provider: cfg_text(&cfg, "llm_provider", ""),
            model: fixed,
            base_url: cfg_text(&cfg, "llm_base_url", ""),
            api_key: cfg_text(&cfg, "api_key", ""),
            api_style: cfg_text(&cfg, "llm_api_style", ""),
        };
        self.sync_messenger();
        if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
            messenger.reload_llm();
        }
    }

    /// `_llm_label`: nombre del proveedor activo para los mensajes de error.
    pub fn llm_label(&self) -> String {
        let provider = cfg_text(&self.config(), "llm_provider", "");
        match provider_spec(Some(&provider)) {
            Some(spec) => spec.label.to_string(),
            None if provider.is_empty() => "LLM".to_string(),
            None => provider,
        }
    }

    /// `_fetch_llm_models`: pide la lista real de modelos en `ezagent-models`.
    pub fn fetch_llm_models(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        api_style: &str,
        force: bool,
    ) {
        if py_strip(base_url).is_empty() {
            return;
        }
        let attempt = (
            provider.to_string(),
            base_url.to_string(),
            api_style.to_string(),
        );
        {
            let mut models = self.inner.config.llm_models.lock();
            if !force && models.tried.contains(&attempt) {
                return;
            }
            models.tried.insert(attempt);
        }
        let app = self.clone();
        let provider = provider.to_string();
        let base_url = base_url.to_string();
        let api_key = api_key.to_string();
        let api_style = api_style.to_string();
        self.start_worker("ezagent-models", move || {
            let models = match ez_core::llm_providers::fetch_models(
                &base_url,
                &api_key,
                &api_style,
                std::time::Duration::from_secs(8),
                80,
            ) {
                Ok(models) => models,
                Err(error) => {
                    log(format!("Lista de modelos de {provider}: {error}"));
                    return;
                }
            };
            if models.is_empty() {
                return;
            }
            app.inner
                .config
                .llm_models
                .lock()
                .cache
                .insert(provider.clone(), models.clone());
            app.inner
                .ui
                .events
                .put(AppEvent::LlmModelsReady(provider.clone(), models.clone()));
            app.inner.ui.hud.update_llm_models(&provider, &models);
        });
    }

    /// `_reload_tts_async`: recarga Kokoro sin bloquear la interfaz.
    pub fn reload_tts_async(&self) {
        let app = self.clone();
        self.start_worker("ezagent-tts-reload", move || {
            let speaker = &app.inner.speech.speaker;
            speaker.set_config(app.config());
            if let Err(error) = speaker.reload_model() {
                app.inner
                    .ui
                    .events
                    .put(AppEvent::Error(format!("Kokoro: {error}"), None));
            }
        });
    }

    /// `_restart_whisper_async`: aplica el modelo si el servidor lo arrancó
    /// esta app.
    pub fn restart_whisper_async(&self) {
        if self.inner.runtime.whisper_proc.lock().is_none() {
            log("Modelo de Whisper guardado; se aplicará al reiniciar whisper-server.");
            return;
        }
        let app = self.clone();
        self.start_worker("ezagent-whisper-restart", move || {
            let child = app.inner.runtime.whisper_proc.lock().take();
            if let Some(mut child) = child {
                kill_tree(&mut child);
            }
            let mut cfg = app.config();
            let before = cfg_text(&cfg, "whisper_url", "");
            // DECISION: `ensure_whisper` de Rust devuelve `Option` (no lanza),
            // así que el `except` del Python no tiene ruta propia aquí.
            let child = ez_stt::server::ensure_whisper(&mut cfg);
            let after = cfg_text(&cfg, "whisper_url", "");
            if before != after {
                app.inner
                    .config
                    .cfg
                    .write()
                    .set("whisper_url", Value::String(after));
            }
            *app.inner.runtime.whisper_proc.lock() = child;
        });
        self.inner
            .turn
            .close_after_answer
            .store(true, Ordering::SeqCst);
    }
}

/// `taskkill /PID <pid> /T /F`, sin shell ni ventana.
fn kill_tree(child: &mut std::process::Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    let pid = child.id().to_string();
    let system_root =
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from("C:\\Windows"));
    let taskkill = PathBuf::from(system_root)
        .join("System32")
        .join("taskkill.exe");
    let mut command = Command::new(&taskkill);
    command
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .env_clear()
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    if command.status().is_err() {
        let _ = child.kill();
    }
}
