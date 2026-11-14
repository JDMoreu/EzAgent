//! `MenuSettings`: valores actuales, copia guardada y opciones de selectores.

use std::collections::BTreeMap;

use ez_core::pyvalue::{casefold, char_len, py_strip};
use parking_lot::Mutex;

use super::dictionary::normalize_rules;
use super::normalize::{
    hex_or, normalize_devices, normalize_for_saved, normalize_names, normalize_providers,
    normalize_voices, quantize, saved_profile_id, Device, OptionSource, ProviderEntry,
    ProviderProfile,
};
use super::value::{text_or_empty, SelectOption, SettingValue, SettingsError, SettingsValues};
use super::{preset, ALL_KEYS, COLOR_KEYS, DEFAULT_VALUES, SLIDERS, TRACKED_KEYS};

/// Valores actuales, copia guardada y opciones de los selectores.
pub struct MenuSettings {
    pub(super) devices: Vec<Device>,
    pub(super) voices: Vec<SelectOption>,
    pub(super) providers: Vec<(String, ProviderProfile)>,
    pub(super) stt_models: Vec<SelectOption>,
    pub(super) llm_models: Mutex<BTreeMap<String, Vec<SelectOption>>>,
    pub(super) values: SettingsValues,
    pub(super) saved: SettingsValues,
}

impl Default for MenuSettings {
    /// `MenuSettings({}, None, None)`.
    fn default() -> Self {
        Self::new(&SettingsValues::new(), &[], None, &[], None, None)
    }
}

impl MenuSettings {
    /// `MenuSettings(...)`: normaliza los valores recibidos y las listas del motor.
    pub fn new(
        values: &SettingsValues,
        devices: &[Device],
        voices: Option<&OptionSource>,
        providers: &[(String, ProviderEntry)],
        stt_models: Option<&OptionSource>,
        llm_models: Option<&[(String, OptionSource)]>,
    ) -> Self {
        let current_voice =
            text_or_empty(values.get("voice").or_else(|| DEFAULT_VALUES.get("voice")));
        let stt_current = text_or_empty(values.get("stt_model"));
        let mut models: BTreeMap<String, Vec<SelectOption>> = BTreeMap::new();
        if let Some(entries) = llm_models {
            for (provider, options) in entries {
                models.insert(provider.clone(), normalize_names(Some(options), ""));
            }
        }
        let mut settings = Self {
            devices: normalize_devices(devices),
            voices: normalize_voices(voices, &current_voice),
            providers: normalize_providers(providers),
            stt_models: normalize_names(stt_models, &stt_current),
            llm_models: Mutex::new(models),
            values: SettingsValues::new(),
            saved: SettingsValues::new(),
        };
        for key in ALL_KEYS {
            let default = DEFAULT_VALUES
                .get(key)
                .cloned()
                .unwrap_or(SettingValue::None);
            let raw = values.get(key).cloned().unwrap_or(default);
            let normalized = settings
                .normalize_value(key, &raw)
                .expect("ALL_KEYS solo contiene claves válidas");
            settings.values.insert(key, normalized);
        }
        // El selector de perfiles arranca en el proveedor activo.
        let current_provider = text_or_empty(settings.values.get("llm_provider"));
        if !current_provider.is_empty() {
            settings
                .values
                .insert("llm_profile", SettingValue::Text(current_provider));
        }
        let saved_id = saved_profile_id(&text_or_empty(settings.values.get("llm_profile_name")));
        if settings.provider(&saved_id).is_some() {
            settings
                .values
                .insert("llm_saved_profile", SettingValue::Text(saved_id));
        }
        settings.saved = TRACKED_KEYS
            .iter()
            .filter_map(|key| {
                settings
                    .values
                    .get(key)
                    .map(|value| ((*key).to_string(), value.clone()))
            })
            .collect();
        settings
    }

    /// `from_app`: crea el modelo desde la configuración real y las listas del motor.
    pub fn from_app(
        values: &SettingsValues,
        devices: &[Device],
        voices: Option<&OptionSource>,
        providers: &[(String, ProviderEntry)],
        stt_models: Option<&OptionSource>,
        llm_models: Option<&[(String, OptionSource)]>,
    ) -> Self {
        Self::new(values, devices, voices, providers, stt_models, llm_models)
    }

