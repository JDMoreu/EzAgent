//! Normalización de valores, dispositivos, voces, modelos y perfiles.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ez_core::llm_providers::{load_provider_catalog, CatalogEntry};
use ez_core::pyvalue::{casefold, py_strip};

use super::value::{
    float_of, int_of, or_default, round_decimals, text_or_empty, SelectOption, SettingValue,
    SettingsError, SliderSpec,
};
use super::{
    voice_label, MenuSettings, DEFAULT_VALUES, FIXED_OPTIONS, SLIDERS, TEXT_KEYS, TOGGLE_KEYS,
};

/// Un micrófono de entrada, como los dicts de `list_input_devices`.
// DECISION: `ez-hud` no declara `ez-audio`; el llamador convierte su
// `InputDevice` a estos cuatro campos ya normalizados.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub index: i64,
    pub name: String,
    pub rate: i64,
    pub channels: i64,
}

/// Una entrada de voz o de modelo, en cualquiera de las formas que acepta Python.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionInput {
    /// Un id suelto: `"em_alex"` (voz) o `"gpt-4.1"` (modelo).
    Text(String),
    /// Un par `(id, etiqueta)`.
    Pair(String, String),
    /// Un `SelectOption` ya construido (solo modelos).
    Select(SelectOption),
    /// Un dict: `id`/`voice` para voces, `value`/`id`/`model` para modelos.
    Dict(Vec<(String, String)>),
}

/// Origen de opciones: un mapping ya resuelto o una secuencia de entradas.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionSource {
    Pairs(Vec<(String, String)>),
    Items(Vec<OptionInput>),
}

/// Perfil de proveedor tal como lo entrega el catálogo o `llm.json`.
// DECISION: `saved` es opcional para reproducir `str(raw.get("saved") or "")`;
// un booleano se guarda como "true", igual que el catálogo del Python.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProviderEntry {
    pub label: Option<String>,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub default_model: Option<String>,
    pub api_key: Option<String>,
    pub provider: Option<String>,
    pub saved: Option<SettingValue>,
    pub api_style: Option<String>,
}

impl ProviderEntry {
    /// Convierte una entrada de `load_provider_catalog`.
    pub fn from_catalog(entry: &CatalogEntry) -> Self {
        Self {
            label: Some(entry.label.clone()),
            name: None,
            base_url: Some(entry.base_url.clone()),
            model: Some(entry.model.clone()),
            default_model: None,
            api_key: Some(entry.api_key.clone()),
            provider: entry.provider.clone(),
            saved: entry.saved.then(|| SettingValue::Text("true".into())),
            api_style: Some(entry.api_style.clone()),
        }
    }
}

/// Perfil ya normalizado, con el orden de claves del dict del Python.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderProfile {
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub provider: String,
    pub saved: String,
    pub api_style: String,
}

impl ProviderProfile {
    pub fn is_saved(&self) -> bool {
        !self.saved.is_empty()
    }
}

/// `_normalize_devices`: descarta índices repetidos conservando el orden.
pub fn normalize_devices(devices: &[Device]) -> Vec<Device> {
    let mut seen: HashSet<i64> = HashSet::new();
    let mut out = Vec::new();
    for device in devices {
        if seen.insert(device.index) {
            out.push(device.clone());
        }
    }
    out
}

