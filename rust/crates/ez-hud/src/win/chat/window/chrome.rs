//! The layered window of the chat panel and its interaction (`chat_ui.py`).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use crate::win::layered::{LayeredWindow, RgbaFrame, WindowEvent, WindowHooks, WindowOptions};
use crate::win::wheel;

use super::ChatPanel;

/// Initial paint interval (`1.0 / 60.0` in Python).
pub const DEFAULT_INTERVAL: f64 = 1.0 / 60.0;

struct PaintState {
    interval: f64,
    stop: bool,
}

/// The chat window; it is created on `start` and presents the panel frames.
pub struct ChatWindow {
    panel: Weak<ChatPanel>,
    window: Arc<LayeredWindow>,
    paint: Mutex<PaintState>,
    paint_cv: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
    events: Mutex<Option<Receiver<WindowEvent>>>,
}

impl ChatWindow {
    /// Creates the window (it is not shown until [`ChatWindow::start`]).
    pub fn new(panel: &Arc<ChatPanel>, x: i32, y: i32, width: i32, height: i32) -> Self {
        let mut options = WindowOptions::new(width, height);
        options.x = x;
        options.y = y;
        options.click_through = false;
        options.title = String::from("EzAgent");
        options.lazy_drag = true;
        options.interval = DEFAULT_INTERVAL;
        // Python skips the low-level mouse hook when the wheel follows the pointer.
        options.mouse_hook = !wheel::wheel_follows_pointer();
        let window = Arc::new(LayeredWindow::new(options));
        let events = window.events();
        window.set_hooks(ChatHooks {
            panel: Arc::downgrade(panel),
            window: Arc::downgrade(&window),
        });
        Self {
            panel: Arc::downgrade(panel),
            window,
            paint: Mutex::new(PaintState {
                interval: DEFAULT_INTERVAL,
                stop: false,
            }),
            paint_cv: Condvar::new(),
            thread: Mutex::new(None),
            events: Mutex::new(events),
        }
    }

    /// Starts the native window and its paint thread; `false` without a desktop.
    pub fn start(self: &Arc<Self>) -> bool {
        if self.thread.lock().is_some() {
            self.window.show();
            return true;
        }
        if !self.window.start() {
            return false;
        }
        self.paint.lock().stop = false;
        let this = Arc::clone(self);
        let handle = thread::Builder::new()
            .name(String::from("ezagent-chat-paint"))
            .spawn(move || this.paint_loop());
        match handle {
            Ok(handle) => {
                *self.thread.lock() = Some(handle);
                true
            }
            Err(error) => {
                tracing::warn!("no se pudo arrancar el hilo del chat: {error}");
                self.window.stop(true);
                false
            }
        }
    }

