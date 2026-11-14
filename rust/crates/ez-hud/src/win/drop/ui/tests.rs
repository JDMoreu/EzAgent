//! Pruebas de `win::drop` (F5-05): coordinación de la barra y la tarjeta con
//! dobles de ventana, configuración, host, chat y menú.

mod support;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::*;
use crate::model::drop::Update;
use crate::paint::peek::PeekItem;
use crate::win::monitor;
use support::*;

fn item(key: &str, state: &str, since: f64, unseen: bool, live: bool) -> PeekItem {
    PeekItem {
        key: key.to_string(),
        title: String::from("Tarea"),
        agent: String::from("codex"),
        state: state.to_string(),
        since,
        unseen,
        live,
    }
}

fn settle_buttons(ui: &Arc<DropUI>, in_call: bool, asking: bool, hover: bool) {
    let mut bar = ui.bar.lock();
    bar.in_call = in_call;
    for index in 0..240 {
        let input = Update {
            now: f64::from(index) / 60.0,
            dt: 1.0 / 60.0,
            hover,
            reduced: true,
            in_call,
            asking,
            ..Update::default()
        };
        bar.model.update(&input);
    }
}

fn button_center(ui: &Arc<DropUI>, name: &str) -> (f64, f64) {
    let bar = ui.bar.lock();
    let positions = bar.model.positions();
    let (x, y, _, _) = positions[name];
    (x, y)
}

fn form_center(ui: &Arc<DropUI>) -> (f64, f64) {
    let bar = ui.bar.lock();
    let positions = bar.model.positions();
    let (x, y, _, _) = positions["pill"];
    (x, y)
}

#[test]
fn show_and_hide_drive_the_bar_window() {
    let harness = Harness::new();
    harness.ui.show("idle");
    let window = harness.windows.bar();
    assert!(window.started.load(Ordering::SeqCst));
    assert!(window.visible.load(Ordering::SeqCst));
    harness.ui.hide();
    assert!(!window.visible.load(Ordering::SeqCst));
    harness.ui.close();
    assert!(window.stopped.load(Ordering::SeqCst));
}

#[test]
fn the_window_size_follows_layout_and_scale() {
    let harness = Harness::new();
    harness.ui.show("idle");
    let window = harness.windows.bar();
    let (width, height) = harness.ui.size();
    assert_eq!(window.size(), (width, height));
    assert_eq!((width, height), drop_window_size("pill", "sides"));

    harness.ui.set_layout("circle");
    assert_eq!(harness.ui.layout(), "circle");
    let dock = harness.ui.dock();
    assert!(crate::model::drop::docks("circle").contains(&dock));
    let (width, height) = harness.ui.size();
    assert_eq!(window.size(), (width, height));
    assert_eq!((width, height), drop_window_size("circle", dock));
    assert_eq!(
        harness.config.saved_value("ui_layout"),
        Some(ConfigValue::Text(String::from("circle")))
    );
    assert_eq!(
        harness.config.saved_value("drop_dock"),
        Some(ConfigValue::Text(dock.to_string()))
    );

    harness.ui.set_scale(2.0);
    assert_eq!(window.size(), (width * 2, height * 2));
}

#[test]
fn a_double_click_on_the_form_switches_pill_and_circle() {
    let harness = Harness::new();
    harness.ui.show("idle");
    settle_buttons(&harness.ui, true, false, true);
    let (x, y) = form_center(&harness.ui);
    assert!(!harness.ui.pointer_down(x, y));
    assert!(harness.ui.pointer_down(x, y));
    assert_eq!(harness.ui.layout(), "circle");
}

#[test]
fn buttons_activate_the_actions_of_the_app() {
    let harness = Harness::new();
    harness.ui.show("idle");
    settle_buttons(&harness.ui, true, false, true);
    for (name, action) in [("gear", "panel"), ("chat", "chat"), ("hang", "hangup")] {
        let (x, y) = button_center(&harness.ui, name);
        assert!(harness.ui.pointer_down(x, y));
        assert!(harness.ui.pointer_up(x, y));
        assert_eq!(
            harness.host.actions.lock().last(),
            Some(&action.to_string())
        );
    }
    let (x, y) = button_center(&harness.ui, "mic");
    assert!(harness.ui.pointer_down(x, y));
    assert!(harness.ui.pointer_up(x, y));
    assert_eq!(
        harness.host.actions.lock().last(),
        Some(&"mute".to_string())
    );

    settle_buttons(&harness.ui, false, false, true);
    let (x, y) = button_center(&harness.ui, "hang");
    harness.ui.pointer_down(x, y);
    harness.ui.pointer_up(x, y);
    assert_eq!(
        harness.host.actions.lock().last(),
        Some(&"listen".to_string())
    );
}

