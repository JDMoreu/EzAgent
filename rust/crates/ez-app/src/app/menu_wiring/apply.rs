//! `_apply_settings` y sus workers (`app.py 2930-3395`, F6-06b).
//!
//! `_apply_settings` valida todo lo candidato antes de tocar `self.cfg`: un
//! endpoint, modelo, voz o ruta inválidos no dejan cambios a medias. Las
//! llamadas largas (red de modelos, Kokoro, whisper) van en sus hilos.

mod live;
mod mic;
mod workers;

use ez_core::config::VoiceConfig;
use ez_core::llm_providers::{match_model, normalize_provider, save_provider_profile, SaveArgs};
use ez_core::models::resolve_kokoro_files;
use ez_core::pyvalue::{casefold, py_strip, repr_str};
use ez_hud::model::settings::{normalize_setting, SettingValue, SettingsValues};
use serde_json::{Map, Value};

use crate::hotkeys::parse_hotkey;

use super::super::core::state::SavedLlm;
use super::super::core::{
    cfg_float, cfg_text, cfg_truthy, log, VoiceApp, HOTKEY_KEYS, INSTANT_SETTINGS,
};
use super::super::startup::check_hotkey_duplicates;
use super::{
    profile_name_too_long, setting_float, setting_int, setting_python_str_or, setting_rounded_int,
    setting_text_or_cfg, setting_text_or_empty, setting_to_json, MenuEffects,
};
use mic::{device_option, update_mic_settings};

/// Campos del candidato LLM antes de escribir la configuración.
struct CandidateLlm {
    provider: String,
    model: String,
    base_url: String,
    api_key: String,
    api_style: String,
}

impl CandidateLlm {
    fn saved(&self) -> SavedLlm {
        SavedLlm {
            provider: self.provider.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            api_style: self.api_style.clone(),
        }
    }
}

/// Campos de voz/TTS del candidato.
struct CandidateTts {
    voice: String,
    variant: String,
    model_path: String,
    voices_path: String,
}

impl CandidateTts {
    fn tuple(&self) -> (String, String, String) {
        (
            self.variant.clone(),
            self.model_path.clone(),
            self.voices_path.clone(),
        )
    }
}

struct Prepared {
    dictionary: String,
    profile_name: String,
    old_device: Option<i64>,
    was_in_call: bool,
    old_hotkeys: [String; 6],
    hotkeys: Map<String, Value>,
    old_llm: SavedLlm,
    old_tts: (String, String, String),
    llm: CandidateLlm,
    tts: CandidateTts,
}

impl VoiceApp {
    /// `_apply_settings`: aplica y guarda los valores del panel; `true` si
    /// reinició la llamada.
    pub fn apply_settings(&self, values: &SettingsValues) -> Result<bool, String> {
        self.apply_settings_with(values, self)
    }

