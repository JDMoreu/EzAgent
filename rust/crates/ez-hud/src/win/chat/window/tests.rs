//! Tests of the chat panel lifecycle, input routing and callbacks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::geometry::{FADE_SECS, MORPH_SECS, SCREEN_PAD};
use crate::model::chat::view::test_support::{FakeHost, FakeInput, FakeModel};
use crate::model::chat::view::{
    Action, ActionData, ChatMessage, ChatModel, ConfirmCard, Role, SessionInfo,
};
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

/// A model whose copy of the history takes its time.
///
/// It stands in for a real conversation: the content layer cannot be painted
/// until the history is copied.
struct SlowModel {
    messages: Vec<ChatMessage>,
    /// Set when the copy starts.
    copying: Arc<AtomicBool>,
    delay: Duration,
}

impl ChatModel for SlowModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        self.copying.store(true, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        (0, self.messages.clone())
    }

    fn version(&self) -> i64 {
        0
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        Vec::new()
    }

    fn busy(&self) -> bool {
        false
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        None
    }

    fn viewing(&self) -> Option<i64> {
        None
    }

    fn current_session(&self) -> Option<i64> {
        None
    }

    fn voice_replies(&self) -> bool {
        false
    }

    fn mic_muted(&self) -> bool {
        false
    }

    fn view_session(&mut self, _: Option<i64>) {}
}

