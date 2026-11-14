//! Presentación: textos de los valores y opciones de los selectores.

use super::model::MenuSettings;
use super::normalize::{as_bool, quantize, voice_name};
use super::value::{
    decimal, mask_secret, text_or_empty, SelectOption, SettingValue, SettingsError,
};
use super::{
    preset_label, FIXED_OPTIONS, LLM_REASONING_OPTIONS, SECRET_KEYS, SLIDERS, STT_ENGINE_OPTIONS,
    TEXT_KEYS, TOGGLE_KEYS, TTS_VARIANTS,
};

impl MenuSettings {
    /// Texto de un valor con el formato cerrado de la especificación.
    pub fn format(&self, key: &str) -> Result<String, SettingsError> {
        self.require(key)?;
        let value = self.values.get(key).cloned().unwrap_or(SettingValue::None);
        self.format_value(key, &value)
    }

    /// Igual que `format`, con un valor explícito.
    pub fn format_value(&self, key: &str, value: &SettingValue) -> Result<String, SettingsError> {
        self.require(key)?;
        if key == "ui_scale" {
            let spec = SLIDERS.get(key).expect("ui_scale es un slider");
            let percent = (quantize(value, spec) * 100.0).round_ties_even() as i64;
            return Ok(format!("{percent} %"));
        }
        if let Some(spec) = SLIDERS.get(key) {
            return Ok(decimal(quantize(value, spec), spec.decimals) + spec.suffix);
        }
        match key {
            "input_device_index" => Ok(self.device_label(value)),
            "voice" => Ok(self.voice_label(value)),
            "tts_model_variant" => Ok(option_or_text(TTS_VARIANTS.as_slice(), value)),
            "stt_engine" => Ok(option_or_text(STT_ENGINE_OPTIONS.as_slice(), value)),
            "llm_reasoning_effort" => Ok(option_or_text(LLM_REASONING_OPTIONS.as_slice(), value)),
            _ if FIXED_OPTIONS.contains_key(key) => Ok(option_or_text(
                FIXED_OPTIONS.get(key).map(Vec::as_slice).unwrap_or(&[]),
                value,
            )),
            "stt_model" => Ok(self.stt_model_label(value)),
            "llm_model" => Ok(model_text(value)),
            "llm_model_pick" => Ok(self.model_pick_label()),
            "theme_preset" => Ok(preset_label(&value.python_str())
                .unwrap_or("Elegir paleta")
                .to_string()),
            "llm_profile" | "llm_saved_profile" => Ok(self.profile_label(key, value)),
            "llm_provider" => Ok(self.provider_label(value)),
            _ if SECRET_KEYS.contains(&key) => Ok(mask_secret(value)),
            _ if TOGGLE_KEYS.contains(&key) => {
                Ok(if as_bool(value) { "Sí" } else { "No" }.to_string())
            }
            _ if TEXT_KEYS.contains(&key) => Ok(text_label(key, value)),
            _ => Ok(value.python_str()),
        }
    }

    /// Etiqueta del valor actual (lo que pinta la fila).
    pub fn value_label(&self, key: &str) -> Result<String, SettingsError> {
        self.format(key)
    }

    fn device_label(&self, value: &SettingValue) -> String {
        if self.devices.is_empty() {
            return "Sin micrófonos".into();
        }
        let Some(index) = super::value::int_of(value) else {
            return "Predeterminado del sistema".into();
        };
        match self.devices.iter().find(|device| device.index == index) {
            Some(device) => format!("{index}: {}", device.name),
            None => format!("{index}: no disponible"),
        }
    }

    fn voice_label(&self, value: &SettingValue) -> String {
        let ident = value.python_str();
        if let Some(option) = self
            .voices
            .iter()
            .find(|option| option.value.python_str() == ident)
        {
            return option.label.clone();
        }
        voice_name(&ident)
    }

    fn stt_model_label(&self, value: &SettingValue) -> String {
        if let Some(option) = self.stt_models.iter().find(|option| option.value == *value) {
            return option.label.clone();
        }
        model_text(value)
    }

    fn model_pick_label(&self) -> String {
        let count = self.model_options().len();
        match count {
            0 => "Sin lista".into(),
            1 => "1 disponible".into(),
            _ => format!("{count} disponibles"),
        }
    }

