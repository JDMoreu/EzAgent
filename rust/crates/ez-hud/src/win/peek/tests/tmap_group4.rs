//! Casos de `test_activity.py` que la vista rápida aún no cubría (TMAP-4).

use std::sync::Arc;

use super::*;
use crate::paint::peek::IconPainter;

struct NoActivityUi;

impl PeekUi for NoActivityUi {
    fn bar_scale(&self) -> Option<f64> {
        None
    }
    fn scale(&self) -> f64 {
        1.0
    }
    fn theme_name(&self) -> String {
        String::from("dark")
    }
}

fn no_activity_peek() -> Arc<Peek> {
    Arc::new(Peek::new(
        Arc::new(NoActivityUi),
        Arc::new(crate::paint::peek::NoIcons) as Arc<dyn IconPainter>,
    ))
}

#[test]
fn without_activity_the_card_never_opens() {
    let peek = no_activity_peek();
    peek.tick(0.0, true, false);
    peek.tick(1.0, true, false);
    assert!(!peek.is_open());
}

#[test]
fn the_menu_or_the_chat_closes_the_card() {
    let peek = Arc::new(Peek::new(ui(1.0), Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![item("a", "working", true, false)]);
    peek.tick(0.0, true, false);
    peek.tick(0.3, true, false);
    assert!(peek.is_open());
    peek.tick(0.4, true, true);
    assert!(!peek.is_open());
}

#[test]
fn asking_by_voice_shows_the_full_list_for_a_while() {
    let peek = Arc::new(Peek::new(ui(1.0), Arc::new(crate::paint::peek::NoIcons)));
    let items: Vec<_> = (0..6)
        .map(|index| item(&index.to_string(), "working", true, false))
        .collect();
    peek.refresh(items);
    peek.reveal(0.0);
    peek.tick(0.0, false, false);
    {
        let state = peek.lock.lock();
        assert_eq!(state.mode, Some(Mode::Toast));
        let rows = visible_rows(&state);
        assert_eq!(rows.0.len(), MAX_ROWS);
        assert_eq!(rows.1, 2);
    }
    peek.tick(REVEAL_SECS + 0.1, false, false);
    assert!(!peek.is_open());
}

/// Hub falso cuya barra usa una escala distinta de la de la pantalla.
struct ScaledHubUi {
    scale: f64,
    bar_scale: f64,
}

impl PeekUi for ScaledHubUi {
    fn bar_scale(&self) -> Option<f64> {
        Some(self.bar_scale)
    }
    fn scale(&self) -> f64 {
        self.scale
    }
    fn theme_name(&self) -> String {
        String::from("dark")
    }
    fn hub_extent(&self) -> Option<(f64, f64, f64, f64)> {
        Some((100.0, 900.0, 300.0, 980.0))
    }
    fn work_area(&self) -> (i32, i32, i32, i32) {
        (0, 0, 1920, 1080)
    }
}

#[test]
fn the_card_follows_the_bar_size_not_only_the_screen() {
    let scale = 1.5;
    let bar_scale = scale * 0.85;
    let peek = Arc::new(Peek::new(
        Arc::new(ScaledHubUi { scale, bar_scale }),
        Arc::new(crate::paint::peek::NoIcons),
    ));
    let surface = SyncSurface::new(1, 1, false);
    *peek.win.lock() = Some(Arc::clone(&surface) as Arc<dyn PeekSurface>);
    peek.refresh(vec![item("a", "working", true, false)]);
    peek.tick(0.0, true, false);
    peek.tick(0.3, true, false);
    let expected = (
        ((PEEK_W + 2.0 * MARGIN) * bar_scale).round_ties_even() as i32,
        ((ROW_H + 2.0 * MARGIN) * bar_scale).round_ties_even() as i32,
    );
    assert_eq!(surface.size(), expected);
}

#[test]
fn window_operations_from_another_thread_only_touch_the_state() {
    use crate::win::layered::{LayeredWindow, WindowOptions};
    let window = Arc::new(LayeredWindow::new(WindowOptions::new(120, 80)));
    let moved = Arc::clone(&window);
    let worker = std::thread::spawn(move || {
        moved.show();
        moved.set_pos(5, 6);
        assert!(moved.is_visible());
        assert_eq!(moved.position(), (5, 6));
    });
    worker.join().expect("las operaciones no bloquean");
    assert!(window.is_visible());
    assert_eq!(window.position(), (5, 6));
}
