//! Pruebas del modelo de ajustes (sin dibujo ni Win32).

use super::normalize::quantize;
use super::*;

mod more;

fn settings() -> MenuSettings {
    MenuSettings::from_app(&SettingsValues::new(), &[], None, &[], None, None)
}

#[test]
fn key_tables_cover_the_python_vocabulary() {
    assert_eq!(EDITABLE_KEYS.len(), 43);
    assert_eq!(UI_KEYS.len(), 3);
    assert_eq!(INSTANT_KEYS.len(), 14);
    assert_eq!(ALL_KEYS.len(), 60);
    assert_eq!(TRACKED_KEYS.len(), 57);
    assert_eq!(SLIDER_KEYS.len(), 16);
    assert_eq!(SELECT_KEYS.len(), 16);
    assert_eq!(TOGGLE_KEYS.len(), 7);
    assert_eq!(TEXT_KEYS.len(), 21);
    assert_eq!(COLOR_KEYS.len(), 4);
    for key in ALL_KEYS {
        assert!(LABELS.contains_key(key), "falta la etiqueta de {key}");
    }
    for (key, _) in VOICE_LABELS {
        assert!(VOICE_IDS.contains(&key));
    }
    assert_eq!(SLIDERS.len(), SLIDER_KEYS.len());
    assert_eq!(FIXED_OPTIONS.len(), 6);
}

#[test]
fn defaults_are_normalized_on_construction() {
    let settings = settings();
    assert_eq!(settings.get("ui_scale"), Some(&SettingValue::Float(1.0)));
    assert_eq!(
        settings.get("llm_max_tokens"),
        Some(&SettingValue::Float(320.0))
    );
    assert_eq!(
        settings.get("input_device_index"),
        Some(&SettingValue::None)
    );
    assert_eq!(
        settings.get("llm_profile"),
        Some(&SettingValue::Text("opencode".into()))
    );
    assert_eq!(
        settings.get("theme_primary"),
        Some(&SettingValue::Text("#1FAE9F".into()))
    );
    assert!(!settings.dirty());
    assert_eq!(settings.snapshot().len(), 57);
    assert_eq!(settings.saved_snapshot().len(), 57);
}

#[test]
fn formats_match_the_specification() {
    let settings = settings();
    let expected = [
        ("ui_scale", "100 %"),
        ("min_rms", "0,000596"),
        ("noise_multiplier", "1,3×"),
        ("silence_secs", "1,3 s"),
        ("turn_threshold", "0,50"),
        ("max_utterance_secs", "30 s"),
        ("barge_in_rms", "0,008"),
        ("speed", "0,95×"),
        ("tts_sentence_pause", "0,30 s"),
        ("stt_timeout_secs", "60 s"),
        ("llm_max_tokens", "320"),
        ("llm_context_tokens", "32000 tok"),
        ("input_device_index", "Sin micrófonos"),
        ("voice", "Álex · masculina"),
        ("tts_model_variant", "int8 · rápida"),
        ("stt_engine", "Automático"),
        ("llm_reasoning_effort", "Sin razonamiento"),
        ("theme", "Automático"),
        ("ui_layout", "Píldora"),
        ("llm_api_style", "OpenAI"),
        ("theme_base", "Oscura"),
        ("close_action", "Pausa"),
        ("ui_motion", "Según el sistema"),
        ("stt_model", "ggml-large-v3-turbo-q8_0"),
        ("llm_model", "deepseek-v4.1-flash"),
        ("llm_model_pick", "1 disponible"),
        ("theme_preset", "Elegir paleta"),
        ("llm_profile", "opencode"),
        ("llm_saved_profile", "Sin configuración guardada"),
        ("llm_provider", "opencode"),
        ("api_key", "Sin clave"),
        ("smart_turn", "Sí"),
        ("confirm_actions", "No"),
        (
            "stt_dictionary",
            "Ejemplo: open code → OpenCode; cloud → Claude",
        ),
        ("whisper_url", "http://127.0.0.1:8088/inference"),
        ("wake_phrase", "EzAgent, escúchame"),
        ("llm_profile_name", "Sin definir"),
        ("stt_prompt", "Sin definir"),
        ("hotkey", "ctrl+alt+space"),
    ];
    for (key, text) in expected {
        assert_eq!(settings.format(key).expect(key), text, "clave {key}");
    }
    assert_eq!(
        settings
            .format_value("ui_scale", &SettingValue::Float(1.33))
            .expect("ui_scale"),
        "135 %"
    );
}

#[test]
fn quantize_clamps_and_rounds_to_the_slider_step() {
    let scale = SLIDERS.get("ui_scale").expect("ui_scale");
    assert_eq!(quantize(&SettingValue::Float(1.33), scale), 1.35);
    assert_eq!(quantize(&SettingValue::Int(9), scale), 2.0);
    assert_eq!(quantize(&SettingValue::Float(0.1), scale), 0.6);
    assert_eq!(quantize(&SettingValue::Float(f64::NAN), scale), 0.6);
    let rms = SLIDERS.get("min_rms").expect("min_rms");
    assert_eq!(quantize(&SettingValue::Float(0.001), rms), 0.001);
    assert_eq!(quantize(&SettingValue::Float(0.00001), rms), 0.0001);
    assert_eq!(quantize(&SettingValue::Float(0.2), rms), 0.1);
    assert_eq!(db_to_rms(METER_DB_FLOOR), 0.0001);
    assert_eq!(db_to_rms(METER_DB_CEIL), 0.1);
}