/// Un cuadro lento no bloquea la entrada.
///
/// DECISION: el cuadro se pinta sin el candado del panel (DX-1, paso 5). La
/// capa de contenido (la cara) se dibuja con la vista fuera del estado, así
/// que la app sigue respondiendo mientras el historial se pinta.
#[test]
fn a_slow_frame_does_not_block_the_input() {
    let copying = Arc::new(AtomicBool::new(false));
    let mut view = ChatView::new(
        Box::new(SlowModel {
            messages: vec![ChatMessage::new(1, Role::User, "hola")],
            copying: Arc::clone(&copying),
            delay: Duration::from_millis(400),
        }),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    let (panel, _recorder) = open_panel(view);
    let painter = {
        let panel = Arc::clone(&panel);
        std::thread::spawn(move || {
            let _ = panel.render();
        })
    };
    // Wait until the frame is painting the content.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !copying.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(copying.load(Ordering::SeqCst), "the frame never started");
    // The input must answer while the frame paints.
    let start = Instant::now();
    let consumed = panel.pointer_down(60, 60);
    let elapsed = start.elapsed();
    assert!(consumed);
    assert!(
        elapsed < Duration::from_millis(100),
        "the input waited {elapsed:?} for the frame"
    );
    painter.join().expect("the painter finished");
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

/// Panel with a view already installed, in the given animation state.
fn panel_with_view(state: AnimState, view: ChatView) -> (Arc<ChatPanel>, Arc<FakeUi>) {
    let ui = Arc::new(FakeUi::default());
    let panel = Arc::new(ChatPanel::new(
        Arc::clone(&ui) as Arc<dyn ChatUi>,
        Arc::new(Recorder::default()),
    ));
    {
        let mut inner = panel.state.lock();
        inner.view = Some(SendView(Box::new(view)));
        inner.anim = Anim {
            state,
            t0: 0.0,
            p: if state == AnimState::Open { 1.0 } else { 0.0 },
        };
        inner.last = 0.0;
    }
    let window = Arc::new(ChatWindow::new(&panel, 0, 0, 120, 120));
    *panel.window.lock() = Some(Arc::clone(&window));
    (panel, ui)
}

#[test]
fn input_is_swallowed_while_the_panel_animates() {
    let (panel, _ui) = panel_with_view(AnimState::Opening, empty_view());
    assert!(panel.pointer_down(60, 60));
    assert!(panel.key(13, false));
    assert!(panel.char("a"));
    assert!(panel.paste("b"));
    assert_eq!(panel.cursor_ident(18, 60), 0);
    let text = panel
        .state
        .lock()
        .view
        .as_ref()
        .map(|view| view.input.text().to_string())
        .unwrap_or_default();
    assert!(text.is_empty());
}

#[test]
fn hovering_a_border_does_not_light_up_the_controls() {
    let (panel, _) = open_panel(empty_view());
    assert!(!panel.hover(18, 60));
    assert!(panel
        .state
        .lock()
        .view
        .as_ref()
        .is_some_and(|view| view.hover.is_none()));
}

#[test]
fn the_status_follows_the_bar_and_a_busy_chat_reads_as_thinking() {
    let ui = FakeUi::default();
    let mut view = sample_view();
    let (mut act, mut last) = (0.0, 0.0);
    for (pose, text) in [
        ("listen", "Escuchando…"),
        ("think", "Pensando…"),
        ("speak", "Hablando…"),
        ("idle", ""),
    ] {
        *ui.pose.lock() = pose.to_string();
        update_live(&ui, &mut view, &mut act, &mut last, 0.0);
        assert_eq!(view.status, text, "{pose}");
    }
    // En reposo la actividad se queda a cero.
    *ui.pose.lock() = "idle".to_string();
    for step in 1..=30 {
        update_live(&ui, &mut view, &mut act, &mut last, f64::from(step) / 30.0);
    }
    assert_eq!(act, 0.0);
    // Pensando, la actividad sube hasta el máximo.
    *ui.pose.lock() = "think".to_string();
    for step in 1..=30 {
        update_live(&ui, &mut view, &mut act, &mut last, f64::from(step) / 30.0);
    }
    assert!(act > 0.95);
    // Un chat ocupado se lee como «Pensando…» aunque la barra esté en reposo.
    let mut busy = ChatView::new(
        Box::new(FakeModel {
            busy: true,
            ..FakeModel::default()
        }),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    *ui.pose.lock() = "idle".to_string();
    update_live(&ui, &mut busy, &mut act, &mut last, 0.0);
    assert_eq!(busy.status, "Pensando…");
}

#[test]
fn the_form_hides_only_after_the_first_painted_frame() {
    let (panel, ui) = panel_with_view(AnimState::Opening, sample_view());
    assert!(!*ui.form_hidden.lock());
    let _ = panel.render();
    assert!(*ui.form_hidden.lock());
}

#[test]
fn the_repaint_asks_for_sixty_fps_only_while_something_moves() {
    let mut view = empty_view();
    assert_eq!(
        frame_interval(AnimState::Opening, &mut view, false, 0.0, 0.0),
        1.0 / 60.0
    );
    assert_eq!(
        frame_interval(AnimState::Open, &mut view, false, 0.0, 0.0),
        1.0 / 30.0
    );
    assert_eq!(
        frame_interval(AnimState::Open, &mut view, true, 0.0, 0.0),
        1.0 / 60.0
    );
    assert_eq!(
        frame_interval(AnimState::Open, &mut view, false, 1.0, 0.0),
        1.0 / 60.0
    );
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
fn open_and_reveal_actions_are_forwarded() {
    let (panel, recorder) = open_panel(empty_view());
    let info = crate::paint::chat::FileInfo {
        path: std::path::PathBuf::from("A"),
        name: "A".to_string(),
        ext: String::new(),
        size: 0,
        mtime: 0.0,
        kind: crate::paint::chat::MediaKind::Other,
        allowed: true,
        openable: true,
    };
    panel.perform(Some(Action::with("open", ActionData::File(info.clone()))));
    panel.perform(Some(Action::with("reveal", ActionData::File(info.clone()))));
    panel.perform(None);
    assert_eq!(
        recorder.events.lock().clone(),
        vec![ChatEvent::Open(info.clone()), ChatEvent::Reveal(info)]
    );
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
    // La animación va por reloj: con el equipo cargado se pintan menos cuadros.
    // Lo que se vigila es que la carga lenta del historial no se coma la
    // animación (antes el primer cuadro ya salía con p = 1).
    let middle = progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count();
    assert!(
        middle >= 1,
        "la apertura no pintó cuadros intermedios: {progress:?}"
    );
    assert!(
        progress.first().is_some_and(|p| *p < 0.5),
        "la apertura empezó casi terminada: {progress:?}"
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
    // La animación va por reloj: con el equipo cargado se pintan menos cuadros;
    // basta con que el cierre se vea antes de retirar la ventana.
    let middle = progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count();
    assert!(
        middle >= 1,
        "el cierre no pintó cuadros intermedios: {progress:?}"
    );
    panel.poll(0.0);
    assert!(
        panel.window.lock().is_none(),
        "la ventana siguió tras terminar el cierre"
    );
}

/// DX-1f: una apertura con historial lento tiene que animarse.
///
/// Antes, el primer cuadro de la apertura maquetaba todo el historial (aquí,
/// dos copias lentas): cuando terminaba, el morph ya había pasado y la persona
/// veía el panel aparecer de golpe. Durante el morph el historial no se
/// maqueta; aparece cuando está, ya con el panel abierto.
#[test]
fn a_slow_history_does_not_eat_the_opening_animation() {
    let copying = Arc::new(AtomicBool::new(false));
    let mut view = ChatView::new(
        Box::new(SlowModel {
            messages: vec![ChatMessage::new(1, Role::User, "hola")],
            copying: Arc::clone(&copying),
            delay: Duration::from_millis(300),
        }),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    let (panel, _recorder) = open_panel(view);
    {
        let mut state = panel.state.lock();
        state.anim = Anim {
            state: AnimState::Opening,
            t0: ez_core::metrics::monotonic(),
            p: 0.0,
        };
    }
    let started = Instant::now();
    let frame = panel.render();
    let took = started.elapsed();
    let (anim, p) = {
        let state = panel.state.lock();
        (state.anim.state, state.anim.p)
    };
    assert_eq!((frame.width, frame.height), (120, 120));
    assert!(
        took < Duration::from_millis(150),
        "el primer cuadro de la apertura tardó {took:?}: maquetó el historial"
    );
    assert!(
        anim == AnimState::Opening,
        "el morph terminó en el primer cuadro"
    );
    assert!(p < 0.5, "la apertura empezó casi terminada: p={p}");
    // El contenido llega después, cuando el panel ya está abierto.
    {
        let mut state = panel.state.lock();
        state.anim = Anim {
            state: AnimState::Open,
            t0: 0.0,
            p: 1.0,
        };
    }
    let _ = panel.render();
    let renders = panel
        .state
        .lock()
        .view
        .as_ref()
        .map_or(0, |view| view.content_renders());
    assert!(renders > 0, "el contenido no se pintó nunca");
}

/// DX-1f: el cuadro que termina el morph no maqueta bajo el candado.
///
/// `step` pasa a `Open` dentro del cuadro. Si ese cuadro tomara el camino
/// estable, maquetaría el historial **con el candado del panel tomado** (lo
/// que DX-1b quitó, y la app volvía a quedarse sin responder). El cuadro que
/// cruza el final sigue siendo del morph; el historial se maqueta en el
/// siguiente, con la vista fuera del candado.
#[test]
fn the_frame_that_ends_the_morph_does_not_lay_out_the_history() {
    let (panel, _) = open_panel(empty_view());
    {
        let mut state = panel.state.lock();
        state.view = Some(SendView(Box::new(sample_view())));
        state.anim = Anim {
            state: AnimState::Opening,
            t0: ez_core::metrics::monotonic() - MORPH_SECS,
            p: 1.0,
        };
        state.content = compose::ContentCache::default();
    }
    let _ = panel.render();
    assert_eq!(panel.state.lock().anim.state, AnimState::Open);
    let layouts = panel
        .state
        .lock()
        .view
        .as_ref()
        .map_or(0, |view| view.layouts());
    assert_eq!(
        layouts, 0,
        "el cuadro que termina el morph maquetó el historial"
    );
    // El siguiente cuadro ya es el panel estable: ahí llega el contenido.
    let _ = panel.render();
    let layouts = panel
        .state
        .lock()
        .view
        .as_ref()
        .map_or(0, |view| view.layouts());
    assert!(layouts > 0, "el contenido no llegó al panel abierto");
}

/// DX-1g: el panel pinta **una** sombra suave, no varias apiladas.
///
/// El alfa del borde crece hacia el cuerpo sin escalones (la caída suave de
/// `rounded_shadow`) y nunca pasa del pico de una sola sombra: 40/255, el alfa
/// que dejaban las tres capas del diseño anterior. Si la sombra se compusiera
/// dos veces, el borde llegaría a 80 y la prueba falla. Se mira en el panel
/// quieto, en la apertura, en el cierre y tras redimensionar.
#[test]
fn the_panel_paints_one_soft_shadow() {
    let mut view = empty_view();
    view.configure(
        440.0,
        640.0,
        1.0,
        "light",
        crate::tokens::palette("light").expect("the light palette exists"),
    );
    let ui = Arc::new(FakeUi {
        theme: "light",
        ..FakeUi::default()
    });
    let panel = Arc::new(ChatPanel::new(
        Arc::clone(&ui) as Arc<dyn ChatUi>,
        Arc::new(Recorder::default()),
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
    let window = Arc::new(ChatWindow::new(&panel, 0, 0, 472, 672));
    *panel.window.lock() = Some(Arc::clone(&window));
    {
        let mut inner = panel.state.lock();
        inner.form = (16.0, 16.0, 456.0, 656.0);
    }
    for state in [AnimState::Open, AnimState::Opening, AnimState::Closing] {
        // El morph ya avanzado (no al final: el cierre dura 0,20 s y se apagaría).
        let elapsed = if state == AnimState::Closing {
            0.05
        } else {
            0.25
        };
        {
            let mut inner = panel.state.lock();
            inner.anim = Anim {
                state,
                t0: ez_core::metrics::monotonic() - elapsed,
                p: 0.9,
            };
        }
        let frame = panel.render();
        assert_one_soft_shadow(&frame, 336, state);
    }
    // Redimensionado: el mismo panel en una ventana más alta.
    //
    // El bucle deja el panel en `Closing` y el morph apaga la sombra con el
    // progreso (`shadow_k`): sin volver a `Open`, este cuadro dependía del
    // reloj real (en debug el cierre ya había avanzado entre un cuadro y el
    // siguiente y no quedaba sombra; en release sí).
    {
        let mut inner = panel.state.lock();
        inner.anim = Anim {
            state: AnimState::Open,
            t0: 0.0,
            p: 1.0,
        };
    }
    window.resize(472, 732);
    let frame = panel.render();
    assert_one_soft_shadow(&frame, 366, AnimState::Open);
}

/// El borde izquierdo, desde fuera hasta el cuerpo: una sola sombra.
fn assert_one_soft_shadow(frame: &compose::Frame, row: usize, state: AnimState) {
    let profile: Vec<u8> = (0..120).map(|x| frame.pixel(x, row)[3]).collect();
    let body = profile
        .iter()
        .position(|alpha| *alpha == 255)
        .unwrap_or_else(|| panic!("the body is missing ({state:?}): {profile:?}"));
    let ring = &profile[..body];
    // Suave y de una sola pasada: cada píxel, hacia el cuerpo, no aclara.
    assert!(
        ring.windows(2).all(|pair| pair[0] <= pair[1]),
        "the shadow has a step ({state:?}): {profile:?}"
    );
    // El pico de una sola sombra (40/255); dos composiciones darían ~80.
    let peak = ring.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= 40,
        "the shadow is composed more than once ({state:?}, peak {peak}): {profile:?}"
    );
    // Una sombra suave tiene varios niveles; tres bandas planas no.
    let levels: std::collections::BTreeSet<u8> = ring.iter().copied().filter(|a| *a > 0).collect();
    assert!(
        levels.len() >= 5,
        "the shadow is stepped, not soft ({state:?}): {profile:?}"
    );
}

/// Un parser que entra en pánico: la maquetación del historial falla.
struct PanicHost;

impl crate::paint::chat::ChatHost for PanicHost {
    fn parse(&self, _: &str) -> Vec<crate::paint::chat::Block> {
        panic!("DX-1d: pánico de maquetación")
    }

    fn resolve_target(&self, _: &str, _: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
        None
    }

    fn file_info(
        &self,
        _: Option<&std::path::Path>,
        _: Option<&[std::path::PathBuf]>,
    ) -> Option<crate::paint::chat::FileInfo> {
        None
    }

    fn load_image(&self, _: &crate::paint::chat::FileInfo) -> Option<ez_gfx::raster::Image> {
        None
    }

    fn text_preview(&self, _: &crate::paint::chat::FileInfo) -> Option<(String, bool)> {
        None
    }
}

impl crate::model::chat::view::ViewHost for PanicHost {
    fn parse_inline(&self, _: &str) -> Vec<crate::paint::chat::Span> {
        panic!("DX-1d: pánico de maquetación")
    }
}

/// DX-1d: si la maquetación de un mensaje entra en pánico, la vista vuelve al
/// panel y el cuadro siguiente se pinta. Antes la vista se perdía y el chat se
/// quedaba muerto en 1×1 para siempre.
#[test]
fn a_panicking_layout_keeps_the_view_and_the_panel_recovers() {
    let mut view = ChatView::new(
        Box::new(FakeModel {
            messages: vec![ChatMessage::new(1, Role::Assistant, "hola".to_string())],
            ..FakeModel::default()
        }),
        Box::new(PanicHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        100.0,
        100.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    let (panel, _) = open_panel(view);
    let frame = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panel.render()));
    let frame = frame.expect("la maquetación fallida tumbó el cuadro");
    assert!(
        panel.state.lock().view.is_some(),
        "la vista se perdió al desenrollar"
    );
    assert_eq!((frame.width, frame.height), (120, 120));
}
