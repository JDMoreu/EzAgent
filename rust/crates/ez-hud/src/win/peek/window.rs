//! Ventana *layered* que presenta la tarjeta de la vista rápida (`peek.py`).

use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use ez_gfx::Image;

use crate::paint::peek::PeekBuffer;
use crate::win::layered::{LayeredWindow, RgbaFrame, WindowHooks, WindowOptions};

use super::controller::Peek;

/// Intervalo inicial de pintura (`1.0 / 60.0`).
pub const DEFAULT_INTERVAL: f64 = 1.0 / 60.0;

struct PaintState {
    interval: f64,
    dirty: bool,
    stop: bool,
}

/// Ventana de la tarjeta; crea su ventana nativa al primer uso.
pub struct PeekWindow {
    peek: Weak<Peek>,
    window: Arc<LayeredWindow>,
    paint: Mutex<PaintState>,
    paint_cv: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl PeekWindow {
    /// Crea la ventana (todavía no nace: hace falta [`PeekWindow::start`]).
    pub fn new(peek: &Arc<Peek>, x: i32, y: i32, w: i32, h: i32) -> Self {
        let mut options = WindowOptions::new(w, h);
        options.x = x;
        options.y = y;
        options.click_through = false;
        options.title = String::from("peek");
        options.lazy_drag = true;
        options.interval = DEFAULT_INTERVAL;
        options.mouse_hook = false;
        let window = Arc::new(LayeredWindow::new(options));
        window.set_hooks(PeekHooks {
            peek: Arc::downgrade(peek),
            window: Arc::downgrade(&window),
        });
        Self {
            peek: Arc::downgrade(peek),
            window,
            paint: Mutex::new(PaintState {
                interval: DEFAULT_INTERVAL,
                dirty: true,
                stop: false,
            }),
            paint_cv: Condvar::new(),
            thread: Mutex::new(None),
        }
    }

    /// Arranca la ventana y su hilo de pintura (bloquea hasta que nace).
    pub fn start(self: &Arc<Self>) {
        if self.thread.lock().is_some() {
            self.window.show();
            return;
        }
        if !self.window.start() {
            return;
        }
        let window = Arc::clone(self);
        let handle = thread::Builder::new()
            .name(String::from("ezagent-peek-paint"))
            .spawn(move || window.paint_loop())
            .ok();
        *self.thread.lock() = handle;
    }

    /// Detiene el hilo de pintura y la ventana.
    pub fn stop(&self) {
        {
            let mut state = self.paint.lock();
            state.stop = true;
            self.paint_cv.notify_all();
        }
        let current = thread::current().id();
        if let Some(handle) = self.thread.lock().take() {
            if handle.thread().id() != current {
                let _ = handle.join();
            }
        }
        self.window.stop(true);
    }

    pub fn show(&self) {
        self.window.show();
        self.mark_dirty();
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn resize(&self, w: i32, h: i32) {
        self.window.resize(w, h);
    }

    pub fn set_pos(&self, x: i32, y: i32) {
        self.window.set_pos(x, y);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn size(&self) -> (i32, i32) {
        self.window.size()
    }

    pub fn position(&self) -> (i32, i32) {
        self.window.position()
    }

    /// `_interval`: cadencia de pintura que fija el controlador.
    pub(super) fn set_interval(&self, seconds: f64) {
        let mut state = self.paint.lock();
        state.interval = seconds.max(0.001);
        state.dirty = true;
        self.paint_cv.notify_all();
    }

    /// `_dirty = True`: pinta cuanto antes.
    pub(super) fn mark_dirty(&self) {
        let mut state = self.paint.lock();
        state.dirty = true;
        self.paint_cv.notify_all();
    }

    /// `PeekWindow.render`: el cuadro de la tarjeta.
    pub fn render(&self) -> PeekBuffer {
        self.peek
            .upgrade()
            .map_or_else(|| PeekBuffer::new(1, 1), |peek| peek.render())
    }

    fn paint_loop(self: &Arc<Self>) {
        let mut state = self.paint.lock();
        loop {
            if state.stop {
                break;
            }
            if !state.dirty {
                let interval = state.interval;
                self.paint_cv
                    .wait_for(&mut state, Duration::from_secs_f64(interval));
                if state.stop {
                    break;
                }
                if !state.dirty {
                    continue;
                }
            }
            state.dirty = false;
            drop(state);
            if self.window.is_visible() {
                if let Some(frame) = to_frame(&self.render()) {
                    self.window.present(frame);
                }
            }
            state = self.paint.lock();
        }
    }
}

impl Drop for PeekWindow {
    fn drop(&mut self) {
        if self.thread.lock().is_some() {
            self.stop();
        }
    }
}

/// `RgbaFrame` a partir del búfer premultiplicado (RGBA recto para `layered`).
pub(super) fn to_frame(buf: &PeekBuffer) -> Option<RgbaFrame> {
    let image =
        Image::from_f32_premultiplied_rgba(buf.width as u32, buf.height as u32, &buf.data).ok()?;
    RgbaFrame::new(image.width(), image.height(), image.as_raw().to_vec())
}

struct PeekHooks {
    peek: Weak<Peek>,
    window: Weak<LayeredWindow>,
}

impl WindowHooks for PeekHooks {
    fn on_pointer_down(&mut self, _x: i32, _y: i32) -> bool {
        true // la tarjeta no se arrastra
    }

    fn on_pointer_up(&mut self, x: i32, y: i32) {
        if let Some(peek) = self.peek.upgrade() {
            peek.click(f64::from(x), f64::from(y));
        }
    }

    fn cursor_hand(&mut self, x: i32, y: i32) -> bool {
        self.peek
            .upgrade()
            .is_some_and(|peek| peek.row_at(f64::from(x), f64::from(y)).is_some())
    }

    fn clamp_position(&mut self, x: i32, y: i32, _width: i32) -> (i32, i32) {
        self.window
            .upgrade()
            .map_or((x, y), |window| window.position())
    }
}
