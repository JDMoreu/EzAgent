//! `_apply_live_setting`: un control del menú se aplica sin persistirse.

use ez_core::llm_providers::{load_provider_catalog, normalize_provider};
use ez_core::pyvalue::py_strip;
use ez_hud::model::settings::SettingValue;
use ez_hud::win::drop::ConfigValue;
use serde_json::{Map, Value};

use crate::hotkeys::parse_hotkey;

use super::super::{
    setting_float, setting_from_config, setting_int, setting_rounded_int, setting_text_or_empty,
};
use super::mic::device_option;
use crate::app::core::{cfg_text, VoiceApp, HOTKEY_KEYS};
use crate::app::startup::check_hotkey_duplicates;

impl VoiceApp {
    /// `_apply_live_setting`, con el `except` del Python en el llamador.
    pub(in crate::app::menu_wiring) fn apply_live_checked(
        &self,
        key: &str,
        value: &ConfigValue,
    ) -> Result<(), String> {
        let setting = setting_from_config(value);
        match key {
            "input_device_index" => self.live_device(&setting),
            "llm_profile" | "llm_saved_profile" => {
                self.apply_live_profile(&setting);
                Ok(())
            }
            "voice" => self.live_voice(&setting),
            "llm_provider" => {
                let provider = normalize_provider(Some(&setting.python_str()));
                self.inner
                    .config
                    .cfg
                    .write()
                    .set("llm_provider", Value::String(provider));
                Ok(())
            }
            "llm_profile_name" => {
                let name = py_strip(&setting.python_str()).to_string();
                self.inner
                    .config
                    .cfg
                    .write()
                    .set("llm_profile_name", Value::String(name));
                Ok(())
            }
            "llm_model" | "llm_model_pick" => {
                let model = py_strip(&setting_text_or_empty(Some(&setting))).to_string();
                if !model.is_empty() {
                    self.inner
                        .config
                        .cfg
                        .write()
                        .set("llm_model", Value::String(model));
                }
                Ok(())
            }
            "llm_max_tokens" | "llm_context_tokens" => {
                let number = setting_rounded_int(&setting)?;
                self.inner.config.cfg.write().set(key, Value::from(number));
                Ok(())
            }
            "smart_turn" | "barge_in" | "confirm_actions" => self.live_flag(key, &setting),
            _ => self.apply_live_rest(key, &setting),
        }
    }

    fn apply_live_rest(&self, key: &str, setting: &SettingValue) -> Result<(), String> {
        match key {
            "auto_start_whisper" => {
                self.inner
                    .config
                    .cfg
                    .write()
                    .set(key, Value::Bool(setting.truthy()));
                Ok(())
            }
            _ if float_key(key).is_some() => self.live_float(key, setting),
            _ if string_key(key) => {
                self.inner
                    .config
                    .cfg
                    .write()
                    .set(key, Value::String(setting.python_str()));
                Ok(())
            }
            _ if HOTKEY_KEYS.contains(&key) => {
                let spec = py_strip(&setting_text_or_empty(Some(setting))).to_string();
                self.apply_live_hotkey(key, spec)
            }
            _ => Ok(()),
        }
    }

    fn live_device(&self, setting: &SettingValue) -> Result<(), String> {
        if matches!(setting, SettingValue::None) {
            return Ok(());
        }
        let index = setting_int(setting)?;
        if index < 0 {
            return Ok(());
        }
        let current = self
            .inner
            .audio
            .mic
            .lock()
            .device_index
            .map(|index| index as i64);
        if Some(index) != current {
            self.reload_mic_with(device_option(index), true, self);
        }
        Ok(())
    }

    fn live_voice(&self, setting: &SettingValue) -> Result<(), String> {
        let voice = setting.python_str();
        self.inner
            .config
            .cfg
            .write()
            .set("voice", Value::String(voice.clone()));
        self.inner.speech.speaker.set_voice(&voice)
    }

