//! Pruebas de `win::peek` (F5-06).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use super::controller::{visible_rows, Mode};
use super::surface::PeekSurface;
use super::*;
use crate::paint::peek::{PeekBuffer, PeekItem};

mod tmap_group4;

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
fn the_autonomous_row_has_no_close_zone_and_still_opens() {
    let ui = ui(1.0);
    let peek = Arc::new(Peek::new(ui.clone(), Arc::new(crate::paint::peek::NoIcons)));
    peek.refresh(vec![item("autonomous", "autonomous", true, false)]);
    {
        let mut state = peek.lock.lock();
        state.rows = (state.items.clone(), 0);
    }
    // Sin ×: pulsar el extremo de la fila abre la fila (que pide confirmación).
    assert_eq!(peek.dismiss_at(240.0, 20.0), None);
    peek.click(240.0, 20.0);
    assert_eq!(ui.opened.lock().as_slice(), ["autonomous"]);
    assert!(ui.dismissed.lock().is_empty());
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

/// Barra con hub: `place` necesita el rectángulo de la forma para colocar.
struct HubUi;

impl PeekUi for HubUi {
    fn bar_scale(&self) -> Option<f64> {
        Some(1.0)
    }
    fn scale(&self) -> f64 {
        1.0
    }
    fn theme_name(&self) -> String {
        String::from("dark")
    }
    fn hub_extent(&self) -> Option<(f64, f64, f64, f64)> {
        Some((100.0, 900.0, 300.0, 980.0))
    }
    fn form_extent(&self) -> Option<(f64, f64, f64)> {
        Some((200.0, 940.0, 24.0))
    }
    fn work_area(&self) -> (i32, i32, i32, i32) {
        (0, 0, 1920, 1080)
    }
}

/// Ventana de la vista rápida que se comporta como Win32: `resize` no vuelve
/// hasta que el hilo de la ventana ha atendido el mensaje, y ese hilo está
/// pintando (`render`) cuando le llega.
struct SyncSurface {
    size: Mutex<(i32, i32)>,
    visible: AtomicBool,
    /// `true` imita a `SetWindowPos`: `resize` espera un cuadro.
    waits: bool,
    frames: Mutex<u64>,
    frames_cv: Condvar,
    stopping: AtomicBool,
    resizes: AtomicUsize,
    /// `true` si algún `resize` no consiguió su cuadro: el lock estaba tomado.
    missed: AtomicBool,
}

impl SyncSurface {
    fn new(width: i32, height: i32, waits: bool) -> Arc<Self> {
        Arc::new(Self {
            size: Mutex::new((width, height)),
            visible: AtomicBool::new(true),
            waits,
            frames: Mutex::new(0),
            frames_cv: Condvar::new(),
            stopping: AtomicBool::new(false),
            resizes: AtomicUsize::new(0),
            missed: AtomicBool::new(false),
        })
    }

    /// El hilo de pintura terminó un cuadro.
    fn frame_done(&self) {
        *self.frames.lock() += 1;
        self.frames_cv.notify_all();
    }

    /// Espera al cuadro siguiente, con plazo: si no llega, la prueba falla en
    /// vez de colgarse (`missed`).
    fn wait_frame(&self) {
        let target = *self.frames.lock() + 1;
        let mut frames = self.frames.lock();
        let waited = self.frames_cv.wait_for(&mut frames, Duration::from_secs(3));
        if waited.timed_out() || *frames < target {
            self.missed.store(true, Ordering::SeqCst);
        }
    }
}

impl PeekSurface for SyncSurface {
    fn start(self: Arc<Self>) {
        self.visible.store(true, Ordering::SeqCst);
    }

    fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    fn show(&self) {
        self.visible.store(true, Ordering::SeqCst);
    }

    fn hide(&self) {
        self.visible.store(false, Ordering::SeqCst);
    }

    fn resize(&self, width: i32, height: i32) {
        self.resizes.fetch_add(1, Ordering::SeqCst);
        if self.waits {
            self.wait_frame();
        }
        *self.size.lock() = (width.max(1), height.max(1));
    }

    fn set_pos(&self, _x: i32, _y: i32) {}

    fn is_visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }

    fn size(&self) -> (i32, i32) {
        *self.size.lock()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn set_interval(&self, _seconds: f64) {}

    fn mark_dirty(&self) {}
}

/// `P-FIX-LOCKAUDIT`: la carrera que congeló la barra en la app real.
///
/// Hilo A (barra): `refresh` + `tick` a ritmo alto con filas que cambian de
/// tamaño, para que cada vuelta pase por `place`. Hilo B (vista rápida):
/// `render` en bucle. Si `place` mueve la ventana con `win` tomado, el mensaje
/// síncrono espera a un `render` que necesita ese mismo lock; y si `tick`
/// vuelve a pedir el estado que ya tiene, el hilo se cuelga. La prueba corre la
/// barra en su hilo y espera con plazo: falla en vez de colgar la suite (con el
/// código anterior a `100d5da` y sin el arreglo de `advance` falla aquí).
#[test]
fn the_card_never_moves_with_its_lock_taken() {
    let peek = Arc::new(Peek::new(
        Arc::new(HubUi),
        Arc::new(crate::paint::peek::NoIcons),
    ));
    let surface = SyncSurface::new(PEEK_W as i32 + 12, 200, true);
    *peek.win.lock() = Some(Arc::clone(&surface) as Arc<dyn PeekSurface>);
    peek.refresh(vec![item("a", "working", true, false)]);

    let painter = {
        let peek = Arc::clone(&peek);
        let surface = Arc::clone(&surface);
        std::thread::spawn(move || {
            while !surface.stopping.load(Ordering::SeqCst) {
                let _ = peek.render();
                surface.frame_done();
            }
        })
    };

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let bar = {
        let peek = Arc::clone(&peek);
        let surface = Arc::clone(&surface);
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut now = 0.0;
            for round in 0..120 {
                if surface.missed.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    break;
                }
                let rows = round % 4 + 1;
                peek.refresh(
                    (0..rows)
                        .map(|index| item(&format!("k{index}"), "working", true, false))
                        .collect(),
                );
                now += 0.05;
                peek.tick(now, true, false);
            }
            let _ = done_tx.send(());
        })
    };

    let finished = done_rx.recv_timeout(Duration::from_secs(20)).is_ok();
    surface.stopping.store(true, Ordering::SeqCst);
    surface.frame_done();
    assert!(
        finished,
        "la barra no terminó sus vueltas: `tick`/`place` se quedaron esperando"
    );
    bar.join().expect("el hilo de la barra terminó");
    painter.join().expect("el hilo de pintura terminó");
    assert!(
        surface.resizes.load(Ordering::SeqCst) > 0,
        "la prueba no forzó ningún `place`"
    );
    assert!(
        !surface.missed.load(Ordering::SeqCst),
        "`resize` esperó a un cuadro con el lock de la ventana tomado"
    );
}

/// `P-FIX-LOCKAUDIT`: `advance` pedía la fila bajo el puntero con `row_at`,
/// que vuelve a bloquear el estado que `tick` ya tiene. Con el puntero sobre la
/// tarjeta abierta, la barra se congelaba en el primer cuadro.
#[test]
fn a_tick_with_the_pointer_over_the_card_does_not_relock_the_state() {
    let peek = Arc::new(Peek::new(
        Arc::new(HubUi),
        Arc::new(crate::paint::peek::NoIcons),
    ));
    *peek.win.lock() = Some(Arc::clone(&SyncSurface::new(260, 200, false)) as Arc<dyn PeekSurface>);
    peek.refresh(vec![item("a", "working", true, false)]);
    {
        let mut state = peek.lock.lock();
        state.rows = (vec![item("a", "working", true, false)], 0);
        state.mode = Some(Mode::Hover);
        state.shown_at = ez_core::metrics::monotonic() - 1.0;
    }
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        peek.tick(ez_core::metrics::monotonic(), false, false);
        let _ = done_tx.send(());
    });
    assert!(
        done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "`tick` volvió a pedir el lock del estado en el mismo hilo"
    );
}
