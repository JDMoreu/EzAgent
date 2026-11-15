//! Tests of the chat panel lifecycle, input routing and callbacks.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::geometry::{FADE_SECS, MORPH_SECS, SCREEN_PAD};
use crate::model::chat::view::{Action, ActionData};
use crate::win::chat::test_support::{empty_view, fake_ui, sample_view, FakeUi, Recorder};

use super::anim::{clamp_to_work, duration, elapsed_for, step};
use super::*;

fn open_panel(view: ChatView) -> (Arc<ChatPanel>, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let ui = Arc::new(FakeUi {
        theme: "dark",
        ..FakeUi::default()
    });
    let panel = Arc::new(ChatPanel::new(
        Arc::clone(&ui) as Arc<dyn ChatUi>,
        Arc::clone(&recorder) as Arc<dyn ChatEvents>,
    ));
    {
        let mut state = panel.state.lock();
        state.view = Some(SendView(Box::new(view)));
        state.anim = Anim {
            state: AnimState::Open,
            t0: 0.0,
            p: 1.0,
        };
        state.last = 0.0;
    }
    let window = Arc::new(ChatWindow::new(&panel, 0, 0, 120, 120));
    *panel.window.lock() = Some(Arc::clone(&window));
    (panel, recorder)
}

#[test]
fn status_labels_are_the_python_texts() {
    assert_eq!(status_text("listen"), "Escuchando…");
    assert_eq!(status_text("think"), "Pensando…");
    assert_eq!(status_text("speak"), "Hablando…");
    assert_eq!(status_text("ask"), "Esperando tu respuesta");
    assert_eq!(status_text("error"), "Algo salió mal");
    assert_eq!(status_text("idle"), "");
    assert_eq!(STATUS_TEXT.len(), 5);
    assert_eq!(ACTIVE_POSES.len(), 4);
}

#[test]
fn hotkey_label_falls_back_to_the_default() {
    assert_eq!(hotkey_label(""), "Ctrl+Alt+T");
    assert_eq!(hotkey_label("ctrl+alt+t"), "Ctrl+Alt+T");
    assert_eq!(hotkey_label("ctrl+shift+space"), "Ctrl+Mayús+Espacio");
}

#[test]
fn the_animation_steps_through_open_and_close() {
    assert_eq!(duration(false, true), CLOSE_SECS);
    assert_eq!(duration(true, false), FADE_SECS);
    let mut finished = false;
    let mut opening = Anim {
        state: AnimState::Opening,
        t0: 0.0,
        p: 0.0,
    };
    assert_eq!(step(&mut opening, MORPH_SECS, false, &mut finished), 1.0);
    assert_eq!(opening.state, AnimState::Open);
    assert!(!finished);
    let mut closing = Anim {
        state: AnimState::Closing,
        t0: 0.0,
        p: 1.0,
    };
    assert_eq!(step(&mut closing, CLOSE_SECS, false, &mut finished), 0.0);
    assert_eq!(closing.state, AnimState::Closing);
    assert!(finished);
}

#[test]
fn elapsed_for_round_trips_through_ease() {
    for target in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let elapsed = elapsed_for(false, target, MORPH_SECS);
        let progress = crate::geometry::ease(elapsed / MORPH_SECS);
        assert!((progress - target).abs() < 1e-6);
    }
    let reduced = elapsed_for(true, 0.5, MORPH_SECS);
    assert!(reduced <= FADE_SECS);
}

#[test]
fn clamping_keeps_the_panel_inside_the_work_area() {
    let work = (0, 0, 1000, 800);
    assert_eq!(clamp_to_work(9999, 9999, 100, 100, work, 16), (916, 716));
    assert_eq!(clamp_to_work(-999, -999, 100, 100, work, 16), (-16, -16));
    assert_eq!(clamp_to_work(300, 200, 100, 100, work, 16), (300, 200));
    assert_eq!(SCREEN_PAD, 12.0);
}

#[test]
fn opening_needs_an_anchor_and_a_view() {
    let ui = fake_ui();
    let recorder = Arc::new(Recorder::default());
    let panel = Arc::new(ChatPanel::new(ui, recorder));
    assert!(!panel.open());
    assert!(!panel.is_open());

    let without_view = FakeUi {
        anchor: Mutex::new(Some((500.0, 500.0))),
        ..FakeUi::default()
    };
    let panel = Arc::new(ChatPanel::new(
        Arc::new(without_view),
        Arc::new(Recorder::default()),
    ));
    assert!(!panel.open());
    assert!(!panel.is_open());
}

#[test]
fn a_closed_panel_swallows_pointer_and_keys() {
    let panel = Arc::new(ChatPanel::new(fake_ui(), Arc::new(Recorder::default())));
    assert!(panel.pointer_down(10, 10));
    assert!(!panel.wheel(120));
    assert!(panel.key(13, false));
    assert!(panel.char("a"));
    assert!(panel.paste("texto"));
    assert!(panel.pointer_up());
    assert_eq!(panel.cursor_ident(18, 60), 0);
    assert!(panel.render().bgra.iter().all(|byte| *byte == 0));
}

#[test]
fn an_empty_zone_lets_the_window_drag() {
    let (panel, _) = open_panel(empty_view());
    assert!(!panel.pointer_down(60, 60));
    assert!(panel.pointer_down(18, 60));
}

#[test]
fn the_border_reports_its_resize_cursor() {
    let (panel, _) = open_panel(empty_view());
    assert_eq!(panel.cursor_ident(18, 60), 32644);
    assert_eq!(panel.cursor_ident(102, 60), 32644);
    assert_eq!(panel.cursor_ident(60, 18), 32645);
    assert_eq!(panel.cursor_ident(18, 18), 32642);
    assert_eq!(panel.cursor_ident(102, 102), 32642);
    assert_eq!(panel.cursor_ident(60, 60), 0);
}