#[test]
fn a_pending_confirmation_uses_yes_and_no() {
    let harness = Harness::new();
    harness.ui.show("idle");
    let yes = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let no = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let yes_flag = Arc::clone(&yes);
    let no_flag = Arc::clone(&no);
    harness.ui.show_confirmation(
        "¿Sigo?",
        false,
        Some(Arc::new(move || yes_flag.store(true, Ordering::SeqCst))),
        Some(Arc::new(move || no_flag.store(true, Ordering::SeqCst))),
    );
    settle_buttons(&harness.ui, true, true, true);
    let (x, y) = button_center(&harness.ui, "mic");
    harness.ui.pointer_down(x, y);
    harness.ui.pointer_up(x, y);
    assert!(yes.load(Ordering::SeqCst));
    let (x, y) = button_center(&harness.ui, "hang");
    harness.ui.pointer_down(x, y);
    harness.ui.pointer_up(x, y);
    assert!(no.load(Ordering::SeqCst));
    harness.ui.hide_confirmation();
}

#[test]
fn the_menu_opens_and_closes_the_card() {
    let harness = Harness::new();
    harness.open_card();
    assert!(harness.ui.menu_open());
    let card = harness.windows.card();
    assert!(card.started.load(Ordering::SeqCst));
    assert!(card.visible.load(Ordering::SeqCst));
    assert_eq!(card.size(), (760, 620));
    let menu = harness.menus.menu();
    assert!(menu.lock().state.lock().open);
    assert!(!menu.lock().state.lock().groups.is_empty());

    menu.lock().state.lock().close_immediately = true;
    harness.ui.close_menu();
    assert!(!harness.ui.menu_open());
    assert!(card.stopped.load(Ordering::SeqCst));
}

#[test]
fn the_card_pointer_routes_edges_sidebar_menu_and_header() {
    let harness = Harness::new();
    harness.open_card();
    let (width, height) = harness.windows.card().size();
    let menu = harness.menus.menu();

    let cursor = harness.ui.card_cursor(width - 10, height / 2);
    // Borde derecho: empieza un redimensionado y el cursor lo anuncia.
    assert_eq!(cursor, crate::paint::settings_window::IDC_SIZEWE);
    assert!(harness
        .ui
        .card_pointer(PointerPhase::Down, width - 10, height / 2));
    let cursor2 = harness.ui.card_cursor(width - 10, height / 2);
    assert_eq!(cursor2, crate::paint::settings_window::IDC_SIZEWE);
    assert!(harness
        .ui
        .card_pointer(PointerPhase::Up, width - 10, height / 2));
    assert_eq!(
        harness.config.saved_value("settings_w"),
        Some(ConfigValue::Int(760))
    );

    // Barra lateral: cambia de sección.
    assert!(harness.ui.card_pointer(PointerPhase::Down, 100, 120));
    assert_eq!(harness.ui.section.lock().as_str(), "listening");
    assert!(menu.lock().state.lock().groups.contains(&"mic".to_string()));

    // Contenido: el menú recibe el clic.
    menu.lock().state.lock().hit = true;
    assert!(harness
        .ui
        .card_pointer(PointerPhase::Down, width - 100, 200));
    assert_eq!(menu.lock().state.lock().pointer_down, 1);

    // Cabecera: dos clics seguidos devuelven el tamaño de fábrica.
    menu.lock().state.lock().hit = false;
    assert!(!harness.ui.card_pointer(PointerPhase::Down, width / 2, 20));
    assert!(harness.ui.card_pointer(PointerPhase::Down, width / 2, 20));
    assert_eq!(
        harness.config.saved_value("settings_w"),
        Some(ConfigValue::Int(760))
    );
}

#[test]
fn the_card_hands_the_wheel_and_text_to_the_menu() {
    let harness = Harness::new();
    harness.open_card();
    let (width, _) = harness.windows.card().size();
    assert!(harness.ui.card_hand(100, 80));
    harness.menus.menu().lock().state.lock().hit = true;
    assert!(harness.ui.card_hand(width - 100, 200));
    assert!(harness.ui.card_wheel(width - 100, 200, 120));
    assert!(!harness.ui.card_wheel(10, 10, 120));
    assert!(harness.ui.card_text(&CardText::Paste(String::from("hola"))));
    assert!(harness.ui.card_text(&CardText::Char('a')));
    assert!(harness.ui.card_text(&CardText::Key(0x41, false)));
    harness.ui.card_focus_lost();
    harness.ui.menu_text_edit(true);
    assert!(harness.windows.card().keyboard_enabled());
    assert!(harness.ui.copy_text("texto"));
    harness.ui.menu_action("abrir");
    assert_eq!(
        harness.host.menu_actions.lock().last(),
        Some(&"abrir".to_string())
    );
    harness.ui.hotkey_capture(true);
    assert_eq!(harness.host.hotkeys.lock().last(), Some(&true));
}