    /// `from_app` con un `VoiceConfig`: las claves ausentes caen a su predeterminado.
    // DECISION: sustituye a la rama `getattr` de `from_app`; es la fuente real
    // que usa la app.
    pub fn from_config(
        config: &ez_core::config::VoiceConfig,
        devices: &[Device],
        voices: Option<&OptionSource>,
        providers: &[(String, ProviderEntry)],
        stt_models: Option<&OptionSource>,
        llm_models: Option<&[(String, OptionSource)]>,
    ) -> Self {
        let mut values = SettingsValues::new();
        for key in ALL_KEYS {
            let value = match config.get(key) {
                Some(raw) => {
                    if raw.is_null() {
                        SettingValue::None
                    } else if let Some(text) = raw.as_str() {
                        SettingValue::Text(text.to_string())
                    } else if let Some(flag) = raw.as_bool() {
                        SettingValue::Bool(flag)
                    } else if let Some(int) = raw.as_i64() {
                        SettingValue::Int(int)
                    } else if let Some(float) = raw.as_f64() {
                        SettingValue::Float(float)
                    } else {
                        SettingValue::Text(ez_core::config::python_str(raw))
                    }
                }
                None => DEFAULT_VALUES
                    .get(key)
                    .cloned()
                    .unwrap_or(SettingValue::None),
            };
            values.insert(key, value);
        }
        Self::new(&values, devices, voices, providers, stt_models, llm_models)
    }

    // ---- lectura ----------------------------------------------------------

    /// Copia de la lista normalizada de micrófonos.
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn voices(&self) -> &[SelectOption] {
        &self.voices
    }

    /// Perfiles disponibles, ya normalizados.
    pub fn providers(&self) -> &[(String, ProviderProfile)] {
        &self.providers
    }

