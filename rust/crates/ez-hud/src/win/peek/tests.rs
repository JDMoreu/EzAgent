//! Pruebas de `win::peek` (F5-06).

use std::sync::Arc;

use parking_lot::Mutex;

use super::controller::{visible_rows, Mode};
use super::*;
use crate::paint::peek::{PeekBuffer, PeekItem};

#[test]
fn place_card_centers_under_the_hub() {
    let hub = (100.0, 100.0, 300.0, 140.0);
    let size = (260, 200);
    let work = (0, 0, 1920, 1080);
    assert_eq!(place_card(hub, size, work, 1.0), (70, 136, false));
    assert_eq!(place_card(hub, size, work, 1.5), (70, 134, false));
}

#[test]
fn place_card_clamps_to_the_work_area_and_flips_above() {
    assert_eq!(
        place_card(
            (50.0, 900.0, 300.0, 980.0),
            (260, 200),
            (0, 0, 1920, 1040),
            1.0
        ),
        (45, 704, true)
    );
    assert_eq!(
        place_card(
            (1800.0, 100.0, 1900.0, 140.0),
            (260, 200),
            (0, 0, 1920, 1080),
            1.0
        ),
        (1654, 136, false)
    );
    assert_eq!(
        place_card(
            (-100.0, 100.0, 50.0, 140.0),
            (260, 200),
            (0, 0, 1920, 1080),
            1.0
        ),
        (-14, 136, false)
    );
    assert_eq!(
        place_card(
            (100.0, 100.0, 300.0, 140.0),
            (300, 300),
            (0, 0, 320, 300),
            1.0
        ),
        (14, -196, true)
    );
}

#[test]
fn compose_fades_and_shifts() {
    let mut buf = PeekBuffer::new(4, 3);
    buf.data[0..4].copy_from_slice(&[1.0, 0.5, 0.25, 1.0]);
    let index = (4 + 2) * 4;
    buf.data[index..index + 4].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
    let index = (2 * 4 + 3) * 4;
    buf.data[index..index + 4].copy_from_slice(&[0.9, 0.8, 0.7, 0.6]);

    let frame = compose(&buf, 1.0, true, 1.0, false);
    assert_eq!(&frame.data[0..4], &[1.0, 0.5, 0.25, 1.0]);
    assert_eq!(frame.data[index], 0.9);

    let faded = compose(&buf, 0.5, true, 1.0, true);
    assert_eq!(&faded.data[0..4], &[0.5, 0.25, 0.125, 0.5]);
    assert_eq!(faded.data[index], 0.45);

    // p = 0.9 y escala 2: el desplazamiento es 1 px y además funde.
    let shifted = compose(&buf, 0.9, false, 2.0, false);
    assert_close(shifted.pixel(3, 1), [0.81, 0.72, 0.63, 0.54]);
    let above = compose(&buf, 0.9, true, 2.0, false);
    assert_close(above.pixel(0, 1), [0.9, 0.45, 0.225, 0.9]);
}

#[test]
fn compose_matches_the_numpy_frame() {
    let mut buf = PeekBuffer::new(4, 3);
    buf.data[0..4].copy_from_slice(&[1.0, 0.5, 0.25, 1.0]);
    let index = 24;
    buf.data[index..index + 4].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
    let index = 44;
    buf.data[index..index + 4].copy_from_slice(&[0.9, 0.8, 0.7, 0.6]);

    let full = compose(&buf, 1.0, true, 1.0, false);
    assert_eq!(
        bgra(&full),
        vec![
            64, 128, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 77, 51,
            26, 102, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 204, 230, 153
        ]
    );

    let reduced = compose(&buf, 0.5, true, 1.0, true);
    assert_eq!(
        bgra(&reduced),
        vec![
            32, 64, 128, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 26,
            13, 51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 89, 102, 115, 77
        ]
    );

    let shifted = compose(&buf, 0.9, true, 2.0, false);
    assert_eq!(
        bgra(&shifted),
        vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 57, 115, 230, 230, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 46, 23, 92, 0, 0, 0, 0
        ]
    );
}

fn bgra(frame: &PeekBuffer) -> Vec<u8> {
    ez_gfx::bgra8_premultiplied(frame.width as u32, frame.height as u32, &frame.data)
        .expect("el búfer tiene el tamaño de la imagen")
}

fn assert_close(actual: [f32; 4], expected: [f32; 4]) {
    for (left, right) in actual.iter().zip(expected) {
        assert!((left - right).abs() < 1e-6, "{actual:?} != {expected:?}");
    }
}

fn ui(scale: f64) -> Arc<FakeUi> {
    Arc::new(FakeUi {
        scale,
        dismissed: Mutex::new(Vec::new()),
        opened: Mutex::new(Vec::new()),
    })
}

struct FakeUi {
    scale: f64,
    dismissed: Mutex<Vec<String>>,
    opened: Mutex<Vec<String>>,
}

impl PeekUi for FakeUi {
    fn bar_scale(&self) -> Option<f64> {
        Some(self.scale)
    }
    fn scale(&self) -> f64 {
        self.scale
    }
    fn theme_name(&self) -> String {
        String::from("dark")
    }
    fn on_activity_dismiss(&self, key: &str) {
        self.dismissed.lock().push(key.to_string());
    }
    fn on_activity_open(&self, key: &str) {
        self.opened.lock().push(key.to_string());
    }
}

