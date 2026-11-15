//! Menú real a través de `DropUI` (P-FIX-MENU): la fábrica viva abre la
//! tarjeta, la pinta y el clic en «Guardar» llega a `DropHost::save_settings`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::layout::{FOOTER, PAD_BOTTOM};
use crate::model::menu::rects::{SAVE_BUTTON_X0, SAVE_BUTTON_X1};
use crate::model::settings::{MenuSettings, SettingValue};
use crate::paint::settings_window::SIDEBAR_W;
use crate::tokens::H_ISLAND;
use crate::win::drop::ui::{DropUI, DropUiOptions, PointerPhase};
use crate::win::menu_panel::LiveMenuFactory;

use super::support::{FakeWindows, RecordingConfig, RecordingHost};

/// Ajustes con un cambio sin guardar: el botón «Guardar» queda habilitado.
fn dirty_settings() -> MenuSettings {
    let mut settings = MenuSettings::default();
    settings
        .set("speed", &SettingValue::Float(1.2))
        .expect("clave editable");
    settings
}

/// Barra con ventanas falsas, la fábrica real de menú y ajustes sucios.
///
/// `set_motion("reduced")` abre el menú sin animación: la tarjeta se puede
/// pintar y pulsar en el mismo instante de la prueba.
fn bar_with_live_menu() -> (Arc<DropUI>, Arc<RecordingHost>) {
    let windows = Arc::new(FakeWindows::default());
    let config = Arc::new(RecordingConfig::default());
    let host = Arc::new(RecordingHost::default());
    let ui = DropUI::new(DropUiOptions {
        config: Some(config),
        host: Some(Arc::clone(&host) as Arc<dyn crate::win::drop::DropHost>),
        factory: Some(windows),
        menu_factory: Some(Arc::new(LiveMenuFactory)),
        scale: 1.0,
        ..DropUiOptions::default()
    });
    ui.set_menu_settings_provider(Some(Arc::new(|| Some(dirty_settings()))));
    ui.set_motion("reduced");
    ui.show("idle");
    (ui, host)
}

#[test]
fn the_live_factory_opens_and_paints_the_card() {
    let (ui, _host) = bar_with_live_menu();
    assert!(ui.menu_panel().is_none());
    ui.toggle_menu();
    assert!(ui.menu_open(), "el menú se abrió");
    assert!(ui.menu_panel().is_some(), "la tarjeta tiene menú");
    let frame = ui.render_card();
    assert!(
        frame.width > 1 && frame.height > 1,
        "la tarjeta tiene tamaño"
    );
    assert!(
        frame.data.iter().any(|value| *value != 0),
        "el cuadro de la tarjeta no está vacío"
    );
}

#[test]
fn a_click_on_save_reaches_the_host() {
    let (ui, host) = bar_with_live_menu();
    ui.toggle_menu();
    let viewport = ui
        .menu_panel()
        .expect("la tarjeta tiene menú")
        .lock()
        .viewport_height();
    // Centro del botón «Guardar» en coordenadas lógicas de la ventana.
    let center_x = (SAVE_BUTTON_X0 + SAVE_BUTTON_X1) / 2.0;
    let center_y = f64::from(H_ISLAND) + viewport - f64::from(PAD_BOTTOM) - f64::from(FOOTER) / 2.0;
    let scale = ui.scale();
    let x = ((center_x + SIDEBAR_W) * scale).round_ties_even() as i32;
    let y = ((center_y - f64::from(H_ISLAND)) * scale).round_ties_even() as i32;
    assert!(ui.card_pointer(PointerPhase::Down, x, y), "cae en el menú");
    assert!(ui.card_pointer(PointerPhase::Up, x, y), "suelta en el menú");
    assert_eq!(host.saves.load(Ordering::SeqCst), 1, "llegó a DropHost");
}