#[test]
fn normalize_setting_matches_python_fallbacks() {
    assert_eq!(
        normalize_setting("ui_scale", &SettingValue::Text("1.33".into())).expect("ui_scale"),
        SettingValue::Float(1.35)
    );
    assert_eq!(
        normalize_setting("ui_scale", &SettingValue::Text(" 1_0 ".into())).expect("ui_scale"),
        SettingValue::Float(2.0)
    );
    assert_eq!(
        normalize_setting("ui_scale", &SettingValue::Text("nan".into())).expect("ui_scale"),
        SettingValue::Float(0.6)
    );
    assert_eq!(
        normalize_setting("input_device_index", &SettingValue::Text("2.9".into())).expect("device"),
        SettingValue::None
    );
    assert_eq!(
        normalize_setting("input_device_index", &SettingValue::Float(2.9)).expect("device"),
        SettingValue::Int(2)
    );
    assert_eq!(
        normalize_setting("input_device_index", &SettingValue::Int(-1)).expect("device"),
        SettingValue::None
    );
    assert_eq!(
        normalize_setting("theme", &SettingValue::Text(" LIGHT ".into())).expect("theme"),
        SettingValue::Text("auto".into())
    );
    assert_eq!(
        normalize_setting("theme_primary", &SettingValue::Text("  #abc ".into())).expect("color"),
        SettingValue::Text("#AABBCC".into())
    );
    assert_eq!(
        normalize_setting("wake_phrase", &SettingValue::Text("  hola  ".into()))
            .expect("wake_phrase"),
        SettingValue::Text("hola".into())
    );
    assert_eq!(
        normalize_setting(
            "stt_dictionary",
            &SettingValue::Text("open code => OpenCode".into())
        )
        .expect("stt_dictionary"),
        SettingValue::Text("open code => OpenCode".into())
    );
    let error = normalize_setting("nope", &SettingValue::Int(1)).expect_err("clave ajena");
    assert_eq!(error.to_string(), "ajuste desconocido: 'nope'");
}

#[test]
fn set_validates_dictionary_profile_name_and_colors() {
    let mut settings = settings();
    settings
        .set(
            "stt_dictionary",
            &SettingValue::Text("open code => OpenCode\ncloud => Claude".into()),
        )
        .expect("diccionario");
    assert_eq!(
        settings.get("stt_dictionary"),
        Some(&SettingValue::Text(
            "open code → OpenCode; cloud → Claude".into()
        ))
    );
    let error = settings
        .set(
            "stt_dictionary",
            &SettingValue::Text("regla sin flecha".into()),
        )
        .expect_err("diccionario inválido");
    assert_eq!(
        error.to_string(),
        "Usa origen → corrección; separa las reglas con punto y coma."
    );
    assert_eq!(
        settings.get("stt_dictionary"),
        Some(&SettingValue::Text(
            "open code → OpenCode; cloud → Claude".into()
        ))
    );
    let long = "x".repeat(81);
    let error = settings
        .set("llm_profile_name", &SettingValue::Text(long))
        .expect_err("nombre largo");
    assert_eq!(
        error.to_string(),
        "El nombre de la configuración admite hasta 80 caracteres."
    );
    settings
        .set("theme_primary", &SettingValue::Text("#abc".into()))
        .expect("color");
    assert_eq!(
        settings.get("theme_primary"),
        Some(&SettingValue::Text("#AABBCC".into()))
    );
    assert_eq!(
        settings.get("theme"),
        Some(&SettingValue::Text("custom".into()))
    );
    settings
        .set("theme_primary", &SettingValue::Text("malo".into()))
        .expect("color inválido no borra");
    assert_eq!(
        settings.get("theme_primary"),
        Some(&SettingValue::Text("#AABBCC".into()))
    );
}

#[test]
fn theme_presets_copy_the_four_colors() {
    let mut settings = settings();
    settings
        .set("theme_preset", &SettingValue::Text("marea".into()))
        .expect("preset");
    assert_eq!(
        settings.get("theme"),
        Some(&SettingValue::Text("custom".into()))
    );
    assert_eq!(
        settings.get("theme_background"),
        Some(&SettingValue::Text("#0E6F7C".into()))
    );
    assert_eq!(
        settings.get("theme_primary"),
        Some(&SettingValue::Text("#1FAE9F".into()))
    );
    assert_eq!(
        settings.get("theme_secondary"),
        Some(&SettingValue::Text("#8FE3C0".into()))
    );
    assert_eq!(
        settings.get("theme_accent"),
        Some(&SettingValue::Text("#FF8A65".into()))
    );
    assert_eq!(
        settings.custom_colors(),
        ["#1FAE9F", "#8FE3C0", "#FF8A65", "#0E6F7C"]
    );
    assert_eq!(
        settings
            .select_options("theme_preset")
            .expect("paletas")
            .iter()
            .map(|option| option.label.clone())
            .collect::<Vec<_>>(),
        ["Marea", "Atardecer", "Bosque", "Lavanda", "Medianoche"]
    );
}
