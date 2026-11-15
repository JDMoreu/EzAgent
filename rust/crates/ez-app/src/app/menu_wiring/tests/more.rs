use super::*;

#[test]
fn apply_settings_rejects_an_unknown_kokoro_voice() {
    let fixture = Fixture::new();
    let effects = Recorder::with_voices(&["em_alex", "ef_dora"]);
    let values = fixture.values(&[("voice", SettingValue::Text("nope".to_string()))]);
    let error = fixture
        .app
        .apply_settings_with(&values, &effects)
        .expect_err("voz inválida");
    assert_eq!(error, "Ninguna voz de Kokoro válida en 'nope'.");
}

#[test]
fn apply_settings_changes_the_input_device() {
    let fixture = Fixture::new();
    let values = fixture.values(&[("input_device_index", SettingValue::Int(0))]);
    let restarted = fixture.app.apply_settings(&values).expect("guardado");
    assert!(!restarted);
    let mic_index = fixture.app.inner.audio.mic.lock().device_index();
    let saved = fixture.app.config();
    let stored = saved.get("input_device_index").and_then(Value::as_i64);
    assert_eq!(stored, mic_index.map(|index| index as i64));
    // El nombre viaja con el índice: es lo que identifica el mismo micrófono
    // en la app de Python, que solo entiende el índice de PortAudio.
    let mic_name = fixture.app.inner.audio.mic.lock().device_name().to_string();
    let written = fixture.config_json();
    assert_eq!(
        written.get("input_device_name").and_then(Value::as_str),
        Some(mic_name.as_str())
    );
    assert!(fixture.config_file.exists());
}

#[test]
fn apply_settings_without_device_change_updates_the_mic() {
    let fixture = Fixture::new();
    let values = fixture.values(&[("min_rms", SettingValue::Float(0.002))]);
    fixture.app.apply_settings(&values).expect("guardado");
    let mic = fixture.app.inner.audio.mic.lock();
    assert!((mic.min_rms() - 0.002).abs() < 1e-9);
    assert!((mic.silence_secs() - 1.3).abs() < 1e-9);
}

#[test]
fn test_voice_restores_the_previous_voice_and_emits_the_event() {
    let fixture = Fixture::new();
    let effects = Recorder::default();
    let values = fixture.values(&[
        ("voice", SettingValue::Text("ef_dora".to_string())),
        ("speed", SettingValue::Float(1.2)),
    ]);
    fixture
        .app
        .test_voice_from_menu_with(&values, &effects)
        .expect("prueba de voz");
    assert_eq!(effects.calls(), vec!["mic:true:", "state:speaking"]);
    fixture.app.drain_workers(5.0);
    assert_eq!(
        fixture.log.voices.lock().as_slice(),
        ["ef_dora".to_string(), "em_alex".to_string()]
    );
    assert_eq!(
        fixture.log.speaks.lock().as_slice(),
        ["Hola, soy tu asistente de voz."]
    );
    let cfg = fixture.app.config();
    assert_eq!(cfg.get("voice").and_then(Value::as_str), Some("em_alex"));
    assert_eq!(cfg.get("speed").and_then(Value::as_f64), Some(0.95));
    assert_eq!(
        cfg.get("barge_in").and_then(Value::as_bool),
        Some(false),
        "el manejador de test_done de F6-07 lo restaura"
    );
    let events: Vec<AppEvent> = std::iter::from_fn(|| fixture.app.events().get_nowait()).collect();
    assert_eq!(events, vec![AppEvent::TestDone(false, 1.0)]);
}

#[test]
fn llm_label_uses_the_provider_spec_or_the_raw_name() {
    let fixture = Fixture::new();
    assert_eq!(fixture.app.llm_label(), "OpenCode Go");
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("llm_provider", Value::String("mi-proveedor".to_string()));
    assert_eq!(fixture.app.llm_label(), "mi-proveedor");
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("llm_provider", Value::String(String::new()));
    assert_eq!(fixture.app.llm_label(), "OpenCode Go");
}