fn item(key: &str, state: &str, live: bool, unseen: bool) -> PeekItem {
    PeekItem {
        key: key.to_string(),
        title: String::from("Tarea"),
        agent: String::from("opencode"),
        state: state.to_string(),
        since: 0.0,
        unseen,
        live,
    }
}

#[test]
fn the_controller_opens_on_hover_and_closes_after_the_grace() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui, Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![item("k", "working", true, false)]);
    assert!(!peek.is_open());
    peek.tick(0.0, true, false);
    assert!(!peek.is_open());
    peek.tick(0.3, true, false);
    assert!(peek.is_open());
    peek.tick(0.4, false, false);
    assert!(peek.is_open());
    peek.tick(0.8, false, false);
    assert!(!peek.is_open());
}

#[test]
fn a_toast_shows_only_its_row() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui, Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![
        item("a", "needs_you", true, false),
        item("b", "done", false, true),
    ]);
    peek.toast(&item("b", "done", false, true), 10.0);
    peek.tick(10.0, false, false);
    let state = peek.lock.lock();
    assert_eq!(state.mode, Some(Mode::Toast));
    let rows = visible_rows(&state);
    assert_eq!(rows.0.len(), 1);
    assert_eq!(rows.0[0].key, "b");
    assert_eq!(rows.1, 0);
}

#[test]
fn row_and_dismiss_zones_follow_the_card_geometry() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui, Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![
        item("a", "working", true, false),
        item("b", "done", false, true),
        item("c", "failed", false, false),
    ]);
    {
        let mut state = peek.lock.lock();
        state.rows = (state.items.clone(), 0);
    }
    assert_eq!(peek.row_at(100.0, 20.0), Some(0));
    assert_eq!(peek.row_at(100.0, 57.0), Some(1));
    assert_eq!(peek.row_at(100.0, 95.0), Some(2));
    assert_eq!(peek.row_at(13.0, 20.0), None);
    assert_eq!(peek.row_at(246.0, 20.0), Some(0));
    assert_eq!(peek.row_at(100.0, 150.0), None);
    assert_eq!(peek.dismiss_at(225.0, 20.0).as_deref(), Some("a"));
    assert_eq!(peek.dismiss_at(220.0, 20.0).as_deref(), Some("a"));
    assert_eq!(peek.dismiss_at(240.0, 20.0).as_deref(), Some("a"));
    assert_eq!(peek.dismiss_at(100.0, 20.0), None);
}

#[test]
fn dismiss_removes_the_row_and_notifies_the_ui() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui.clone(), Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![
        item("a", "working", true, false),
        item("b", "done", false, true),
    ]);
    {
        let mut state = peek.lock.lock();
        state.rows = (state.items.clone(), 1);
    }
    peek.dismiss("a");
    assert_eq!(peek.lock.lock().items.len(), 1);
    assert_eq!(peek.lock.lock().rows.0.len(), 1);
    assert_eq!(ui.dismissed.lock().as_slice(), ["a"]);
}

#[test]
fn click_opens_the_clicked_row() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui.clone(), Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![
        item("a", "working", true, false),
        item("b", "done", false, true),
    ]);
    {
        let mut state = peek.lock.lock();
        state.rows = (state.items.clone(), 0);
    }
    peek.click(100.0, 57.0);
    assert_eq!(ui.opened.lock().as_slice(), ["b"]);
    assert_eq!(ui.dismissed.lock().len(), 0);
}

#[test]
fn frames_round_trip_premultiplied_pixels() {
    let mut buf = PeekBuffer::new(2, 1);
    buf.data[0..4].copy_from_slice(&[0.1, 0.2, 0.3, 0.5]);
    buf.data[4..8].copy_from_slice(&[0.0, 0.0, 0.0, 0.0]);
    let frame = super::window::to_frame(&buf).expect("búfer válido");
    assert_eq!(&frame.data[0..4], &[51, 101, 153, 128]);
    assert_eq!(&frame.data[4..8], &[0, 0, 0, 0]);
    assert_eq!(&frame.to_bgra_premultiplied()[0..4], &[77, 51, 26, 128]);
}

#[test]
fn render_paints_the_card_through_the_window_size() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui, Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![
        item("a", "working", true, false),
        item("b", "needs_you", true, false),
    ]);
    {
        let mut state = peek.lock.lock();
        state.rows = (state.items.clone(), 0);
        state.mode = Some(Mode::Hover);
        state.shown_at = ez_core::metrics::monotonic() - 1.0;
    }
    let window = Arc::new(PeekWindow::new(&peek, 0, 0, 260, 200));
    *peek.win.lock() = Some(window);
    let frame = peek.render();
    assert_eq!((frame.width, frame.height), (260, 200));
    let body = [13.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0, 1.0];
    let corner = frame.pixel(43, 20);
    for channel in 0..4 {
        assert!((corner[channel] - body[channel]).abs() < 1e-6);
    }
    // La marca de «en curso» cambia algún píxel de la columna del indicador.
    let lit = (24..41)
        .flat_map(|y| (24..41).map(move |x| (x, y)))
        .filter(|&(x, y)| frame.pixel(x, y)[3] > 0.0 && frame.pixel(x, y) != body)
        .count();
    assert!(lit > 10, "la marca en curso no llegó al cuadro ({lit} px)");
}
