//! `INSTANT_SETTINGS` de la app debe ser la misma tabla que usa el HUD (TMAP-1).

#[test]
fn instant_settings_match_the_hud_table() {
    assert_eq!(
        ez_app::app::core::INSTANT_SETTINGS,
        ez_hud::model::settings::INSTANT_KEYS
    );
    for key in ["ui_scale", "show_advanced"] {
        assert!(
            ez_hud::model::settings::INSTANT_KEYS.contains(&key),
            "falta {key}"
        );
    }
}
