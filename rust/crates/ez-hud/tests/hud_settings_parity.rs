//! Paridad de `hud/settings.py`, `settings_window.py` y `hotkey_field.py`
//! contra `parity/fixtures/hud_settings.jsonl` (P-HUD-B).

macro_rules! setting {
    ($raw:expr) => {{
        let raw = $raw;
        if raw.is_null() {
            SettingValue::None
        } else if let Some(flag) = raw.as_bool() {
            SettingValue::Bool(flag)
        } else if let Some(text) = raw.as_str() {
            SettingValue::Text(text.to_string())
        } else if let Some(int) = raw.as_i64() {
            SettingValue::Int(int)
        } else if let Some(float) = raw.as_f64() {
            SettingValue::Float(float)
        } else {
            SettingValue::Text(raw.to_string())
        }
    }};
}

macro_rules! text_entries {
    ($raw:expr) => {{
        let mut entries = Vec::new();
        if let Some(map) = $raw.as_object() {
            for (key, value) in map {
                entries.push((key.clone(), value.as_str().unwrap_or_default().to_string()));
            }
        }
        entries
    }};
}

macro_rules! options_from {
    ($value:expr) => {{
        let value = $value;
        if value.is_null() {
            None
        } else if value.is_object() {
            Some(OptionSource::Pairs(text_entries!(value)))
        } else {
            let mut items = Vec::new();
            for item in value.as_array().map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(text) = item.as_str() {
                    items.push(OptionInput::Text(text.to_string()));
                } else if item.is_object() {
                    items.push(OptionInput::Dict(text_entries!(item)));
                } else if let Some(pair) = item.as_array() {
                    let first = pair.first().and_then(|raw| raw.as_str()).unwrap_or("");
                    let second = pair.get(1).and_then(|raw| raw.as_str()).unwrap_or("");
                    items.push(OptionInput::Pair(first.to_string(), second.to_string()));
                }
            }
            Some(OptionSource::Items(items))
        }
    }};
}

macro_rules! provider_entry {
    ($raw:expr) => {{
        let raw = $raw;
        let text = |key: &str| {
            raw.get(key)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        };
        ProviderEntry {
            label: text("label"),
            name: text("name"),
            base_url: text("base_url"),
            model: text("model"),
            default_model: text("default_model"),
            api_key: text("api_key"),
            provider: text("provider"),
            saved: raw.get("saved").map(|value| setting!(value)),
            api_style: text("api_style"),
        }
    }};
}

macro_rules! cmp {
    ($case:expr, $($key:literal => $value:expr),* $(,)?) => {
        super::support::compare($case, &[$(($key, $value)),*])
    };
}

#[path = "hud_settings_parity/cases.rs"]
mod cases;
#[path = "hud_settings_parity/hotkey.rs"]
mod hotkey;
#[path = "hud_settings_parity/support.rs"]
mod support;
#[path = "hud_settings_parity/window.rs"]
mod window;

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use ez_hud::model::settings::{MenuSettings, SettingValue, SettingsValues};
use ez_parity::fixtures;

const FIXTURES: &str = "../../../parity/fixtures/hud_settings.jsonl";
const CASE_COUNT: usize = 1175;

static DIRTY: OnceLock<HashMap<String, bool>> = OnceLock::new();

fn dirty_value(name: &str) -> bool {
    *DIRTY
        .get_or_init(dirty_replay)
        .get(name)
        .expect("caso dirty conocido")
}

fn dirty_replay() -> HashMap<String, bool> {
    let mut settings = MenuSettings::new(
        &SettingsValues::new(),
        &support::default_devices(),
        None,
        &support::default_providers(),
        Some(&support::default_stt()),
        Some(&support::default_llm()),
    );
    let mut out = HashMap::new();
    let mut put = |name: &str, settings: &MenuSettings| {
        out.insert(name.to_string(), settings.dirty());
    };
    put("hud_settings/saved/clean", &settings);
    put("hud_settings/defaults/dirty", &settings);
    settings
        .set("speed", &SettingValue::Float(1.1))
        .expect("speed");
    put("hud_settings/saved/changed", &settings);
    settings
        .set("speed", &SettingValue::Float(0.95))
        .expect("speed");
    put("hud_settings/saved/back", &settings);
    settings
        .set("speed", &SettingValue::Float(0.9504))
        .expect("speed");
    put("hud_settings/saved/epsilon", &settings);
    settings.mark_saved(None);
    put("hud_settings/saved/marked", &settings);
    settings
        .set("smart_turn", &SettingValue::Bool(false))
        .expect("smart_turn");
    let mut snapshot = SettingsValues::new();
    snapshot.insert("smart_turn", SettingValue::Text("no".into()));
    snapshot.insert("speed", SettingValue::Text("1.05".into()));
    snapshot.insert("input_device_index", SettingValue::Text("1".into()));
    settings.mark_saved(Some(&snapshot));
    settings.restore_saved(&snapshot);
    settings
        .set("input_device_index", &SettingValue::None)
        .expect("device");
    put("hud_settings/saved/device-none", &settings);
    out
}

#[test]
fn every_settings_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de ajustes legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases::run(case);
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "se saltó algún caso de {FIXTURES}"
    );
    assert!(cases_in_file >= CASE_COUNT, "faltan casos en {FIXTURES}");
}