#[test]
fn the_card_frame_is_composed_at_the_window_size() {
    let harness = Harness::new();
    harness.open_card();
    harness.ui.set_save_result(true);
    let frame = harness.ui.render_card();
    assert_eq!((frame.width, frame.height), (760, 620));
    assert_eq!(frame.data.len(), 760 * 620 * 4);
    assert_eq!(frame.fps, 30.0);
    assert_eq!(harness.menus.menu().lock().state.lock().applied, vec![true]);
}

#[test]
fn a_bar_frame_renders_at_thirty_fps_and_listening_asks_sixty() {
    let harness = Harness::new();
    settle_buttons(&harness.ui, false, false, false);
    let frame = harness.ui.render_frame();
    let (width, height) = harness.ui.size();
    assert_eq!((frame.width, frame.height), (width as u32, height as u32));
    assert_eq!(frame.data.len(), width as usize * height as usize * 4);
    assert_eq!(frame.fps, 30.0);

    harness.ui.set_state("listening");
    std::thread::sleep(std::time::Duration::from_millis(200));
    harness.ui.show("listening");
    assert_eq!(harness.ui.render_frame().fps, 60.0);
}

#[test]
fn activity_becomes_the_orbit_in_priority_order() {
    let harness = Harness::new();
    harness.ui.set_activity(vec![
        item("a", "done", 1.0, true, false),
        item("b", "working", 2.0, false, true),
        item("c", "needs_you", 0.5, true, false),
        item("d", "failed", 3.0, false, true),
    ]);
    let states = super::render::orbit_states(&harness.ui.activity.read());
    assert_eq!(states, vec!["needs_you", "failed", "working", "done"]);
}

#[test]
fn chat_is_optional_but_respects_asking_and_mute() {
    let harness = Harness::new();
    harness.ui.toggle_chat();
    assert_eq!(harness.chat.toggles.load(Ordering::SeqCst), 1);
    assert!(harness.chat.is_open());
    harness.ui.set_muted(true);
    assert!(harness.chat.muted.load(Ordering::SeqCst));
    harness.ui.hide();
    assert!(harness.chat.closed.load(Ordering::SeqCst));

    harness.chat.open.store(false, Ordering::SeqCst);
    harness.ui.show_confirmation("", false, None, None);
    harness.ui.toggle_chat();
    assert_eq!(harness.chat.toggles.load(Ordering::SeqCst), 1);
    harness.ui.hide_confirmation();
}

#[test]
fn theme_custom_colors_and_chat_surface() {
    let _guard = crate::tokens::test_guard();
    let harness = Harness::new();
    harness.ui.set_theme("light");
    assert_eq!(harness.ui.theme_name(), "light");
    harness.ui.set_custom_base("light");
    harness.ui.set_theme("custom");
    assert_eq!(harness.ui.theme_name(), "light");
    harness
        .ui
        .set_custom_colors("#3366ff", "#66ccff", "#00ffaa", "#101828");
    assert!(harness.ui.custom_for_frame().is_some());
    assert_eq!(
        harness.ui.chat_body_rgb(),
        crate::theme::card_palette("light")["BODY"].channels()
    );
    harness.ui.set_theme("dark");
    assert_eq!(harness.ui.theme_name(), "dark");
    assert!(harness.ui.custom_for_frame().is_none());
}

#[test]
fn chat_size_is_remembered_with_one_decimal() {
    let harness = Harness::new();
    harness.ui.remember_chat_size(320.04, 480.06);
    assert_eq!(harness.ui.chat_size(), Some((320.04, 480.06)));
    assert_eq!(
        harness.config.saved_value("chat_width"),
        Some(ConfigValue::Float(320.0))
    );
    assert_eq!(
        harness.config.saved_value("chat_height"),
        Some(ConfigValue::Float(480.1))
    );
}

#[test]
fn clamping_keeps_the_bar_on_screen() {
    let harness = Harness::new();
    let work = monitor::work_area();
    let (width, height) = harness.ui.size();
    let low = harness.ui.clamp_position(i32::MIN, i32::MIN);
    assert_eq!(low, (work.0 - 12, work.1 - 12));
    let high = harness.ui.clamp_position(i32::MAX, i32::MAX);
    assert_eq!(high, (work.2 - width + 12, work.3 - height + 12));
}

#[test]
fn dragging_near_the_edge_moves_the_buttons_and_saves_the_dock() {
    let harness = Harness::new();
    harness.ui.show("idle");
    let work = monitor::work_area();
    let (x, y) = harness.ui.drag_target(work.2 - 5, work.1 + 100);
    assert_eq!(harness.ui.dock(), "left");
    assert!(x <= work.2 - harness.ui.size().0 + 12);
    assert!(y >= work.1 - 12);
    harness.ui.drag_end(x, y);
    assert_eq!(
        harness.config.saved_value("drop_dock"),
        Some(ConfigValue::Text(String::from("left")))
    );
    assert_eq!(harness.host.positions.lock().last(), Some(&(x, y)));
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