    pub fn provider(&self, name: &str) -> Option<&ProviderProfile> {
        self.providers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, profile)| profile)
    }

    pub fn value(&self, key: &str) -> Result<SettingValue, SettingsError> {
        self.require(key)?;
        Ok(self.values.get(key).cloned().unwrap_or(SettingValue::None))
    }

    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.values.get(key)
    }

    pub fn get_or(&self, key: &str, default: SettingValue) -> SettingValue {
        self.values.get(key).cloned().unwrap_or(default)
    }

    pub(super) fn require(&self, key: &str) -> Result<(), SettingsError> {
        if ALL_KEYS.contains(&key) {
            Ok(())
        } else {
            Err(SettingsError::UnknownKey(key.to_string()))
        }
    }

    // ---- edición ----------------------------------------------------------

    /// Cambia un valor, lo valida y lo normaliza. Devuelve el nuevo valor.
    pub fn set(&mut self, key: &str, value: &SettingValue) -> Result<SettingValue, SettingsError> {
        self.require(key)?;
        let mut input = value.clone();
        if key == "stt_dictionary" {
            let text = text_or_empty(Some(&input));
            input = SettingValue::Text(normalize_rules(&text).map_err(SettingsError::Invalid)?);
        }
        if key == "llm_profile_name" && char_len(py_strip(&text_or_empty(Some(&input)))) > 80 {
            return Err(SettingsError::ProfileNameTooLong);
        }
        if COLOR_KEYS.contains(&key) && hex_or(&input, "").is_empty() {
            // Un color mal escrito no borra el anterior.
            return Ok(self.values.get(key).cloned().unwrap_or(SettingValue::None));
        }
        let new = self.normalize_value(key, &input)?;
        self.values.insert(key, new.clone());
        self.apply_change(key, &new);
        Ok(new)
    }

    fn apply_change(&mut self, key: &str, new: &SettingValue) {
        if key == "llm_profile" || key == "llm_saved_profile" {
            self.apply_profile(&new.python_str());
        } else if key == "llm_profile_name" {
            let name = new.python_str();
            let saved = if name.is_empty() {
                String::new()
            } else {
                format!("saved:{}", casefold(&name))
            };
            self.values
                .insert("llm_saved_profile", SettingValue::Text(saved));
        } else if key == "llm_model_pick" && new.truthy() {
            self.values.insert("llm_model", new.clone());
        } else if key == "theme_preset" {
            if let Some((background, primary, secondary, accent)) = preset(&new.python_str()) {
                for (name, color) in [
                    ("theme_background", background),
                    ("theme_primary", primary),
                    ("theme_secondary", secondary),
                    ("theme_accent", accent),
                ] {
                    self.values
                        .insert(name, SettingValue::Text(color.to_string()));
                }
                self.values
                    .insert("theme", SettingValue::Text("custom".into()));
            }
        } else if COLOR_KEYS.contains(&key) {
            self.values
                .insert("theme", SettingValue::Text("custom".into()));
        }
    }

    /// `(principal, secundario, acento, fondo)` del tema personalizado.
    pub fn custom_colors(&self) -> [String; 4] {
        COLOR_KEYS.map(|key| {
            self.values
                .get(key)
                .map(SettingValue::python_str)
                .unwrap_or_default()
        })
    }

    /// Sustituye la lista de modelos de un proveedor (hilo de red).
    pub fn set_llm_models(&self, provider: &str, models: Option<&OptionSource>) {
        let options = normalize_names(models, "");
        self.llm_models.lock().insert(provider.to_string(), options);
    }

    pub(super) fn model_options(&self) -> Vec<SelectOption> {
        let provider = text_or_empty(self.values.get("llm_provider"));
        let mut options = self
            .llm_models
            .lock()
            .get(&provider)
            .cloned()
            .unwrap_or_default();
        let current = text_or_empty(self.values.get("llm_model"));
        if !current.is_empty()
            && !options
                .iter()
                .any(|option| option.value == SettingValue::Text(current.clone()))
        {
            options.insert(
                0,
                SelectOption::new(SettingValue::Text(current.clone()), current),
            );
        }
        options
    }

    /// Copia un perfil guardado en los campos editables de LLM.
    fn apply_profile(&mut self, name: &str) {
        let Some(profile) = self.provider(name).cloned() else {
            return;
        };
        let provider = if profile.provider.is_empty() {
            name.to_string()
        } else {
            profile.provider.clone()
        };
        let saved = profile.is_saved();
        let values = [
            ("llm_provider", SettingValue::Text(provider.clone())),
            (
                "llm_api_style",
                SettingValue::Text(profile.api_style.clone()),
            ),
            ("llm_model", SettingValue::Text(profile.model.clone())),
            ("llm_base_url", SettingValue::Text(profile.base_url.clone())),
            ("api_key", SettingValue::Text(profile.api_key.clone())),
            (
                "llm_profile_name",
                SettingValue::Text(if saved {
                    profile.label.clone()
                } else {
                    String::new()
                }),
            ),
        ];
        for (key, value) in values {
            let normalized = self
                .normalize_value(key, &value)
                .expect("campos de perfil conocidos");
            self.values.insert(key, normalized);
        }
        self.values
            .insert("llm_profile", SettingValue::Text(provider));
        self.values.insert(
            "llm_saved_profile",
            SettingValue::Text(if saved {
                name.to_string()
            } else {
                String::new()
            }),
        );
    }

    // ---- guardado ---------------------------------------------------------

    /// True si algún valor editable difiere de la copia guardada.
    pub fn dirty(&self) -> bool {
        for key in TRACKED_KEYS {
            let current = self.values.get(key).cloned().unwrap_or(SettingValue::None);
            let saved = self.saved.get(key).cloned().unwrap_or(SettingValue::None);
            if let Some(spec) = SLIDERS.get(key) {
                if (quantize(&current, spec) - quantize(&saved, spec)).abs() > 1e-9 {
                    return true;
                }
            } else if current != saved {
                return true;
            }
        }
        false
    }

    /// Marca como guardados los valores actuales (o una instantánea dada).
    pub fn mark_saved(&mut self, values: Option<&SettingsValues>) {
        match values {
            None => self.saved = self.values.clone(),
            Some(snapshot) => {
                let mut saved = SettingsValues::new();
                for key in TRACKED_KEYS {
                    let raw = snapshot
                        .get(key)
                        .cloned()
                        .or_else(|| self.saved.get(key).cloned())
                        .unwrap_or(SettingValue::None);
                    saved.insert(key, normalize_for_saved(key, &raw));
                }
                self.saved = saved;
            }
        }
    }

    /// Restaura la copia guardada sin tocar los valores actuales.
    pub fn restore_saved(&mut self, values: &SettingsValues) {
        self.mark_saved(Some(values));
    }

    /// Copia de la instantánea persistida (para depuración y rollback).
    pub fn saved_snapshot(&self) -> SettingsValues {
        self.saved.clone()
    }

    /// Diccionario de valores actuales listo para `on_save`/`on_test`.
    pub fn snapshot(&self) -> SettingsValues {
        TRACKED_KEYS
            .iter()
            .filter_map(|key| {
                self.values
                    .get(key)
                    .map(|value| ((*key).to_string(), value.clone()))
            })
            .collect()
    }
}
