//! `_hud_menu_settings`: instantánea del menú con sus listas de opciones.

use crate::app::core::{cfg_text, VoiceApp, INSTANT_SETTINGS, MENU_VOICES};
use ez_core::config::VoiceConfig;
use ez_core::llm_providers::{load_provider_catalog, normalize_provider, provider_models};
use ez_hud::model::settings::{
    Device, MenuSettings, OptionInput, OptionSource, ProviderEntry, SettingValue, SettingsValues,
    EDITABLE_KEYS,
};

use super::{json_to_setting, map_to_settings, settings_to_map};

impl VoiceApp {
    /// `_hud_menu_settings`: instantánea completa para el menú.
    pub fn hud_menu_settings(&self) -> Option<MenuSettings> {
        // DECISION: el Python devuelve None si falla el import de `settings`;
        // en Rust el modelo siempre está disponible, así que siempre hay menú.
        let cfg = self.config();
        let devices = self.menu_devices();
        let values = self.menu_values(&cfg);
        let providers = self.menu_providers(&cfg);
        let stt_models = OptionSource::Pairs(
            ez_stt::client::available_whisper_models(&cfg_text(&cfg, "stt_model", ""))
                .into_iter()
                .collect(),
        );
        let llm_models = self.menu_llm_models(&providers);
        let voices = OptionSource::Pairs(
            MENU_VOICES
                .iter()
                .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
                .collect(),
        );
        self.fetch_llm_models(
            &normalize_provider(Some(&cfg_text(&cfg, "llm_provider", ""))),
            &cfg_text(&cfg, "llm_base_url", ""),
            &cfg_text(&cfg, "api_key", ""),
            &cfg_text(&cfg, "llm_api_style", ""),
            false,
        );
        let mut settings = MenuSettings::from_app(
            &values,
            &devices,
            Some(&voices),
            &providers,
            Some(&stt_models),
            Some(&llm_models),
        );
        let mut saved = self.inner.config.menu_saved.lock();
        match saved.as_ref() {
            None => *saved = Some(settings_to_map(&settings.saved_snapshot())),
            Some(snapshot) => settings.restore_saved(&map_to_settings(snapshot)),
        }
        Some(settings)
    }

    fn menu_devices(&self) -> Vec<Device> {
        let devices = self.inner.config.devices_cache.lock().clone();
        if devices.is_empty() {
            // La enumeración de PyAudio no corre en el hilo de render.
            let app = self.clone();
            self.start_worker("ezagent-devices", move || app.refresh_devices());
        }
        devices
            .into_iter()
            .map(|device| Device {
                index: device.index as i64,
                name: device.name,
                rate: i64::from(device.rate),
                channels: i64::from(device.channels),
            })
            .collect()
    }

    fn menu_values(&self, cfg: &VoiceConfig) -> SettingsValues {
        let mut values = SettingsValues::new();
        for key in EDITABLE_KEYS.iter().chain(INSTANT_SETTINGS.iter()) {
            let value = cfg
                .get(key)
                .map(json_to_setting)
                .unwrap_or(SettingValue::None);
            values.insert(*key, value);
        }
        let device = self.inner.audio.mic.lock().device_index;
        values.insert(
            "input_device_index",
            device.map_or(SettingValue::None, |index| SettingValue::Int(index as i64)),
        );
        values.insert(
            "llm_profile",
            SettingValue::Text(cfg_text(cfg, "llm_provider", "")),
        );
        values
    }

    fn menu_providers(&self, cfg: &VoiceConfig) -> Vec<(String, ProviderEntry)> {
        let mut entries: Vec<(String, ProviderEntry)> = match load_provider_catalog(None, None) {
            Ok(catalog) => catalog
                .into_iter()
                .map(|(name, entry)| (name, ProviderEntry::from_catalog(&entry)))
                .collect(),
            Err(error) => {
                crate::app::core::log(format!("No pude leer llm.json: {error}"));
                Vec::new()
            }
        };
        let current = normalize_provider(Some(&cfg_text(cfg, "llm_provider", "")));
        if !entries.iter().any(|(name, _)| *name == current) {
            entries.push((current.clone(), ProviderEntry::default()));
        }
        if let Some((_, entry)) = entries.iter_mut().find(|(name, _)| *name == current) {
            if entry.label.as_deref().is_none_or(str::is_empty) {
                entry.label = Some(py_title(&current));
            }
            entry.base_url = Some(nonempty_or(
                &cfg_text(cfg, "llm_base_url", ""),
                &entry.base_url,
            ));
            entry.model = Some(nonempty_or(&cfg_text(cfg, "llm_model", ""), &entry.model));
            let api_key = cfg_text(cfg, "api_key", "");
            if !api_key.is_empty() {
                entry.api_key = Some(api_key);
            }
            let style = cfg_text(cfg, "llm_api_style", "");
            if !style.is_empty() {
                entry.api_style = Some(style);
            } else if entry.api_style.is_none() {
                entry.api_style = Some("openai".to_string());
            }
        }
        entries
    }

    fn menu_llm_models(
        &self,
        providers: &[(String, ProviderEntry)],
    ) -> Vec<(String, OptionSource)> {
        let cache = self.inner.config.llm_models.lock().cache.clone();
        providers
            .iter()
            .map(|(name, _)| {
                let cached = cache.get(name).filter(|models| !models.is_empty());
                let source = match cached {
                    Some(models) => OptionSource::Items(
                        models
                            .iter()
                            .cloned()
                            .map(OptionInput::Text)
                            .collect::<Vec<_>>(),
                    ),
                    None => OptionSource::Pairs(
                        provider_models(Some(name))
                            .iter()
                            .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
                            .collect(),
                    ),
                };
                (name.clone(), source)
            })
            .collect()
    }
}

/// `str.title()` de Python para ASCII y letras con caja.
pub(super) fn py_title(text: &str) -> String {
    let mut out = String::new();
    let mut previous_cased = false;
    for character in text.chars() {
        let cased = character.is_uppercase() || character.is_lowercase();
        if cased && previous_cased {
            out.extend(character.to_lowercase());
        } else if cased {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
        previous_cased = cased;
    }
    out
}

fn nonempty_or(value: &str, fallback: &Option<String>) -> String {
    if value.is_empty() {
        fallback.clone().unwrap_or_default()
    } else {
        value.to_string()
    }
}
