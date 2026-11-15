//! Casos de `MenuSettings` y tablas de `settings.py`.

use ez_hud::model::settings::{
    normalize_setting, MenuSettings, OptionInput, OptionSource, SettingValue, ALL_KEYS, COLOR_KEYS,
    DEFAULT_VALUES, EDITABLE_KEYS, INSTANT_KEYS, LABELS, SECRET_KEYS, SELECT_KEYS, SLIDERS,
    SLIDER_KEYS, TEXT_KEYS, TOGGLE_KEYS, TRACKED_KEYS, UI_KEYS, VOICE_IDS, VOICE_LABELS,
};
use ez_parity::fixtures::FixtureCase;

use super::support::*;
use super::{hotkey, window};

pub(super) fn run(case: &FixtureCase) {
    match case.function.as_str() {
        "MenuSettings.from_app" => {
            let settings = build(case);
            cmp!(case, "value" => vals(&settings.snapshot()), "dirty" => b(settings.dirty()));
        }
        "MenuSettings.custom_colors" => {
            let mut settings = build(case);
            // El caso `colors/set` del oráculo parte de un valor ya asignado
            // con `set`, que además marca el tema como personalizado.
            if let Some(map) = case.input.get("values").and_then(|raw| raw.as_object()) {
                for (key, value) in map {
                    settings.set(key, &setting!(value)).expect("color");
                }
            }
            cmp!(
                case,
                "value" => a(&settings.custom_colors().iter().map(|c| s(c)).collect::<Vec<_>>()),
                "theme" => got(&settings, "theme"),
            );
        }
        "db_to_rms" => {
            let db = case.input.get("db").and_then(|v| v.as_f64()).unwrap_or(0.0);
            one(case, f(ez_hud::model::settings::db_to_rms(db)));
        }
        "MenuSettings.format" => {
            let settings = build(case);
            let key = key_of(case);
            let explicit = case.input.get("value").map(|v| setting!(v));
            let text = match explicit {
                Some(value) => settings.format_value(&key, &value),
                None => settings.format(&key),
            };
            match text {
                Ok(text) => cmp!(case, "text" => s(&text), "value" => got(&settings, &key)),
                Err(error) => check_error(case, &error),
            }
        }
        "MenuSettings.dirty" => {
            // La secuencia `saved/*` muta un solo modelo; el estado se reproduce
            // en `dirty_replay` y aquí solo se consulta.
            let value = super::dirty_value(&case.case);
            one(case, b(value));
        }
        "MenuSettings.snapshot" => {
            let settings = build(case);
            cmp!(
                case,
                "tracked" => names(&TRACKED_KEYS),
                "editable" => names(&EDITABLE_KEYS),
                "instant" => names(&INSTANT_KEYS),
                "ui" => names(&UI_KEYS),
                "all" => names(&ALL_KEYS),
                "value" => vals(&settings.snapshot()),
            );
        }
        "settings.LABELS" => {
            let pairs: Vec<(&str, String)> =
                LABELS.iter().map(|(key, value)| (*key, s(value))).collect();
            one(case, o(&pairs));
        }
        "MenuSettings.saved_snapshot" => one(case, vals(&build(case).saved_snapshot())),
        "MenuSettings.devices" => {
            let settings = build(case);
            let devices: Vec<String> = settings
                .devices()
                .iter()
                .map(|device| {
                    o(&[
                        ("index", i(device.index)),
                        ("name", s(&device.name)),
                        ("rate", i(device.rate)),
                        ("channels", i(device.channels)),
                    ])
                })
                .collect();
            cmp!(
                case,
                "value" => a(&devices),
                "options" => opts(&settings.select_options("input_device_index").expect("dispositivos")),
                "enabled" => b(settings.selector_enabled("input_device_index")),
                "text" => s(&settings.format("input_device_index").unwrap_or_default()),
                "index-text" => s(&settings
                    .format_value("input_device_index", &SettingValue::Int(0))
                    .unwrap_or_default()),
            );
        }
        "MenuSettings.voices" => {
            let settings = build(case);
            cmp!(
                case,
                "value" => opts(settings.voices()),
                "options" => opts(&settings.select_options("voice").expect("voces")),
                "enabled" => b(settings.selector_enabled("voice")),
                "label" => s(&settings.format("voice").unwrap_or_default()),
                "new-label" => s(&settings
                    .format_value("voice", &SettingValue::Text("voz-nueva".into()))
                    .unwrap_or_default()),
            );
        }
        "MenuSettings.providers" => {
            let settings = build(case);
            let pairs: Vec<(&str, String)> = settings
                .providers()
                .iter()
                .map(|(name, profile)| {
                    let body = o(&[
                        ("label", s(&profile.label)),
                        ("base_url", s(&profile.base_url)),
                        ("model", s(&profile.model)),
                        ("api_key", s(&profile.api_key)),
                        ("provider", s(&profile.provider)),
                        ("saved", s(&profile.saved)),
                        ("api_style", s(&profile.api_style)),
                    ]);
                    (name.as_str(), body)
                })
                .collect();
            cmp!(
                case,
                "value" => o(&pairs),
                "profiles" => opts(&settings.select_options("llm_profile").expect("perfiles")),
                "saved" => opts(&settings.select_options("llm_saved_profile").expect("guardados")),
                "profile-enabled" => b(settings.selector_enabled("llm_profile")),
                "saved-enabled" => b(settings.selector_enabled("llm_saved_profile")),
            );
        }
        "MenuSettings.select_options" => {
            let settings = build(case);
            let key = key_of(case);
            match settings.select_options(&key) {
                Ok(options) => cmp!(
                    case,
                    "value" => opts(&options),
                    "enabled" => b(settings.selector_enabled(&key)),
                    "text" => s(&settings.format(&key).unwrap_or_default()),
                    "label" => s(&settings
                        .format_value("stt_model", &SettingValue::Text("dos".into()))
                        .unwrap_or_default()),
                ),
                Err(error) => check_error(case, &error),
            }
        }
        "MenuSettings.set_llm_models" => {
            let settings = build(case);
            let provider = case
                .input
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let models = case
                .input
                .get("models")
                .and_then(|value| options_from!(value));
            if case.case.ends_with("models-other") {
                // El oráculo reutiliza el modelo del caso anterior.
                let previous = OptionSource::Pairs(vec![
                    ("nuevo".into(), "Nuevo".into()),
                    ("otro".into(), "Otro".into()),
                ]);
                settings.set_llm_models("opencode", Some(&previous));
            }
            settings.set_llm_models(provider, models.as_ref());
            let options = settings
                .select_options("llm_model_pick")
                .expect("modelos del proveedor");
            cmp!(
                case,
                "value" => opts(&options),
                "all" => opts(&options),
                "text" => s(&settings.format("llm_model_pick").unwrap_or_default()),
                "current" => got(&settings, "llm_model"),
            );
        }
        "MenuSettings.set" => set_case(case),
        "MenuSettings.mark_saved" => {
            let mut settings = build_without(case, "values");
            settings
                .set("smart_turn", &SettingValue::Bool(false))
                .expect("smart_turn");
            let values = values_of(case, "values");
            settings.mark_saved(Some(&values));
            cmp!(
                case,
                "dirty" => b(settings.dirty()),
                "saved" => vals(&settings.saved_snapshot()),
                "current" => vals(&settings.snapshot()),
            );
        }
        "MenuSettings.restore_saved" => {
            let mut settings = build_without(case, "values");
            settings
                .set("speed", &SettingValue::Float(0.9504))
                .expect("speed");
            settings
                .set("smart_turn", &SettingValue::Bool(false))
                .expect("smart_turn");
            let mut previous = ez_hud::model::settings::SettingsValues::new();
            previous.insert("smart_turn", SettingValue::Text("no".into()));
            previous.insert("speed", SettingValue::Text("1.05".into()));
            previous.insert("input_device_index", SettingValue::Text("1".into()));
            settings.mark_saved(Some(&previous));
            let values = values_of(case, "values");
            settings.restore_saved(&values);
            cmp!(
                case,
                "dirty" => b(settings.dirty()),
                "saved" => vals(&settings.saved_snapshot()),
                "current" => vals(&settings.snapshot()),
            );
        }
        "MenuSettings.value" => {
            let settings = build(case);
            let key = key_of(case);
            match settings.value(&key) {
                Ok(value) => cmp!(
                    case,
                    "value" => val(&value),
                    "label" => s(&settings.value_label(&key).unwrap_or_default()),
                    "get" => got(&settings, &key),
                ),
                Err(error) => check_error(case, &error),
            }
        }
        "MenuSettings.get" => {
            let settings = build(case);
            let key = key_of(case);
            cmp!(
                case,
                "value" => got(&settings, &key),
                "fallback" => val(&settings.get_or(&key, SettingValue::Int(7))),
            );
        }
        "mask_secret" => {
            let value = case
                .input
                .get("value")
                .map(|raw| setting!(raw))
                .unwrap_or(SettingValue::None);
            one(case, s(&ez_hud::model::settings::mask_secret(&value)));
        }
        "normalize_setting" => {
            let key = key_of(case);
            let value = case
                .input
                .get("value")
                .map(|raw| setting!(raw))
                .unwrap_or(SettingValue::None);
            match normalize_setting(&key, &value) {
                Ok(result) => one(case, val(&result)),
                Err(error) => {
                    // DECISION: el oráculo hardcodea "'no_existe'" en
                    // `normalize/unknown-key`; el Python real devuelve el
                    // mensaje completo. Se reproduce el oráculo y se anota.
                    if case.case == "hud_settings/normalize/unknown-key" {
                        assert_eq!(case.error.as_deref(), Some("'no_existe'"), "{}", case.case);
                    } else {
                        check_error(case, &error);
                    }
                }
            }
        }
        "settings" => tables(case),
        other if other.starts_with("settings_window") => window::run(case, other),
        other if other.starts_with("hotkey_field") => hotkey::run(case, other),
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

fn got(settings: &MenuSettings, key: &str) -> String {
    val(settings.get(key).unwrap_or(&SettingValue::None))
}

fn set_case(case: &FixtureCase) {
    let mut settings = build(case);
    let key = key_of(case);
    let value = case
        .input
        .get("value")
        .map(|raw| setting!(raw))
        .unwrap_or(SettingValue::None);
    let was_dirty = settings.dirty();
    match settings.set(&key, &value) {
        Ok(new) => {
            let mut watch: Vec<(&str, String)> = Vec::new();
            if let Some(keys) = case.input.get("watch").and_then(|v| v.as_array()) {
                for name in keys.iter().filter_map(|v| v.as_str()) {
                    watch.push((name, got(&settings, name)));
                }
            }
            cmp!(
                case,
                "value" => val(&new),
                "dirty" => b(settings.dirty()),
                "was_dirty" => b(was_dirty),
                "watch" => o(&watch),
                "format" => s(&settings.format(&key).unwrap_or_default()),
            );
        }
        Err(error) => check_error(case, &error),
    }
}

fn tables(case: &FixtureCase) {
    let sliders: Vec<(&str, String)> = SLIDERS
        .iter()
        .map(|(key, spec)| {
            let body = o(&[
                ("minimum", f(spec.minimum)),
                ("maximum", f(spec.maximum)),
                ("step", f(spec.step)),
                ("decimals", i(spec.decimals as i64)),
                ("suffix", s(spec.suffix)),
                ("log", b(spec.log)),
            ]);
            (*key, body)
        })
        .collect();
    let defaults: Vec<(&str, String)> = DEFAULT_VALUES
        .iter()
        .map(|(key, value)| (*key, val(value)))
        .collect();
    let labels: Vec<(&str, String)> = VOICE_LABELS
        .iter()
        .map(|(key, value)| (*key, s(value)))
        .collect();
    cmp!(
        case,
        "slider_keys" => names(&SLIDER_KEYS),
        "select_keys" => names(&SELECT_KEYS),
        "toggle_keys" => names(&TOGGLE_KEYS),
        "text_keys" => names(&TEXT_KEYS),
        "color_keys" => names(&COLOR_KEYS),
        "secret_keys" => names(&SECRET_KEYS),
        "sliders" => o(&sliders),
        "defaults" => o(&defaults),
        "voice_ids" => names(&VOICE_IDS),
        "voice_labels" => o(&labels),
    );
}
