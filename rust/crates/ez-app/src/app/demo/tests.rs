//! Pruebas de los pasos de la demo (F6-07).

use ez_hud::model::settings::SettingValue;

use super::{animated_state, demo_settings, DEMO_STEPS};

#[test]
fn demo_visits_every_state_in_order() {
    assert_eq!(DEMO_STEPS.len(), 14);
    assert_eq!(DEMO_STEPS[0], "open");
    assert_eq!(DEMO_STEPS[1], "idle");
    assert_eq!(DEMO_STEPS[2], "listen");
    assert_eq!(DEMO_STEPS[4], "speak");
    assert_eq!(DEMO_STEPS.last(), Some(&"close"));
}

#[test]
fn the_meter_only_animates_for_listen_and_speak() {
    assert_eq!(animated_state(0), "idle");
    assert_eq!(animated_state(3), "listen");
    assert_eq!(animated_state(5), "speak");
    assert_eq!(animated_state(DEMO_STEPS.len()), "idle");
}

#[test]
fn demo_settings_use_the_sample_values_and_voices() {
    let settings = demo_settings();
    assert_eq!(
        settings.get("voice"),
        Some(&SettingValue::Text("em_alex".to_string()))
    );
    assert_eq!(settings.get("speed"), Some(&SettingValue::Float(0.95)));
    assert_eq!(settings.get("barge_in"), Some(&SettingValue::Bool(true)));
    assert_eq!(settings.voices().len(), 3);
}
