//! Pruebas del adaptador vivo del menú (`win/menu_panel.rs`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ez_gfx::sdf::Layer;

use crate::layout::Group;
use crate::model::menu::geometry::{Menu, MenuCallbacks as ModelCallbacks, MenuOptions};
use crate::model::settings::{MenuSettings, SettingValue};
use crate::paint::menu::advanced::Advanced;
use crate::paint::settings_window;
use crate::win::drop::{MenuCallbacks, MenuFactory, MenuPanel};

use super::{LiveMenuFactory, LiveMenuPanel};

/// Escala y pantalla de las pruebas (las de la tarjeta real).
const SCALE: f64 = 1.5;
const SCREEN_H: f64 = 1080.0;
const NOW: f64 = 10.0;
/// Ancho del contenido de la tarjeta (`content_size(760, 620)`).
const CONTENT_W: f64 = 576.0;

fn callbacks(saves: &Arc<AtomicUsize>) -> MenuCallbacks {
    let saves = Arc::clone(saves);
    MenuCallbacks {
        live_change: Arc::new(|_, _| {}),
        save: Arc::new(move |_| {
            saves.fetch_add(1, Ordering::SeqCst);
            Some(true)
        }),
        test: Arc::new(|_| {}),
        hide: Arc::new(|| {}),
        close: Arc::new(|| {}),
        text_edit: Arc::new(|_| {}),
        action: Arc::new(|_| {}),
        copy: Arc::new(|_| true),
        hotkey_capture: Arc::new(|_| {}),
    }
}

fn no_saves() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

/// Ajustes con un cambio sin guardar (`settings.dirty()`).
fn dirty_settings() -> MenuSettings {
    let mut settings = MenuSettings::default();
    settings
        .set("speed", &SettingValue::Float(1.2))
        .expect("clave editable");
    settings
}

fn groups() -> Vec<Group> {
    settings_window::section_groups("appearance")
}

fn title() -> String {
    settings_window::section_title("appearance")
}

fn model_groups(groups: &[Group]) -> Vec<(String, Vec<(String, String)>)> {
    groups
        .iter()
        .map(|(name, rows)| {
            let rows = rows
                .iter()
                .map(|(key, kind)| ((*key).to_string(), (*kind).to_string()))
                .collect();
            ((*name).to_string(), rows)
        })
        .collect()
}

/// Capa del tamaño que usa `DropUI::compose_card`.
fn card_layer(width: f64, viewport: f64) -> Layer {
    let px = |value: f64| value.round_ties_even().max(1.0) as u32;
    Layer::new(px(width * SCALE), px((viewport + 4.0) * SCALE))
}

#[test]
fn the_adapter_draws_exactly_like_the_model() {
    // La pintura lee la paleta global, que otras pruebas pueden retiñir en
    // paralelo: si su versión cambia entre los dos cuadros, se repite.
    let mut matched = false;
    for _ in 0..8 {
        let version = crate::tokens::palette_version();
        matched = compare_adapter_with_model();
        if matched || crate::tokens::palette_version() == version {
            break;
        }
    }
    assert!(matched, "el adaptador pinta igual que el modelo");
}

/// Dibuja el mismo menú por el adaptador y por el modelo y compara las capas.
fn compare_adapter_with_model() -> bool {
    let panel = LiveMenuFactory
        .create(
            dirty_settings(),
            SCALE,
            "dark",
            SCREEN_H,
            callbacks(&no_saves()),
        )
        .expect("el menú se crea");
    let groups = groups();
    let title = title();
    let (through_adapter, directly) = {
        let mut menu = panel.lock();
        menu.set_palette_overrides(crate::theme::card_overrides);
        menu.set_width(CONTENT_W);
        menu.set_max_height(600.0);
        menu.set_available_height(1000.0);
        menu.set_groups(&groups, &title, NOW);
        menu.open(NOW);
        let mut through_adapter = card_layer(menu.width(), menu.viewport_height());
        menu.draw(&mut through_adapter, NOW + 1.0);

        let mut model = Menu::new(
            dirty_settings(),
            MenuOptions {
                scale: SCALE,
                theme: String::from("dark"),
                screen_h: SCREEN_H,
            },
            ModelCallbacks::default(),
        )
        .expect("el menú se crea");
        model.set_palette_overrides(Some(Box::new(crate::theme::card_overrides)));
        model.set_width(Some(CONTENT_W));
        model.set_max_height(Some(600.0));
        model.set_available_height(Some(1000.0));
        model.set_groups(model_groups(&groups), Some(&title), Some(NOW));
        model.open(NOW);
        let mut directly = card_layer(model.width(), model.viewport_height());
        model.draw_with(&mut directly, NOW + 1.0, &mut Advanced);
        (through_adapter, directly)
    };

    let same = through_adapter.as_raw() == directly.as_raw();
    assert!(
        directly.as_raw().iter().any(|value| *value > 0.0),
        "la tarjeta dibujada no quedó vacía"
    );
    same
}

#[test]
fn opening_and_clicking_save_reach_the_callbacks() {
    let saves = no_saves();
    let mut panel =
        LiveMenuPanel::new(dirty_settings(), SCALE, "dark", SCREEN_H, callbacks(&saves))
            .expect("el menú se crea");
    let groups = groups();
    let title = title();
    panel.set_width(CONTENT_W);
    panel.set_max_height(600.0);
    panel.set_groups(&groups, &title, NOW);
    assert!(!panel.hit(100.0, 100.0, NOW), "cerrado no hay controles");
    panel.open(NOW);
    assert_eq!(panel.height(NOW), 0.0);
    assert!(panel.height(NOW + 1.0) > 0.0);
    let (x, y) = {
        let model = panel.model();
        let layout = model.layout();
        model.save_rect(&layout).center()
    };
    assert!(panel.hit(x, y, NOW + 1.0), "el botón Guardar responde");
    panel.pointer_down(x, y, NOW + 1.0);
    panel.pointer_up(x, y, NOW + 1.0);
    assert_eq!(saves.load(Ordering::SeqCst), 1, "llegó el guardado");
}

#[test]
fn live_scale_and_models_reach_the_settings() {
    let mut panel = LiveMenuPanel::new(
        MenuSettings::default(),
        SCALE,
        "dark",
        SCREEN_H,
        callbacks(&no_saves()),
    )
    .expect("el menú se crea");
    panel.set_live_scale(1.35);
    panel.set_llm_models("opencode", &[String::from("gpt-4.1")]);
    let mut model = panel.model();
    assert_eq!(
        model.settings.get("ui_scale"),
        Some(&SettingValue::Float(1.35))
    );
    model
        .settings
        .set("llm_provider", &SettingValue::Text("opencode".into()))
        .expect("proveedor válido");
    let options = model
        .settings
        .select_options("llm_model_pick")
        .unwrap_or_default();
    assert!(options
        .iter()
        .any(|option| option.value == SettingValue::Text("gpt-4.1".into())));
}