    fn profile_label(&self, key: &str, value: &SettingValue) -> String {
        if let Some(profile) = self.provider(&value.python_str()) {
            return profile.label.clone();
        }
        if key == "llm_saved_profile" {
            let name = text_or_empty(self.values.get("llm_profile_name"));
            if name.is_empty() {
                "Sin configuración guardada".into()
            } else {
                format!("Nueva · {name}")
            }
        } else {
            let text = text_or_empty(Some(value));
            if text.is_empty() {
                "Sin perfiles".into()
            } else {
                text
            }
        }
    }

    fn provider_label(&self, value: &SettingValue) -> String {
        if let Some(profile) = self.provider(&value.python_str()) {
            return profile.label.clone();
        }
        let text = text_or_empty(Some(value));
        if text.is_empty() {
            "Sin proveedor".into()
        } else {
            text
        }
    }

    /// Opciones del selector, en el orden en que se muestran.
    pub fn select_options(&self, key: &str) -> Result<Vec<SelectOption>, SettingsError> {
        match key {
            "input_device_index" => Ok(self
                .devices
                .iter()
                .map(|device| {
                    SelectOption::new(
                        SettingValue::Int(device.index),
                        format!("{}: {}", device.index, device.name),
                    )
                })
                .collect()),
            "voice" => Ok(self.voices.clone()),
            "tts_model_variant" => Ok(TTS_VARIANTS.clone()),
            "stt_model" => Ok(self.stt_models.clone()),
            "stt_engine" => Ok(STT_ENGINE_OPTIONS.clone()),
            "llm_reasoning_effort" => Ok(LLM_REASONING_OPTIONS.clone()),
            _ if FIXED_OPTIONS.contains_key(key) => {
                Ok(FIXED_OPTIONS.get(key).cloned().unwrap_or_default())
            }
            "llm_model_pick" => Ok(self.model_options()),
            "theme_preset" => Ok(super::PRESETS
                .iter()
                .map(|(name, _, _, _, _, label)| SelectOption::text(name, label))
                .collect()),
            "llm_profile" => Ok(self
                .providers
                .iter()
                .filter(|(_, profile)| !profile.is_saved())
                .map(|(name, profile)| {
                    SelectOption::new(profile_value(name), profile.label.clone())
                })
                .collect()),
            "llm_saved_profile" => {
                let mut options = vec![SelectOption::text("", "Elegir configuración")];
                options.extend(
                    self.providers
                        .iter()
                        .filter(|(_, profile)| profile.is_saved())
                        .map(|(name, profile)| {
                            SelectOption::new(profile_value(name), profile.label.clone())
                        }),
                );
                Ok(options)
            }
            _ => Err(SettingsError::NotSelector(key.to_string())),
        }
    }

    /// False solo para micrófono sin dispositivos (fila deshabilitada).
    pub fn selector_enabled(&self, key: &str) -> bool {
        match key {
            "input_device_index" => !self.devices.is_empty(),
            "voice" => !self.voices.is_empty(),
            "tts_model_variant" => true,
            "stt_model" => !self.stt_models.is_empty(),
            "stt_engine" => true,
            "llm_reasoning_effort" => true,
            _ if FIXED_OPTIONS.contains_key(key) => true,
            "llm_model_pick" => !self.model_options().is_empty(),
            "theme_preset" => true,
            "llm_profile" => !self.providers.is_empty(),
            "llm_saved_profile" => self.providers.iter().any(|(_, profile)| profile.is_saved()),
            _ => false,
        }
    }
}

fn profile_value(name: &str) -> SettingValue {
    SettingValue::Text(name.to_string())
}

fn option_or_text(options: &[SelectOption], value: &SettingValue) -> String {
    options
        .iter()
        .find(|option| option.value == *value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| value.python_str())
}

fn model_text(value: &SettingValue) -> String {
    if value.truthy() {
        value.python_str()
    } else {
        "Sin modelo".into()
    }
}

fn text_label(key: &str, value: &SettingValue) -> String {
    if key == "stt_dictionary" && !value.truthy() {
        return "Ejemplo: open code → OpenCode; cloud → Claude".into();
    }
    let text = text_or_empty(Some(value));
    if text.is_empty() {
        "Sin definir".into()
    } else {
        text
    }
}