    /// Igual que [`Self::apply_settings`] con efectos intercambiables.
    pub fn apply_settings_with(
        &self,
        values: &SettingsValues,
        effects: &dyn MenuEffects,
    ) -> Result<bool, String> {
        let prepared = self.prepare_settings(values, effects)?;
        self.write_settings(&prepared, values)?;
        self.apply_turn_settings();
        self.save_profiles(&prepared)?;
        if prepared.llm.saved() != prepared.old_llm {
            self.fetch_llm_models(
                &prepared.llm.provider,
                &prepared.llm.base_url,
                &prepared.llm.api_key,
                &prepared.llm.api_style,
                true,
            );
        }
        let device_changed = self.apply_device(&prepared, values, effects)?;
        self.apply_tts(&prepared)?;
        self.sync_messenger();
        let cfg = self.config();
        if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
            messenger.set_confirm_actions(cfg_truthy(&cfg, "confirm_actions", false));
        }
        if let Some(agent) = self.inner.agents.agent.lock().as_ref() {
            agent.set_confirm_actions(cfg_truthy(&cfg, "confirm_actions", false));
        }
        let llm_changed = prepared.llm.saved() != prepared.old_llm;
        if llm_changed {
            if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
                messenger.reload_llm();
            }
        }
        // DECISION: `voice.json` de la instancia, no `config_path()`.
        self.inner
            .config
            .cfg
            .read()
            .save(Some(&self.inner.config.config_file))?;
        self.finish_hotkeys(&prepared);
        *self.inner.config.saved_llm.lock() = prepared.llm.saved();
        self.finish_stt();
        let restarted = self.finish_call(&prepared, device_changed, effects);
        log("Ajustes guardados.");
        Ok(restarted)
    }

    // ---- preparación ------------------------------------------------------

    fn prepare_settings(
        &self,
        values: &SettingsValues,
        effects: &dyn MenuEffects,
    ) -> Result<Prepared, String> {
        let cfg = self.config();
        if profile_name_too_long(values) {
            return Err("El nombre de la configuración admite hasta 80 caracteres.".to_string());
        }
        let dictionary = self.dictionary_value(values)?;
        let hotkeys = self.hotkey_values(values, &cfg)?;
        let old_llm = self.inner.config.saved_llm.lock().clone();
        let old_tts = tts_tuple(&cfg);
        let llm = self.candidate_llm(values, &cfg, &old_llm)?;
        let tts = candidate_tts(values, &cfg, &old_tts, effects)?;
        Ok(Prepared {
            dictionary,
            profile_name: profile_name(values, &cfg),
            old_device: self
                .inner
                .audio
                .mic
                .lock()
                .device_index
                .map(|index| index as i64),
            was_in_call: self
                .inner
                .session
                .in_call
                .load(std::sync::atomic::Ordering::SeqCst),
            old_hotkeys: hotkey_tuple(&cfg),
            hotkeys,
            old_llm,
            old_tts,
            llm,
            tts,
        })
    }

    fn dictionary_value(&self, values: &SettingsValues) -> Result<String, String> {
        let raw = match values.get("stt_dictionary") {
            Some(value) => setting_text_or_empty(Some(value)),
            None => cfg_text(&self.config(), "stt_dictionary", ""),
        };
        let normalized = normalize_setting("stt_dictionary", &SettingValue::Text(raw))
            .map_err(|error| error.to_string())?;
        Ok(normalized.python_str())
    }

    fn hotkey_values(
        &self,
        values: &SettingsValues,
        cfg: &VoiceConfig,
    ) -> Result<Map<String, Value>, String> {
        let mut hotkeys = Map::new();
        for key in HOTKEY_KEYS {
            let spec = py_strip(&setting_text_or_cfg(values, key, cfg)).to_string();
            parse_hotkey(&spec).map_err(|error| format!("Atajo {key}: {error}"))?;
            hotkeys.insert(key.to_string(), Value::String(spec));
        }
        check_hotkey_duplicates(&hotkeys)?;
        Ok(hotkeys)
    }

    fn candidate_llm(
        &self,
        values: &SettingsValues,
        cfg: &VoiceConfig,
        old_llm: &SavedLlm,
    ) -> Result<CandidateLlm, String> {
        let provider =
            normalize_provider(Some(&setting_python_str_or(values, "llm_provider", cfg)));
        let mut candidate = CandidateLlm {
            provider,
            model: py_strip(&setting_python_str_or(values, "llm_model", cfg)).to_string(),
            base_url: py_strip(&setting_python_str_or(values, "llm_base_url", cfg)).to_string(),
            api_key: py_strip(&setting_python_str_or(values, "api_key", cfg)).to_string(),
            api_style: {
                let raw = setting_text_or_cfg(values, "llm_api_style", cfg);
                if raw.is_empty() {
                    "openai".to_string()
                } else {
                    raw
                }
            },
        };
        if candidate.saved() != *old_llm {
            if candidate.base_url.is_empty() {
                return Err("El proveedor LLM necesita un endpoint (URL).".to_string());
            }
            if candidate.model.is_empty() {
                return Err("El proveedor LLM necesita un modelo.".to_string());
            }
            let known = self
                .inner
                .config
                .llm_models
                .lock()
                .cache
                .get(&candidate.provider)
                .cloned()
                .unwrap_or_default();
            let fixed = match_model(Some(&candidate.model), &known);
            if fixed != candidate.model {
                log(format!(
                    "Modelo «{}» completado como «{fixed}».",
                    candidate.model
                ));
                candidate.model = fixed;
            }
        }
        Ok(candidate)
    }

    // ---- escritura --------------------------------------------------------

    fn write_settings(&self, prepared: &Prepared, values: &SettingsValues) -> Result<(), String> {
        let mut cfg = self.inner.config.cfg.write();
        write_numbers(&mut cfg, values)?;
        write_texts(&mut cfg, prepared, values);
        write_llm(&mut cfg, prepared);
        write_counters(&mut cfg, values)?;
        for key in ["barge_in", "smart_turn", "confirm_actions"] {
            write_bool(&mut cfg, values, key)?;
        }
        write_bool(&mut cfg, values, "auto_start_whisper")?;
        for (key, value) in &prepared.hotkeys {
            cfg.set(key, value.clone());
        }
        for key in INSTANT_SETTINGS {
            if let Some(value) = values.get(key) {
                cfg.set(key, setting_to_json(value));
            }
        }
        Ok(())
    }

    fn save_profiles(&self, prepared: &Prepared) -> Result<(), String> {
        let cfg = self.config();
        let provider = cfg_text(&cfg, "llm_provider", "");
        let model = cfg_text(&cfg, "llm_model", "");
        let base_url = cfg_text(&cfg, "llm_base_url", "");
        let api_key = cfg_text(&cfg, "api_key", "");
        let api_style = cfg_text(&cfg, "llm_api_style", "");
        if prepared.llm.saved() != prepared.old_llm {
            save_provider_profile(&SaveArgs {
                provider: &provider,
                model: &model,
                base_url: &base_url,
                api_key: &api_key,
                api_style: &api_style,
                profile_name: &prepared.profile_name,
                config_file: None,
            })?;
            self.inner
                .config
                .cfg
                .write()
                .set("llm_pinned", Value::Bool(true));
        } else if !prepared.profile_name.is_empty() {
            save_provider_profile(&SaveArgs {
                provider: &provider,
                model: &model,
                base_url: &base_url,
                api_key: &api_key,
                api_style: &api_style,
                profile_name: &prepared.profile_name,
                config_file: None,
            })?;
        }
        Ok(())
    }

    fn apply_device(
        &self,
        prepared: &Prepared,
        values: &SettingsValues,
        effects: &dyn MenuEffects,
    ) -> Result<bool, String> {
        let new_device = match values.get("input_device_index") {
            None | Some(SettingValue::None) => None,
            Some(value) => Some(setting_int(value)?),
        };
        match new_device {
            Some(index) if Some(index) != prepared.old_device => {
                self.reload_mic_with(device_option(index), false, effects);
                Ok(true)
            }
            _ => {
                update_mic_settings(self);
                Ok(false)
            }
        }
    }

    fn apply_tts(&self, prepared: &Prepared) -> Result<(), String> {
        let cfg = self.config();
        self.inner.speech.speaker.set_config(cfg.clone());
        if prepared.tts.tuple() != prepared.old_tts {
            // Cargar Kokoro tarda: se hace en un worker para no congelar la UI.
            self.reload_tts_async();
        } else {
            self.inner
                .speech
                .speaker
                .set_voice(&cfg_text(&cfg, "voice", ""))?;
        }
        Ok(())
    }

    fn sync_messenger(&self) {
        if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
            messenger.apply_config(self.config());
        }
    }

    fn finish_hotkeys(&self, prepared: &Prepared) {
        if hotkey_tuple(&self.config()) != prepared.old_hotkeys {
            self.reload_hotkeys();
        }
    }

    fn finish_stt(&self) {
        let cfg = self.config();
        let model = cfg_text(&cfg, "stt_model", "");
        let engine = cfg_text(&cfg, "stt_engine", "");
        let mut saved_model = self.inner.config.saved_stt_model.write();
        let mut saved_engine = self.inner.config.saved_stt_engine.write();
        if model != *saved_model || engine != *saved_engine {
            *saved_model = model;
            *saved_engine = engine;
            drop(saved_model);
            drop(saved_engine);
            self.restart_whisper_async();
        }
    }

    fn finish_call(
        &self,
        prepared: &Prepared,
        device_changed: bool,
        effects: &dyn MenuEffects,
    ) -> bool {
        if !(prepared.was_in_call && device_changed) {
            return false;
        }
        // Los umbrales del VAD se leen en cada bloque; el dispositivo sí exige
        // reabrir el stream.
        if self
            .inner
            .session
            .in_call
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            effects.hang_up();
        }
        super::schedule_start_call(self);
        true
    }
}

