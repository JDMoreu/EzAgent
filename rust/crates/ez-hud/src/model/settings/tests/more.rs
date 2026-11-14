//! Pruebas de perfiles, dispositivos, listas y guardado.

use ez_core::llm_providers::CatalogEntry;

use super::super::*;
use super::settings;

fn values(pairs: &[(&str, SettingValue)]) -> SettingsValues {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn python_providers() -> Vec<(String, ProviderEntry)> {
    vec![
        (
            "custom".into(),
            ProviderEntry {
                label: Some("Personalizado".into()),
                model: Some("default".into()),
                ..ProviderEntry::default()
            },
        ),
        (
            "saved:local".into(),
            ProviderEntry {
                label: Some("Local".into()),
                provider: Some("custom".into()),
                saved: Some(SettingValue::Text("true".into())),
                model: Some("fast".into()),
                base_url: Some("http://localhost:1234/v1".into()),
                api_key: Some("secret".into()),
                ..ProviderEntry::default()
            },
        ),
    ]
}

#[test]
fn saved_configurations_restore_the_connection_fields() {
    let settings = MenuSettings::from_app(
        &SettingsValues::new(),
        &[],
        None,
        &python_providers(),
        None,
        None,
    );
    let profiles: Vec<String> = settings
        .select_options("llm_profile")
        .expect("perfiles")
        .iter()
        .map(|option| option.value.python_str())
        .collect();
    assert_eq!(profiles, ["custom"]);
    assert!(settings.selector_enabled("llm_saved_profile"));
    let mut settings = settings;
    settings
        .set(
            "llm_saved_profile",
            &SettingValue::Text("saved:local".into()),
        )
        .expect("perfil guardado");
    assert_eq!(
        settings.get("llm_provider"),
        Some(&SettingValue::Text("custom".into()))
    );
    assert_eq!(
        settings.get("llm_profile"),
        Some(&SettingValue::Text("custom".into()))
    );
    assert_eq!(
        settings.get("llm_profile_name"),
        Some(&SettingValue::Text("Local".into()))
    );
    assert_eq!(
        settings.get("llm_model"),
        Some(&SettingValue::Text("fast".into()))
    );
    assert_eq!(
        settings.get("llm_base_url"),
        Some(&SettingValue::Text("http://localhost:1234/v1".into()))
    );
    assert_eq!(
        settings.get("api_key"),
        Some(&SettingValue::Text("secret".into()))
    );
    assert_eq!(settings.format("api_key").expect("api_key"), "Configurada");
    assert!(settings.dirty());
    assert_eq!(
        settings.snapshot().get("llm_profile_name"),
        Some(&SettingValue::Text("Local".into()))
    );
}

#[test]
fn switching_to_a_provider_clears_the_saved_profile() {
    let mut settings = MenuSettings::from_app(
        &SettingsValues::new(),
        &[],
        None,
        &python_providers(),
        None,
        None,
    );
    settings
        .set("llm_profile", &SettingValue::Text("custom".into()))
        .expect("perfil");
    assert_eq!(
        settings.get("llm_profile_name"),
        Some(&SettingValue::Text(String::new()))
    );
    assert_eq!(
        settings.get("llm_saved_profile"),
        Some(&SettingValue::Text(String::new()))
    );
    assert_eq!(
        settings.get("llm_model"),
        Some(&SettingValue::Text("default".into()))
    );
}

#[test]
fn naming_a_configuration_marks_its_saved_id() {
    let mut settings = settings();
    settings
        .set(
            "llm_profile_name",
            &SettingValue::Text("  Trabajo local  ".into()),
        )
        .expect("nombre");
    assert_eq!(
        settings.get("llm_saved_profile"),
        Some(&SettingValue::Text("saved:trabajo local".into()))
    );
    assert_eq!(
        settings.get("llm_profile_name"),
        Some(&SettingValue::Text("Trabajo local".into()))
    );
    let snapshot = settings.snapshot();
    let mut reloaded = MenuSettings::from_app(&snapshot, &[], None, &[], None, None);
    reloaded.restore_saved(&snapshot);
    assert!(!reloaded.dirty());
}

fn device_settings() -> MenuSettings {
    let devices = [
        Device {
            index: 1,
            name: "Mic A".into(),
            rate: 48_000,
            channels: 1,
        },
        Device {
            index: 1,
            name: "dup".into(),
            rate: 0,
            channels: 0,
        },
        Device {
            index: 3,
            name: "Mic B".into(),
            rate: 44_100,
            channels: 2,
        },
    ];
    let voices = OptionSource::Items(vec![
        OptionInput::Pair("em_alex".into(), "Álex".into()),
        OptionInput::Pair("x".into(), String::new()),
        OptionInput::Pair("x".into(), "dup".into()),
    ]);
    let stt_models = OptionSource::Items(vec![
        OptionInput::Pair("m1".into(), "Model 1".into()),
        OptionInput::Pair("m2".into(), String::new()),
    ]);
    let llm_models = vec![(
        "opencode".to_string(),
        OptionSource::Items(vec![
            OptionInput::Pair("x".into(), String::new()),
            OptionInput::Pair("y".into(), "Y".into()),
        ]),
    )];
    MenuSettings::from_app(
        &values(&[
            ("input_device_index", SettingValue::Int(3)),
            ("voice", SettingValue::Text("nueva".into())),
        ]),
        &devices,
        Some(&voices),
        &[],
        Some(&stt_models),
        Some(&llm_models),
    )
}

#[test]
fn devices_and_voices_keep_their_order() {
    let settings = device_settings();
    assert_eq!(settings.devices().len(), 2);
    assert_eq!(settings.devices()[1].name, "Mic B");
    let labels: Vec<String> = settings
        .voices()
        .iter()
        .map(|option| option.label.clone())
        .collect();
    assert_eq!(labels, ["Álex", "x", "nueva"]);
    assert_eq!(settings.format("voice").expect("voice"), "nueva");
    assert_eq!(
        settings.format("input_device_index").expect("dev"),
        "3: Mic B"
    );
    assert_eq!(
        settings
            .format_value("input_device_index", &SettingValue::None)
            .expect("dev"),
        "Predeterminado del sistema"
    );
    assert_eq!(
        settings
            .format_value("input_device_index", &SettingValue::Int(9))
            .expect("dev"),
        "9: no disponible"
    );
    let devices_options = settings.select_options("input_device_index").expect("devs");
    assert_eq!(devices_options[0].value, SettingValue::Int(1));
    assert_eq!(devices_options[1].label, "3: Mic B");
}

#[test]
fn model_lists_keep_their_order_and_prepend_the_current_model() {
    let stt_models = OptionSource::Items(vec![
        OptionInput::Pair("m1".into(), "Model 1".into()),
        OptionInput::Pair("m2".into(), String::new()),
    ]);
    let llm_models = vec![(
        "opencode".to_string(),
        OptionSource::Items(vec![
            OptionInput::Pair("x".into(), String::new()),
            OptionInput::Pair("y".into(), "Y".into()),
        ]),
    )];
    let settings = MenuSettings::from_app(
        &SettingsValues::new(),
        &[],
        None,
        &[],
        Some(&stt_models),
        Some(&llm_models),
    );
    let stt: Vec<String> = settings
        .select_options("stt_model")
        .expect("modelos")
        .iter()
        .map(|option| option.label.clone())
        .collect();
    assert_eq!(stt, ["Model 1", "m2"]);
    settings.set_llm_models(
        "opencode",
        Some(&OptionSource::Items(vec![
            OptionInput::Pair("a".into(), "A".into()),
            OptionInput::Pair("b".into(), "B".into()),
        ])),
    );
    let pick: Vec<String> = settings
        .select_options("llm_model_pick")
        .expect("modelos")
        .iter()
        .map(|option| option.label.clone())
        .collect();
    assert_eq!(pick, ["deepseek-v4.1-flash", "A", "B"]);
    settings.set_llm_models("opencode", None);
    let pick: Vec<String> = settings
        .select_options("llm_model_pick")
        .expect("modelos")
        .iter()
        .map(|option| option.value.python_str())
        .collect();
    assert_eq!(pick, ["deepseek-v4.1-flash"]);
}

#[test]
fn empty_voice_input_falls_back_to_the_engine_list() {
    let settings = MenuSettings::from_app(
        &values(&[("voice", SettingValue::Text(String::new()))]),
        &[],
        None,
        &[],
        None,
        None,
    );
    assert_eq!(
        settings.get("voice"),
        Some(&SettingValue::Text("em_alex".into()))
    );
    assert_eq!(settings.voices().len(), 3);
}

#[test]
fn dirty_compares_sliders_after_quantizing_and_mark_saved_copies() {
    let mut settings = settings();
    settings
        .set("ui_scale", &SettingValue::Float(1.004))
        .expect("ui_scale");
    assert!(!settings.dirty());
    settings
        .set("ui_scale", &SettingValue::Float(1.05))
        .expect("ui_scale");
    assert!(settings.dirty());
    settings.mark_saved(Some(&SettingsValues::new()));
    assert!(settings.dirty());
    settings.mark_saved(None);
    assert!(!settings.dirty());
    assert_eq!(settings.saved_snapshot().len(), 60);
}

#[test]
fn mask_secret_never_shows_the_whole_key() {
    assert_eq!(mask_secret(&SettingValue::None), "Sin clave");
    assert_eq!(
        mask_secret(&SettingValue::Text("abc".into())),
        "Configurada"
    );
    assert_eq!(
        mask_secret(&SettingValue::Text("12345678".into())),
        "Configurada · ····5678"
    );
    assert_eq!(
        mask_secret(&SettingValue::Text("123456789".into())),
        "Configurada · ····6789"
    );
}

#[test]
fn selectors_report_their_availability() {
    let settings = settings();
    assert!(settings.selector_enabled("voice"));
    assert!(settings.selector_enabled("tts_model_variant"));
    assert!(settings.selector_enabled("stt_engine"));
    assert!(settings.selector_enabled("theme"));
    assert!(!settings.selector_enabled("input_device_index"));
    assert!(!settings.selector_enabled("stt_model"));
    assert!(!settings.selector_enabled("llm_profile"));
    assert!(!settings.selector_enabled("llm_saved_profile"));
    assert!(!settings.selector_enabled("nope"));
    let error = settings.select_options("nope").expect_err("clave ajena");
    assert_eq!(error.to_string(), "'nope' no es un selector");
}

#[test]
fn from_config_and_catalog_use_the_shared_crate() {
    let config = ez_core::config::VoiceConfig::new();
    let settings = MenuSettings::from_config(&config, &[], None, &[], None, None);
    assert_eq!(
        settings.get("voice"),
        Some(&SettingValue::Text("em_alex".into()))
    );
    assert_eq!(settings.get("ui_scale"), Some(&SettingValue::Float(1.0)));
    let catalog = vec![(
        "saved:local".to_string(),
        CatalogEntry {
            label: "Local".into(),
            base_url: "http://localhost:1234/v1".into(),
            model: "fast".into(),
            api_key: "secret".into(),
            api_style: "openai".into(),
            provider: Some("custom".into()),
            saved: true,
        },
    )];
    let entries = providers_from_catalog(catalog);
    assert_eq!(entries[0].1.saved, Some(SettingValue::Text("true".into())));
    let settings = MenuSettings::from_app(&SettingsValues::new(), &[], None, &entries, None, None);
    assert_eq!(
        settings
            .provider("saved:local")
            .map(|profile| profile.label.as_str()),
        Some("Local")
    );
    assert!(settings.selector_enabled("llm_saved_profile"));
}