#[test]
fn fetch_models_skips_empty_url_and_repeated_attempts() {
    let fixture = Fixture::new();
    fixture
        .app
        .fetch_llm_models("opencode", "", "", "openai", false);
    assert!(fixture.app.inner.config.llm_models.lock().tried.is_empty());
    assert!(fixture.app.inner.runtime.workers.lock().is_empty());
    fixture.app.inner.config.llm_models.lock().tried.insert((
        "opencode".to_string(),
        "http://127.0.0.1:9/v1".to_string(),
        "openai".to_string(),
    ));
    let workers = fixture.app.inner.runtime.workers.lock().len();
    fixture
        .app
        .fetch_llm_models("opencode", "http://127.0.0.1:9/v1", "", "openai", false);
    assert_eq!(fixture.app.inner.runtime.workers.lock().len(), workers);
    assert_eq!(fixture.app.inner.config.llm_models.lock().tried.len(), 1);
}

#[test]
fn restart_whisper_without_a_process_only_logs() {
    let fixture = Fixture::new();
    fixture.app.restart_whisper_async();
    assert!(!fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
    assert!(fixture.app.inner.runtime.workers.lock().is_empty());
}

#[test]
fn menu_hide_event_hides_the_bar_and_saves_the_flag() {
    let fixture = Fixture::new();
    fixture
        .app
        .handle_menu_event(&MenuEvent::Hide)
        .expect("ocultar");
    assert!(fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert_eq!(
        fixture
            .config_json()
            .get("hud_hidden")
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn menu_live_event_reaches_the_config() {
    let fixture = Fixture::new();
    fixture
        .app
        .handle_menu_event(&MenuEvent::Live(
            "speed".to_string(),
            ConfigValue::Float(1.4),
        ))
        .expect("en vivo");
    assert_eq!(
        fixture.app.config().get("speed").and_then(Value::as_f64),
        Some(1.4)
    );
}

#[test]
fn conversion_helpers_match_python_texts() {
    assert_eq!(
        setting_float(&SettingValue::None).unwrap_err(),
        "float() argument must be a string or a real number, not 'NoneType'"
    );
    assert_eq!(
        setting_float(&SettingValue::Text("x".to_string())).unwrap_err(),
        "could not convert string to float: 'x'"
    );
    assert_eq!(
        setting_int(&SettingValue::Text("x".to_string())).unwrap_err(),
        "invalid literal for int() with base 10: 'x'"
    );
    assert_eq!(
        setting_rounded_int(&SettingValue::Float(2.5)).expect("par"),
        2
    );
    assert_eq!(
        setting_rounded_int(&SettingValue::Float(3.5)).expect("par"),
        4
    );
}

#[test]
fn menu_save_event_applies_and_persists() {
    let fixture = Fixture::new();
    let values = fixture.values(&[("speed", SettingValue::Float(1.45))]);
    fixture
        .app
        .handle_menu_event(&MenuEvent::Save(values))
        .expect("guardar");
    assert_eq!(
        fixture.app.config().get("speed").and_then(Value::as_f64),
        Some(1.45)
    );
    assert_eq!(
        fixture.config_json().get("speed").and_then(Value::as_f64),
        Some(1.45)
    );
}

#[test]
fn changed_tts_settings_reload_kokoro_in_a_worker() {
    let fixture = Fixture::new();
    let model = temp_path("kokoro.onnx");
    let voices = temp_path("voices.bin");
    std::fs::write(&model, b"modelo").expect("modelo temporal");
    std::fs::write(&voices, b"voces").expect("voces temporales");
    let values = fixture.values(&[
        ("tts_model_variant", SettingValue::Text("fp32".to_string())),
        (
            "kokoro_model_path",
            SettingValue::Text(model.display().to_string()),
        ),
        (
            "kokoro_voices_path",
            SettingValue::Text(voices.display().to_string()),
        ),
    ]);
    fixture.app.apply_settings(&values).expect("guardado");
    fixture.app.drain_workers(5.0);
    assert_eq!(*fixture.log.reloads.lock(), 1);
    let cfg = fixture.app.config();
    assert_eq!(
        cfg.get("tts_model_variant").and_then(Value::as_str),
        Some("fp32")
    );
    let _ = std::fs::remove_file(model);
    let _ = std::fs::remove_file(voices);
}

#[test]
fn fix_model_name_ignores_a_model_that_already_matches() {
    let fixture = Fixture::new();
    let before = fixture.app.config();
    let models = vec!["deepseek-v4.1-flash".to_string()];
    fixture.app.fix_model_name("opencode", &models);
    assert_eq!(
        fixture.app.config().get("llm_model"),
        before.get("llm_model")
    );
    assert!(!fixture.config_file.exists());
}