fn write_numbers(cfg: &mut VoiceConfig, values: &SettingsValues) -> Result<(), String> {
    for key in [
        "min_rms",
        "silence_secs",
        "noise_multiplier",
        "pre_roll_secs",
        "min_speech_secs",
        "max_utterance_secs",
        "barge_in_rms",
        "speed",
        "tts_sentence_pause",
        "tts_clause_pause",
        "stt_timeout_secs",
        "llm_temperature",
        "turn_threshold",
    ] {
        let value = match values.get(key) {
            Some(value) => setting_float(value)?,
            None => cfg_float(cfg, key, 0.0),
        };
        cfg.set(key, Value::from(value));
    }
    Ok(())
}

fn write_texts(cfg: &mut VoiceConfig, prepared: &Prepared, values: &SettingsValues) {
    for key in [
        "whisper_url",
        "stt_model",
        "stt_engine",
        "stt_prompt",
        "llm_reasoning_effort",
    ] {
        let value = setting_python_str_or(values, key, cfg);
        cfg.set(key, Value::String(value));
    }
    for (key, value) in [
        ("tts_model_variant", &prepared.tts.variant),
        ("kokoro_model_path", &prepared.tts.model_path),
        ("kokoro_voices_path", &prepared.tts.voices_path),
        ("voice", &prepared.tts.voice),
        ("stt_dictionary", &prepared.dictionary),
    ] {
        cfg.set(key, Value::String(value.clone()));
    }
}

