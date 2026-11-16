//! Pruebas de la barra: forma, tamaño, posición guardada y prueba de impacto (separadas por tamaño).

use super::*;

#[test]
fn switching_shape_and_bar_size_recheck_the_dock() {
    let harness = Harness::new();
    harness.ui.show("idle");
    harness.ui.set_layout("circle");
    let work = monitor::work_area();
    let bottom = (
        f64::from(work.0 + work.2) / 2.0,
        f64::from(work.3) - crate::model::drop::CIRCLE_D / 2.0,
    );
    let request = harness.ui.origin_for(bottom, harness.ui.dock());
    harness.ui.drag_target(request.0, request.1);
    assert_eq!(harness.ui.dock(), "top");
    harness.ui.set_layout("pill");
    assert_eq!(harness.ui.dock(), "sides");
    assert_eq!(harness.windows.bar().size(), harness.ui.size());
    drop(harness);

    let harness = Harness::new();
    harness.ui.show("idle");
    harness.ui.set_layout("circle");
    let (form_x, form_y) = crate::model::drop::form_center("circle", Some("bottom"));
    let x = (f64::from(work.0 + work.2) / 2.0 - form_x).round_ties_even() as i32;
    let y = (f64::from(work.3) - 40.0 - form_y).round_ties_even() as i32;
    harness.windows.bar().set_pos(x, y);
    harness.ui.set_bar_scale(2.0);
    assert_ne!(harness.ui.dock(), "bottom");
    assert!(
        crate::model::drop::window_overflow(
            "circle",
            harness.ui.dock(),
            harness.ui.form_anchor().expect("forma visible"),
            work,
            harness.ui.bs(),
            0.0,
        ) <= 24.0
    );
}

#[test]
fn the_saved_dock_only_counts_for_the_layout_it_was_saved_with() {
    let config = |layout: &str, saved: &str, dock: &str| {
        RecordingConfig::with(&[
            ("ui_layout", ConfigValue::Text(layout.to_string())),
            ("drop_layout", ConfigValue::Text(saved.to_string())),
            ("drop_dock", ConfigValue::Text(dock.to_string())),
        ])
    };
    let same = create_ui(
        Arc::new(RecordingHost::default()),
        Some(config("circle", "circle", "top")),
        None,
    );
    assert_eq!(same.dock(), "top");
    let other = create_ui(
        Arc::new(RecordingHost::default()),
        Some(config("circle", "pill", "top")),
        None,
    );
    assert_eq!(other.dock(), "bottom");
    let junk = create_ui(
        Arc::new(RecordingHost::default()),
        Some(config("pill", "pill", "top")),
        None,
    );
    assert_eq!(junk.dock(), "sides");
}

#[test]
fn create_ui_restores_the_position_only_for_the_same_layout() {
    let host = Arc::new(RecordingHost::default());
    let config = RecordingConfig::with(&[
        ("ui_layout", ConfigValue::Text(String::from("pill"))),
        ("drop_layout", ConfigValue::Text(String::from("pill"))),
        ("drop_x", ConfigValue::Int(100)),
        ("drop_y", ConfigValue::Int(200)),
    ]);
    let ui = create_ui(host, Some(config), None);
    assert_eq!(ui.bar.lock().position, Some((100, 200)));

    let host = Arc::new(RecordingHost::default());
    let config = RecordingConfig::with(&[
        ("ui_layout", ConfigValue::Text(String::from("pill"))),
        ("drop_layout", ConfigValue::Text(String::from("circle"))),
        ("drop_x", ConfigValue::Int(100)),
        ("drop_y", ConfigValue::Int(200)),
    ]);
    let ui = create_ui(host, Some(config), None);
    assert_eq!(ui.bar.lock().position, None);
}

#[test]
fn open_menu_waits_for_a_pending_confirmation() {
    let harness = Harness::new();
    harness.ui.show_confirmation("", false, None, None);
    harness.ui.open_menu();
    assert!(!harness.ui.menu_open());
    harness.ui.hide_confirmation();
    harness.open_card();
    assert!(harness.ui.menu_open());
    harness.ui.open_menu();
    assert!(harness.ui.menu_open());
}

#[test]
fn live_settings_flow_through_the_menu_callbacks() {
    let harness = Harness::new();
    harness.open_card();
    harness.ui.refresh_menu_settings();
    assert!(harness.menus.menu().lock().state.lock().settings_received);
    harness.ui.set_testing(true);
    assert!(harness.menus.menu().lock().state.lock().testing);
}

#[test]
fn bar_hit_testing_follows_scale() {
    let harness = Harness::new();
    harness.ui.set_bar_scale(2.0);
    let (fx, fy) = crate::model::drop::form_center(harness.ui.layout(), Some(harness.ui.dock()));
    assert_eq!(harness.ui.item_at(fx, fy), Some("pill"));
}