/// `_normalize_voices`: ids, pares, dicts o mapping id → etiqueta.
pub fn normalize_voices(voices: Option<&OptionSource>, current: &str) -> Vec<SelectOption> {
    let mut pairs = voices.map(voice_pairs).unwrap_or_default();
    if pairs.is_empty() {
        pairs = super::VOICE_LABELS
            .iter()
            .map(|(ident, label)| ((*ident).to_string(), (*label).to_string()))
            .collect();
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<SelectOption> = Vec::new();
    for (ident, label) in pairs {
        if ident.is_empty() || !seen.insert(ident.clone()) {
            continue;
        }
        let label = if label.is_empty() {
            voice_name(&ident)
        } else {
            label
        };
        out.push(SelectOption::new(SettingValue::Text(ident), label));
    }
    if !current.is_empty() && !seen.contains(current) {
        let label = voice_name(current);
        out.push(SelectOption::new(
            SettingValue::Text(current.to_string()),
            label,
        ));
    }
    out
}

fn voice_pairs(source: &OptionSource) -> Vec<(String, String)> {
    match source {
        OptionSource::Pairs(items) => items.clone(),
        OptionSource::Items(items) => items.iter().filter_map(voice_pair).collect(),
    }
}

fn voice_pair(item: &OptionInput) -> Option<(String, String)> {
    match item {
        OptionInput::Text(ident) => Some((ident.clone(), voice_name(ident))),
        OptionInput::Dict(entries) => {
            let ident = dict_text(entries, "id")
                .or_else(|| dict_text(entries, "voice"))
                .unwrap_or_default();
            if ident.is_empty() {
                return None;
            }
            let label = dict_text(entries, "label").unwrap_or_else(|| voice_name(&ident));
            Some((ident, label))
        }
        OptionInput::Pair(ident, label) => Some((ident.clone(), label.clone())),
        // `SelectOption` no es una secuencia en Python: las voces lo ignoran.
        OptionInput::Select(_) => None,
    }
}

/// `_normalize_names`: opciones de modelo como mapping, pares, strings u opciones.
pub fn normalize_names(options: Option<&OptionSource>, current: &str) -> Vec<SelectOption> {
    let mut pairs = options.map(name_pairs).unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<SelectOption> = Vec::new();
    for (ident, label) in pairs.drain(..) {
        if ident.is_empty() || !seen.insert(ident.clone()) {
            continue;
        }
        let label = if label.is_empty() {
            ident.clone()
        } else {
            label
        };
        out.push(SelectOption::new(SettingValue::Text(ident), label));
    }
    if !current.is_empty() && !seen.contains(current) {
        out.push(SelectOption::new(
            SettingValue::Text(current.to_string()),
            current,
        ));
    }
    out
}

fn name_pairs(source: &OptionSource) -> Vec<(String, String)> {
    match source {
        OptionSource::Pairs(items) => items.clone(),
        OptionSource::Items(items) => items.iter().filter_map(name_pair).collect(),
    }
}

fn name_pair(item: &OptionInput) -> Option<(String, String)> {
    match item {
        OptionInput::Select(option) => Some((option.value.python_str(), option.label.clone())),
        OptionInput::Text(ident) => Some((ident.clone(), ident.clone())),
        OptionInput::Pair(ident, label) => Some((ident.clone(), label.clone())),
        OptionInput::Dict(entries) => {
            let ident = dict_text(entries, "value")
                .or_else(|| dict_text(entries, "id"))
                .or_else(|| dict_text(entries, "model"))
                .unwrap_or_default();
            if ident.is_empty() {
                return None;
            }
            let label = dict_text(entries, "label").unwrap_or_else(|| ident.clone());
            Some((ident, label))
        }
    }
}

/// `_normalize_providers`: perfiles `nombre -> {label, base_url, model, ...}`.
pub fn normalize_providers(
    providers: &[(String, ProviderEntry)],
) -> Vec<(String, ProviderProfile)> {
    let mut out: Vec<(String, ProviderProfile)> = Vec::new();
    for (raw_name, raw) in providers {
        let name = py_strip(raw_name).to_string();
        if name.is_empty() {
            continue;
        }
        let label = nonempty(&raw.label)
            .or_else(|| nonempty(&raw.name))
            .unwrap_or_else(|| name.clone());
        let base_url = raw.base_url.clone().unwrap_or_default();
        let model = nonempty(&raw.model)
            .or_else(|| nonempty(&raw.default_model))
            .unwrap_or_default();
        let api_key = raw.api_key.clone().unwrap_or_default();
        let provider = nonempty(&raw.provider).unwrap_or_else(|| name.clone());
        let saved = text_or_empty(raw.saved.as_ref());
        let api_style = match raw.api_style.as_deref() {
            Some("openai") => "openai",
            Some("anthropic") => "anthropic",
            _ => "openai",
        };
        let profile = ProviderProfile {
            label: if label.is_empty() {
                name.clone()
            } else {
                label
            },
            base_url,
            model,
            api_key,
            provider,
            saved,
            api_style: api_style.into(),
        };
        if let Some(slot) = out.iter_mut().find(|(key, _)| *key == name) {
            slot.1 = profile;
        } else {
            out.push((name, profile));
        }
    }
    out
}

/// Convierte un catálogo de `load_provider_catalog` en entradas del modelo.
pub fn providers_from_catalog<I>(catalog: I) -> Vec<(String, ProviderEntry)>
where
    I: IntoIterator<Item = (String, CatalogEntry)>,
{
    catalog
        .into_iter()
        .map(|(name, entry)| (name, ProviderEntry::from_catalog(&entry)))
        .collect()
}

/// Catálogo ya cargado y normalizado para `MenuSettings::new`.
pub fn provider_entries(
    environ: Option<&HashMap<String, String>>,
    config_file: Option<&Path>,
) -> Result<Vec<(String, ProviderEntry)>, String> {
    load_provider_catalog(environ, config_file).map(providers_from_catalog)
}

/// `_hex_or`: `#RRGGBB` en mayúsculas; acepta `RRGGBB` y `#RGB`.
pub(crate) fn hex_or(value: &SettingValue, default: &str) -> String {
    let cleaned = text_or_empty(Some(value));
    let mut text = py_strip(&cleaned).trim_start_matches('#').to_uppercase();
    if text.chars().count() == 3 {
        text = text
            .chars()
            .flat_map(|character| [character, character])
            .collect();
    }
    if text.chars().count() == 6
        && text
            .chars()
            .all(|character| character.is_ascii_digit() || ('A'..='F').contains(&character))
    {
        return format!("#{text}");
    }
    default.to_string()
}

/// `_as_bool`.
pub(crate) fn as_bool(value: &SettingValue) -> bool {
    match value {
        SettingValue::Text(text) => {
            let lowered = py_strip(text).to_lowercase();
            !matches!(lowered.as_str(), "" | "0" | "false" | "no" | "off")
        }
        other => other.truthy(),
    }
}

/// `_quantize`: redondea al paso (anclado al mínimo) y limpia el ruido binario.
pub(crate) fn quantize(value: &SettingValue, spec: &SliderSpec) -> f64 {
    let mut number = float_of(value).unwrap_or(spec.minimum);
    if number.is_nan() {
        number = spec.minimum;
    }
    number = number.max(spec.minimum).min(spec.maximum);
    if spec.log {
        let low = 20.0 * spec.minimum.log10();
        let db = 20.0 * number.log10();
        let db = low + ((db - low) / spec.step).round_ties_even() * spec.step;
        let number = super::db_to_rms(db).max(spec.minimum).min(spec.maximum);
        return round_decimals(number, spec.decimals);
    }
    let steps = ((number - spec.minimum) / spec.step).round_ties_even();
    let quantized = spec.minimum + steps * spec.step;
    // `decimals + 2` conserva el paso y elimina colas como 0.30000000000004.
    let quantized = round_decimals(quantized, spec.decimals + 2);
    quantized.max(spec.minimum).min(spec.maximum)
}

/// `normalize_setting`: valor válido para `key` (cae al predeterminado si no lo es).
pub fn normalize_setting(key: &str, value: &SettingValue) -> Result<SettingValue, SettingsError> {
    let mut settings = MenuSettings::default();
    settings.normalize_value(key, value)
}

/// `VOICE_LABELS.get(ident, ident)`.
pub(crate) fn voice_name(ident: &str) -> String {
    voice_label(ident).unwrap_or(ident).to_string()
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn dict_text(entries: &[(String, String)], key: &str) -> Option<String> {
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

/// `_normalize_for_saved`: como `_normalize_value`, sin validar claves ni tocar
/// la lista de voces. Lo no reconocido se conserva tal cual.
pub(super) fn normalize_for_saved(key: &str, value: &SettingValue) -> SettingValue {
    if let Some(spec) = SLIDERS.get(key) {
        return SettingValue::Float(quantize(value, spec));
    }
    if TOGGLE_KEYS.contains(&key) {
        return SettingValue::Bool(as_bool(value));
    }
    match key {
        "input_device_index" => match int_of(value) {
            Some(index) if index >= 0 => SettingValue::Int(index),
            _ => SettingValue::None,
        },
        "voice" => voice_value(value),
        "tts_model_variant" => fixed_text(
            value,
            "int8",
            super::TTS_VARIANTS
                .iter()
                .map(|option| option.value.as_str().unwrap_or("")),
        ),
        "stt_engine" => fixed_text(
            value,
            "auto",
            super::STT_ENGINE_OPTIONS
                .iter()
                .map(|option| option.value.as_str().unwrap_or("")),
        ),
        "llm_reasoning_effort" => fixed_text(
            value,
            "none",
            super::LLM_REASONING_OPTIONS
                .iter()
                .map(|option| option.value.as_str().unwrap_or("")),
        ),
        "stt_model" | "llm_model" => string_value(value),
        _ if TEXT_KEYS.contains(&key) => string_value(value),
        _ => value.clone(),
    }
}

/// `_normalize_value`: el valor ya convertido a su tipo y validado.
impl MenuSettings {
    pub(super) fn normalize_value(
        &mut self,
        key: &str,
        value: &SettingValue,
    ) -> Result<SettingValue, SettingsError> {
        if let Some(spec) = SLIDERS.get(key) {
            return Ok(SettingValue::Float(quantize(value, spec)));
        }
        if TOGGLE_KEYS.contains(&key) {
            return Ok(SettingValue::Bool(as_bool(value)));
        }
        match key {
            "input_device_index" => Ok(match int_of(value) {
                Some(index) if index >= 0 => SettingValue::Int(index),
                _ => SettingValue::None,
            }),
            "voice" => {
                let text = if value.truthy() {
                    value.python_str()
                } else {
                    default_text("voice", "em_alex")
                };
                if !self
                    .voices
                    .iter()
                    .any(|option| option.value.python_str() == text)
                {
                    let label = voice_name(&text);
                    self.voices
                        .push(SelectOption::new(SettingValue::Text(text.clone()), label));
                }
                Ok(SettingValue::Text(text))
            }
            "tts_model_variant" => Ok(fixed_text(
                value,
                "int8",
                super::TTS_VARIANTS
                    .iter()
                    .map(|option| option.value.as_str().unwrap_or("")),
            )),
            "stt_engine" => Ok(fixed_text(
                value,
                "auto",
                super::STT_ENGINE_OPTIONS
                    .iter()
                    .map(|option| option.value.as_str().unwrap_or("")),
            )),
            "llm_reasoning_effort" => Ok(fixed_text(
                value,
                "none",
                super::LLM_REASONING_OPTIONS
                    .iter()
                    .map(|option| option.value.as_str().unwrap_or("")),
            )),
            _ if FIXED_OPTIONS.contains_key(key) => {
                let fallback = default_text(key, "");
                let text = text_or_empty(Some(value)).to_lowercase();
                let valid = FIXED_OPTIONS
                    .get(key)
                    .map(|options| {
                        options
                            .iter()
                            .any(|option| option.value.as_str() == Some(text.as_str()))
                    })
                    .unwrap_or(false);
                if valid {
                    Ok(SettingValue::Text(text))
                } else {
                    Ok(SettingValue::Text(fallback))
                }
            }
            "wake_phrase" => {
                let raw = text_or_empty(Some(value));
                let text = py_strip(&raw);
                if text.is_empty() {
                    Ok(SettingValue::Text(default_text("wake_phrase", "")))
                } else {
                    Ok(SettingValue::Text(text.to_string()))
                }
            }
            "llm_profile_name" => Ok(SettingValue::Text(
                py_strip(&text_or_empty(Some(value))).to_string(),
            )),
            "llm_profile" | "llm_saved_profile" | "llm_model_pick" | "theme_preset" => Ok(
                SettingValue::Text(py_strip(&text_or_empty(Some(value))).to_string()),
            ),
            key if super::COLOR_KEYS.contains(&key) => {
                Ok(SettingValue::Text(hex_or(value, &default_text(key, ""))))
            }
            "stt_model" | "llm_model" => Ok(string_value(value)),
            key if TEXT_KEYS.contains(&key) => Ok(string_value(value)),
            _ => Err(SettingsError::UnknownKey(key.to_string())),
        }
    }
}

fn default_text(key: &str, fallback: &str) -> String {
    DEFAULT_VALUES
        .get(key)
        .map(SettingValue::python_str)
        .unwrap_or_else(|| fallback.to_string())
}

fn string_value(value: &SettingValue) -> SettingValue {
    match value {
        SettingValue::None => SettingValue::Text(String::new()),
        other => SettingValue::Text(other.python_str()),
    }
}

fn voice_value(value: &SettingValue) -> SettingValue {
    let fallback = SettingValue::Text(default_text("voice", "em_alex"));
    let chosen = or_default(Some(value), &fallback);
    SettingValue::Text(chosen.python_str())
}

fn fixed_text<'a>(
    value: &SettingValue,
    fallback: &str,
    valid: impl Iterator<Item = &'a str>,
) -> SettingValue {
    let text = text_or_empty(Some(value));
    let text = if text.is_empty() {
        fallback.to_string()
    } else {
        text
    };
    if valid.into_iter().any(|option| option == text) {
        SettingValue::Text(text)
    } else {
        SettingValue::Text(fallback.to_string())
    }
}

/// Nombre comparable de una configuración guardada: `"saved:" + casefold`.
pub(crate) fn saved_profile_id(name: &str) -> String {
    format!("saved:{}", casefold(py_strip(name)))
}