#[test]
fn dropped_files_enter_the_input_as_links() {
    let (panel, _) = open_panel(empty_view());
    panel.files(vec![String::from(r"C:\a b.txt")]);
    let state = panel.state.lock();
    let text = state
        .view
        .as_ref()
        .map(|view| view.input.text().to_string())
        .unwrap_or_default();
    assert_eq!(text, "[a b.txt](<C:\\a b.txt>)\n");
}

#[test]
fn actions_reach_the_callbacks() {
    let (panel, recorder) = open_panel(empty_view());
    panel.perform(Some(Action::with(
        "send",
        ActionData::Text(String::from("hola")),
    )));
    panel.perform(Some(Action::with("voice", ActionData::Flag(true))));
    panel.perform(Some(Action::with("mic", ActionData::Flag(false))));
    panel.perform(Some(Action::new("settings")));
    panel.perform(Some(Action::new("new")));
    panel.perform(Some(Action::new("stop")));
    panel.perform(Some(Action::with("switch", ActionData::Session(3))));
    panel.perform(Some(Action::with("delete", ActionData::Session(4))));
    panel.perform(Some(Action::with("confirm", ActionData::Flag(true))));
    let events = recorder.events.lock().clone();
    assert_eq!(
        events,
        vec![
            ChatEvent::Send(String::from("hola")),
            ChatEvent::Voice(true),
            ChatEvent::Mic(false),
            ChatEvent::Settings,
            ChatEvent::New,
            ChatEvent::Stop,
            ChatEvent::Switch(3),
            ChatEvent::Delete(4),
            ChatEvent::Confirm(true),
        ]
    );
}

#[test]
fn dpi_changes_update_the_scale() {
    let ui = fake_ui();
    let panel = Arc::new(ChatPanel::new(ui.clone(), Arc::new(Recorder::default())));
    panel.dpi_changed(144);
    assert_eq!(ui.scale(), 1.5);
    panel.dpi_changed(72);
    assert_eq!(ui.scale(), 1.0);
}

#[test]
fn render_paints_the_panel_with_its_body_colour() {
    let (panel, _) = open_panel(sample_view());
    let frame = panel.render();
    assert_eq!((frame.width, frame.height), (120, 120));
    let body = frame
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel == [26, 22, 13, 255])
        .count();
    assert!(body > 100, "the body colour is missing ({body} px)");
    assert_eq!(frame.pixel(60, 60)[3], 255);
    assert_eq!(frame.pixel(2, 2)[3], 0);
}

#[test]
fn stable_frames_are_cached_until_something_changes() {
    let (panel, _) = open_panel(sample_view());
    let first = panel.render();
    let second = panel.render();
    assert_eq!(first, second);
    {
        let mut state = panel.state.lock();
        if let Some(view) = state.view.as_mut() {
            view.mode = crate::model::chat::view::Mode::History;
        }
    }
    let third = panel.render();
    assert_ne!(first, third);
}

#[test]
fn closing_restores_the_shape_flags() {
    let (panel, _) = open_panel(empty_view());
    panel.close();
    let state = panel.state.lock();
    assert_eq!(state.anim.state, AnimState::Closing);
    drop(state);
    panel.close_now();
    assert!(!panel.is_open());
}

#[test]
fn a_slow_history_still_animates_the_opening() {
    let ui = Arc::new(FakeUi::default());
    let panel = Arc::new(ChatPanel::new(
        Arc::clone(&ui) as Arc<dyn ChatUi>,
        Arc::new(Recorder::default()),
    ));
    *ui.view.lock() = Some(SendView(Box::new(sample_view())));
    // A real conversation can take longer to paint than the morph lasts.
    *ui.slow_view.lock() = MORPH_SECS + 0.12;
    if !panel.open() {
        // Sin escritorio no hay ventana nativa: el ciclo de vida no aplica.
        return;
    }
    let mut progress = Vec::new();
    for _ in 0..80 {
        let _ = panel.render();
        let (state, p) = {
            let state = panel.state.lock();
            (state.anim.state, state.anim.p)
        };
        progress.push(p);
        if state == AnimState::Open {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let middle = progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count();
    assert!(
        middle >= 3,
        "la apertura solo pintó {middle} cuadros intermedios: {progress:?}"
    );
    assert_eq!(panel.state.lock().anim.state, AnimState::Open);
    panel.close_now();
}

#[test]
fn closing_paints_intermediate_frames_before_removing_the_window() {
    let (panel, _) = open_panel(sample_view());
    // Un cuadro estable deja la capa de contenido cacheada, como en la app.
    let _ = panel.render();
    panel.close();
    let mut progress = Vec::new();
    let mut finished = false;
    for _ in 0..120 {
        let _ = panel.render();
        let (p, done) = {
            let state = panel.state.lock();
            (state.anim.p, state.finished)
        };
        progress.push(p);
        finished = done;
        if done {
            break;
        }
        panel.poll(0.0);
        assert!(
            panel.window.lock().is_some(),
            "la ventana se retiró a mitad del cierre (p={p})"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(finished, "el cierre no terminó: {progress:?}");
    let middle = progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count();
    assert!(
        middle >= 3,
        "el cierre solo pintó {middle} cuadros intermedios: {progress:?}"
    );
    panel.poll(0.0);
    assert!(
        panel.window.lock().is_none(),
        "la ventana siguió tras terminar el cierre"
    );
}
