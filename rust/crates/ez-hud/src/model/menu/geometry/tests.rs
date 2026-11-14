//! Pruebas del estado y la geometría del menú (F5-07).

use std::collections::BTreeSet;

use crate::layout;
use crate::model::settings::{MenuSettings, SettingValue};

use super::placement::{footer, header};
use super::*;

fn menu() -> Menu {
    Menu::new(
        MenuSettings::default(),
        MenuOptions::default(),
        MenuCallbacks::default(),
    )
    .expect("opciones válidas")
}

#[test]
fn defaults_match_the_python_constructor() {
    let menu = menu();
    assert_eq!(menu.scale, 1.0);
    assert_eq!(menu.theme, "dark");
    assert_eq!(menu.screen_h, 1080.0);
    assert_eq!(menu.viewport_height(), 560.0);
    assert_eq!(menu.width(), 320.0);
    assert_eq!(menu.title, "AJUSTES");
    assert!(!menu.is_open());
    assert!(menu.closed_notified);
    assert_eq!(menu.blocks.len(), layout::GROUPS.len() + 2);
    assert_eq!(menu.row_hover.len(), ALL_KEYS.len() + 2 + ACTION_KEYS.len());
}

#[test]
fn invalid_scale_keeps_the_python_message() {
    let options = MenuOptions {
        scale: 0.0,
        ..MenuOptions::default()
    };
    let result = Menu::new(MenuSettings::default(), options, MenuCallbacks::default());
    match result {
        Err(error) => assert_eq!(error, "scale debe ser mayor que cero"),
        Ok(_) => panic!("scale cero no es válido"),
    }
}

#[test]
fn theme_and_screen_are_clamped() {
    let options = MenuOptions {
        theme: "neon".into(),
        screen_h: 400.0,
        ..MenuOptions::default()
    };
    let menu = Menu::new(MenuSettings::default(), options, MenuCallbacks::default())
        .expect("opciones válidas");
    assert_eq!(menu.theme, "dark");
    assert_eq!(menu.viewport_height(), 244.0);
}

#[test]
fn geometry_follows_width_height_and_available_slots() {
    let mut menu = menu();
    menu.set_width(Some(420.0));
    assert_eq!(menu.dx, 100.0);
    assert_eq!(menu.width(), 420.0);
    menu.set_width(None);
    assert_eq!(menu.dx, 0.0);
    menu.set_max_height(Some(100.0));
    assert_eq!(menu.viewport_height(), 160.0);
    menu.set_available_height(Some(120.0));
    assert_eq!(menu.viewport_height(), 120.0);
    menu.set_available_height(None);
    assert_eq!(menu.viewport_height(), 160.0);
}

#[test]
fn advanced_and_custom_rows_hide_like_the_python() {
    let mut menu = menu();
    let visible_keys = |menu: &Menu| -> BTreeSet<String> {
        menu.shown_groups()
            .into_iter()
            .flat_map(|(_name, rows)| rows.into_iter().map(|(key, _kind)| key))
            .collect()
    };
    let keys = visible_keys(&menu);
    assert!(!keys.contains("theme_primary"));
    assert!(!keys.contains("turn_threshold"));

    let mut settings = MenuSettings::default();
    settings
        .set("theme", &SettingValue::Text("custom".into()))
        .expect("tema válido");
    settings
        .set("show_advanced", &SettingValue::Bool(true))
        .expect("interruptor válido");
    menu.set_settings(settings, Some(0.0));
    let keys = visible_keys(&menu);
    assert!(keys.contains("theme_primary"));
    assert!(keys.contains("turn_threshold"));
}

#[test]
fn layout_anchors_and_scroll_max_match_the_table() {
    let menu = menu();
    let layout = menu.layout();
    assert_eq!(header(&layout).y0, 46.0);
    assert_eq!(header(&layout).y1, 78.0);
    assert_eq!(menu.content_top(&layout), 88.0);
    assert_eq!(footer(&layout).y1, 586.0);
    assert!(menu.scroll_max() > 0.0);
    assert_eq!(menu.scroll_offset(), 0.0);
}

#[test]
fn set_groups_falls_back_and_resets_the_selector() {
    let mut menu = menu();
    menu.selector_key = Some("voice".into());
    menu.selector_closing = true;
    menu.list_scroll = 20.0;
    menu.set_groups(
        vec![("only".into(), vec![("ui_scale".into(), "slider".into())])],
        Some("SECCIÓN"),
        Some(1.0),
    );
    assert_eq!(menu.groups(), vec!["only"]);
    assert_eq!(menu.title, "SECCIÓN");
    assert!(menu.selector_key.is_none());
    assert_eq!(menu.list_scroll, 0.0);
    menu.set_groups(Vec::new(), None, None);
    assert_eq!(menu.groups().len(), layout::GROUPS.len());
    assert_eq!(menu.title, "AJUSTES");
}

#[test]
fn block_values_scale_alpha_and_offset() {
    let mut menu = menu();
    menu.content_alpha.value = 1.0;
    menu.blocks.get_mut("header").expect("bloque").value = 0.5;
    assert_eq!(menu.block_values("header", 0.0), (0.5, 3.0));
    assert_eq!(menu.block_values("footer", 0.0), (0.0, 6.0));
}

#[test]
fn indicator_fades_after_its_hold_time() {
    let mut menu = menu();
    menu.indicator_activity(1.0);
    assert!((menu.indicator_alpha(1.06) - 0.5).abs() < 1e-9);
    assert_eq!(menu.indicator_alpha(0.9), 0.0);
    assert!((menu.indicator_alpha(1.5) - 0.75).abs() < 1e-9);
    assert_eq!(menu.indicator_alpha(2.0), 0.0);
}