    fn live_flag(&self, key: &str, setting: &SettingValue) -> Result<(), String> {
        let flag = setting.truthy();
        self.inner.config.cfg.write().set(key, Value::Bool(flag));
        match key {
            "smart_turn" => self.apply_turn_settings(),
            "confirm_actions" => {
                self.sync_messenger();
                if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
                    messenger.set_confirm_actions(flag);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn live_float(&self, key: &str, setting: &SettingValue) -> Result<(), String> {
        let number = setting_float(setting)?;
        self.inner.config.cfg.write().set(key, Value::from(number));
        if float_key(key) == Some(true) {
            let mut mic = self.inner.audio.mic.lock();
            match key {
                "min_rms" => mic.min_rms = number,
                "noise_multiplier" => mic.noise_multiplier = number,
                "silence_secs" => mic.silence_secs = number,
                "pre_roll_secs" => mic.pre_roll_secs = number,
                "min_speech_secs" => mic.min_speech_secs = number,
                "max_utterance_secs" => mic.max_utterance_secs = number,
                "barge_in_rms" => mic.barge_in_rms = number,
                _ => {}
            }
        }
        if key == "turn_threshold" {
            self.apply_turn_settings();
        }
        Ok(())
    }

    fn apply_live_profile(&self, setting: &SettingValue) {
        let name = setting.python_str();
        let Ok(catalog) = load_provider_catalog(None, None) else {
            return;
        };
        let Some(entry) = catalog.get(&name) else {
            return;
        };
        {
            let mut cfg = self.inner.config.cfg.write();
            cfg.set(
                "llm_provider",
                Value::String(normalize_provider(Some(
                    entry.provider.as_deref().unwrap_or(&name),
                ))),
            );
            cfg.set(
                "llm_profile_name",
                Value::String(if entry.saved {
                    entry.label.clone()
                } else {
                    String::new()
                }),
            );
            cfg.set(
                "llm_saved_profile",
                Value::String(if entry.saved { name } else { String::new() }),
            );
            cfg.set("llm_model", Value::String(entry.model.clone()));
            cfg.set("llm_base_url", Value::String(entry.base_url.clone()));
            cfg.set("api_key", Value::String(entry.api_key.clone()));
            cfg.set("llm_api_style", Value::String(entry.api_style.clone()));
        }
        self.sync_messenger();
        if let Some(messenger) = self.inner.agents.messenger.lock().clone() {
            messenger.reload_llm();
        }
        let cfg = self.config();
        self.fetch_llm_models(
            &cfg_text(&cfg, "llm_provider", ""),
            &cfg_text(&cfg, "llm_base_url", ""),
            &cfg_text(&cfg, "api_key", ""),
            &cfg_text(&cfg, "llm_api_style", ""),
            true,
        );
    }

    fn apply_live_hotkey(&self, key: &str, spec: String) -> Result<(), String> {
        parse_hotkey(&spec)?;
        let cfg = self.config();
        let mut candidate = Map::new();
        for name in HOTKEY_KEYS {
            candidate.insert(name.to_string(), Value::String(cfg_text(&cfg, name, "")));
        }
        candidate.insert(key.to_string(), Value::String(spec.clone()));
        check_hotkey_duplicates(&candidate)?;
        self.inner.config.cfg.write().set(key, Value::String(spec));
        self.reload_hotkeys();
        Ok(())
    }
}

/// `(clave, también se aplica al micrófono en vivo)`.
const FLOAT_KEYS: [(&str, bool); 12] = [
    ("min_rms", true),
    ("noise_multiplier", true),
    ("silence_secs", true),
    ("pre_roll_secs", true),
    ("min_speech_secs", true),
    ("max_utterance_secs", true),
    ("barge_in_rms", true),
    ("speed", false),
    ("tts_sentence_pause", false),
    ("tts_clause_pause", false),
    ("stt_timeout_secs", false),
    ("llm_temperature", false),
];

fn float_key(key: &str) -> Option<bool> {
    if key == "turn_threshold" {
        return Some(false);
    }
    FLOAT_KEYS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, live)| *live)
}

fn string_key(key: &str) -> bool {
    matches!(
        key,
        "tts_model_variant"
            | "kokoro_model_path"
            | "kokoro_voices_path"
            | "whisper_url"
            | "stt_model"
            | "stt_engine"
            | "stt_prompt"
            | "stt_dictionary"
            | "llm_api_style"
            | "llm_base_url"
            | "api_key"
            | "llm_reasoning_effort"
    )
}