fn write_llm(cfg: &mut VoiceConfig, prepared: &Prepared) {
    cfg.set("llm_provider", Value::String(prepared.llm.provider.clone()));
    cfg.set("llm_model", Value::String(prepared.llm.model.clone()));
    cfg.set(
        "llm_profile_name",
        Value::String(prepared.profile_name.clone()),
    );
    cfg.set(
        "llm_saved_profile",
        Value::String(if prepared.profile_name.is_empty() {
            String::new()
        } else {
            format!("saved:{}", casefold(&prepared.profile_name))
        }),
    );
    cfg.set("llm_base_url", Value::String(prepared.llm.base_url.clone()));
    cfg.set("api_key", Value::String(prepared.llm.api_key.clone()));
    cfg.set(
        "llm_api_style",
        Value::String(prepared.llm.api_style.clone()),
    );
}

fn write_counters(cfg: &mut VoiceConfig, values: &SettingsValues) -> Result<(), String> {
    for key in ["llm_max_tokens", "llm_context_tokens"] {
        let default = cfg_float(cfg, key, 0.0).round_ties_even() as i64;
        let value = match values.get(key) {
            Some(value) => setting_rounded_int(value)?,
            None => default,
        };
        cfg.set(key, Value::from(value));
    }
    Ok(())
}

fn write_bool(cfg: &mut VoiceConfig, values: &SettingsValues, key: &str) -> Result<(), String> {
    let value = match values.get(key) {
        Some(value) => value.truthy(),
        None => cfg_truthy(cfg, key, false),
    };
    cfg.set(key, Value::Bool(value));
    Ok(())
}

fn tts_tuple(cfg: &VoiceConfig) -> (String, String, String) {
    (
        cfg_text(cfg, "tts_model_variant", ""),
        cfg_text(cfg, "kokoro_model_path", ""),
        cfg_text(cfg, "kokoro_voices_path", ""),
    )
}

fn profile_name(values: &SettingsValues, cfg: &VoiceConfig) -> String {
    let raw = match values.get("llm_profile_name") {
        Some(value) => value.python_str(),
        None => cfg_text(cfg, "llm_profile_name", ""),
    };
    py_strip(&raw).to_string()
}

fn hotkey_tuple(cfg: &VoiceConfig) -> [String; 6] {
    HOTKEY_KEYS.map(|key| cfg_text(cfg, key, ""))
}

fn candidate_tts(
    values: &SettingsValues,
    cfg: &VoiceConfig,
    old_tts: &(String, String, String),
    effects: &dyn MenuEffects,
) -> Result<CandidateTts, String> {
    let candidate = CandidateTts {
        voice: setting_python_str_or(values, "voice", cfg),
        variant: setting_python_str_or(values, "tts_model_variant", cfg),
        model_path: setting_python_str_or(values, "kokoro_model_path", cfg),
        voices_path: setting_python_str_or(values, "kokoro_voices_path", cfg),
    };
    let names: Vec<&str> = candidate
        .voice
        .split('+')
        .map(py_strip)
        .filter(|name| !name.is_empty())
        .collect();
    let valid = effects.kokoro_voices();
    if !valid.is_empty()
        && !names
            .iter()
            .any(|name| valid.iter().any(|voice| voice == name))
    {
        return Err(format!(
            "Ninguna voz de Kokoro válida en {}.",
            repr_str(&candidate.voice)
        ));
    }
    if candidate.tuple() != *old_tts {
        let (model, voices) = resolve_kokoro_files(
            &candidate.variant,
            Some(&candidate.model_path),
            Some(&candidate.voices_path),
        )
        .map_err(|error| format!("No pude preparar Kokoro: {error}"))?;
        if model
            .as_deref()
            .is_none_or(|path| !std::path::Path::new(path).is_file())
        {
            return Err("Falta el modelo de Kokoro; revisa la variante o la ruta.".to_string());
        }
        if voices
            .as_deref()
            .is_none_or(|path| !std::path::Path::new(path).is_file())
        {
            return Err("Faltan las voces de Kokoro; revisa la ruta.".to_string());
        }
    }
    Ok(candidate)
}