    /// Stops the paint thread and destroys the window.
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
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn resize(&self, width: i32, height: i32) {
        self.window.resize(width, height);
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

    pub fn copy_text(&self, text: &str) -> bool {
        self.window.copy_text(text)
    }

    pub fn set_keyboard_enabled(&self, enabled: bool) {
        self.window.set_keyboard_enabled(enabled);
    }

    /// `render`: the current panel frame.
    pub fn render(&self) -> RgbaFrame {
        self.panel.upgrade().map_or_else(
            || RgbaFrame::solid(1, 1, [0, 0, 0, 0]),
            |panel| panel.render().to_rgba_frame(),
        )
    }

    /// `_interval`: repaint cadence fixed by the panel each frame.
    pub(crate) fn set_interval(&self, seconds: f64) {
        let mut state = self.paint.lock();
        state.interval = seconds.max(0.001);
        self.paint_cv.notify_all();
    }

    /// Wakes the paint loop (a state change asks for a frame now, not on the
    /// next 30 fps tick).
    pub(crate) fn wake(&self) {
        self.paint_cv.notify_all();
    }

    // ------------------------------------------------------------- input
    pub fn on_pointer_down(&self, x: i32, y: i32) -> bool {
        self.panel
            .upgrade()
            .is_some_and(|panel| panel.pointer_down(x, y))
    }

    pub fn on_pointer_move(&self, x: i32, y: i32) -> bool {
        self.panel
            .upgrade()
            .is_some_and(|panel| panel.pointer_move(x, y))
    }

    pub fn on_pointer_up(&self, _x: i32, _y: i32) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.pointer_up())
    }

    pub fn cursor_hand(&self, x: i32, y: i32) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.hover(x, y))
    }

    pub fn cursor_ident(&self, x: i32, y: i32) -> u16 {
        self.panel
            .upgrade()
            .map_or(0, |panel| panel.cursor_ident(x, y))
    }

    pub fn on_wheel(&self, _x: i32, _y: i32, delta: i32) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.wheel(delta))
    }

    pub fn on_global_wheel(&self, x: i32, y: i32, delta: i32) -> bool {
        let (wx, wy) = self.position();
        let (width, height) = self.size();
        let inside = 0 <= x - wx && x - wx < width && 0 <= y - wy && y - wy < height;
        inside && self.panel.upgrade().is_some_and(|panel| panel.wheel(delta))
    }

    pub fn on_key(&self, vk: i32, ctrl: bool) -> bool {
        self.panel
            .upgrade()
            .is_some_and(|panel| panel.key(vk, ctrl))
    }

    pub fn on_char(&self, value: &str) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.char(value))
    }

    pub fn on_paste(&self, text: &str) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.paste(text))
    }

    pub fn on_files(&self, paths: Vec<PathBuf>) {
        let paths: Vec<String> = paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        if let Some(panel) = self.panel.upgrade() {
            panel.files(paths);
        }
    }

    pub fn on_dpi_changed(&self, dpi: u32) {
        if let Some(panel) = self.panel.upgrade() {
            panel.dpi_changed(dpi);
        }
    }

    pub fn clamp_position(&self, x: i32, y: i32) -> (i32, i32) {
        self.panel
            .upgrade()
            .map_or((x, y), |panel| panel.clamp(x, y))
    }

    // ------------------------------------------------------------- paint
    fn paint_loop(self: &Arc<Self>) {
        let mut state = self.paint.lock();
        loop {
            if state.stop {
                break;
            }
            drop(state);
            self.drain_events();
            // `paint_due`: a lazy-drag window (the chat) skips painting while dragged.
            let dragging = {
                let status = self.window.state.lock();
                self.window.lazy_drag && status.dragging && status.drag_moved
            };
            let mut painted = 0.0;
            if self.window.is_visible() && !dragging {
                let started = ez_core::metrics::monotonic();
                // Python catches render exceptions so the window stays alive.
                match catch_unwind(AssertUnwindSafe(|| self.render())) {
                    Ok(frame) => {
                        let (width, height) = self.window.size();
                        // Never present the 1×1 fallback of a panel without a
                        // view: it would resize the native window.
                        if (frame.width as i32, frame.height as i32) == (width, height) {
                            self.window.present(frame);
                        }
                    }
                    Err(_) => tracing::warn!("el render del chat falló"),
                }
                painted = ez_core::metrics::monotonic() - started;
            }
            state = self.paint.lock();
            if state.stop {
                break;
            }
            // The interval of THIS frame: the panel just set 60 fps for the
            // morph, and waiting on the previous one lost the first frames.
            // `wait_delay` discounts the time the frame took, so 60 fps means
            // 60 frames per second and not 50.
            let interval = state.interval;
            let wait = (interval - painted).max(0.001);
            self.paint_cv
                .wait_for(&mut state, Duration::from_secs_f64(wait));
        }
    }

    fn drain_events(&self) {
        let slot = self.events.lock();
        let Some(receiver) = slot.as_ref() else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            match event {
                WindowEvent::Files(paths) => self.on_files(paths),
                WindowEvent::DpiChanged(dpi) => self.on_dpi_changed(dpi),
                _ => {}
            }
        }
    }
}

impl Drop for ChatWindow {
    fn drop(&mut self) {
        if self.thread.lock().is_some() {
            self.stop();
        }
    }
}

struct ChatHooks {
    panel: Weak<ChatPanel>,
    window: Weak<LayeredWindow>,
}

impl WindowHooks for ChatHooks {
    fn on_pointer_down(&mut self, x: i32, y: i32) -> bool {
        self.panel
            .upgrade()
            .is_some_and(|panel| panel.pointer_down(x, y))
    }

    fn on_pointer_move(&mut self, x: i32, y: i32) {
        if let Some(panel) = self.panel.upgrade() {
            panel.pointer_move(x, y);
        }
    }

    fn on_pointer_up(&mut self, _x: i32, _y: i32) {
        if let Some(panel) = self.panel.upgrade() {
            panel.pointer_up();
        }
    }

    fn on_wheel(&mut self, _x: i32, _y: i32, delta: i32) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.wheel(delta))
    }

    fn on_global_wheel(&mut self, x: i32, y: i32, delta: i32) -> bool {
        let Some(window) = self.window.upgrade() else {
            return false;
        };
        let (wx, wy) = window.position();
        let (width, height) = window.size();
        let inside = 0 <= x - wx && x - wx < width && 0 <= y - wy && y - wy < height;
        inside && self.panel.upgrade().is_some_and(|panel| panel.wheel(delta))
    }

    fn on_key(&mut self, vk: u32, ctrl: bool) -> bool {
        self.panel
            .upgrade()
            .is_some_and(|panel| panel.key(vk as i32, ctrl))
    }

    fn on_char(&mut self, character: char) -> bool {
        let value = character.to_string();
        self.panel.upgrade().is_some_and(|panel| panel.char(&value))
    }

    fn on_paste(&mut self, text: &str) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.paste(text))
    }

    fn cursor_hand(&mut self, x: i32, y: i32) -> bool {
        self.panel.upgrade().is_some_and(|panel| panel.hover(x, y))
    }

    fn cursor_ident(&mut self, x: i32, y: i32) -> u16 {
        self.panel
            .upgrade()
            .map_or(0, |panel| panel.cursor_ident(x, y))
    }

    fn clamp_position(&mut self, x: i32, y: i32, _width: i32) -> (i32, i32) {
        self.panel
            .upgrade()
            .map_or((x, y), |panel| panel.clamp(x, y))
    }
}
