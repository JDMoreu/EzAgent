//! Port of `src/ezagent/voice/hud/settings.py` (F5-11).
//!
//! Modelo de la ventana de ajustes: valores editables, copia persistida y las
//! opciones que el menú necesita para pintarse. Sin dibujo, audio ni Win32.

mod dictionary;
mod format;
mod model;
mod normalize;
mod options;
mod tables;
#[cfg(test)]
mod tests;
mod value;

pub use model::MenuSettings;
pub use normalize::{
    normalize_devices, normalize_names, normalize_providers, normalize_setting, normalize_voices,
    provider_entries, providers_from_catalog, Device, OptionInput, OptionSource, ProviderEntry,
    ProviderProfile,
};
pub use options::{
    db_to_rms, FIXED_OPTIONS, LLM_REASONING_OPTIONS, METER_DB_CEIL, METER_DB_FLOOR, METER_DB_STEP,
    SLIDERS, STT_ENGINE_OPTIONS, TTS_VARIANTS, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP,
};
pub use tables::{
    voice_label, ALL_KEYS, COLOR_KEYS, DEFAULT_VALUES, EDITABLE_KEYS, INSTANT_KEYS, LABELS,
    SECRET_KEYS, SELECT_KEYS, SLIDER_KEYS, TEXT_KEYS, TOGGLE_KEYS, TRACKED_KEYS, UI_KEYS,
    VOICE_IDS, VOICE_LABELS,
};
pub use value::{
    mask_secret, setting_value, SelectOption, SettingValue, SettingsError, SettingsValues,
    SliderSpec,
};

use options::{preset, preset_label, PRESETS};
