//! `DropWindow`: la ventana de la barra (`drop.py 846-894`).
//!
//! Presenta los fotogramas de [`DropUI::render_frame`] y traduce los eventos
//! del puntero a coordenadas lógicas. No instala el gancho global de ratón: la
//! barra no lo necesita (solo la rueda del menú), y tenerlo hacía esperar a
//! este hilo cada movimiento del sistema.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;

use crate::win::layered::{LayeredWindow, WindowEvent, WindowHooks, WindowOptions};

use super::ui::{DropUI, HudWindow};

/// Intervalo inicial de pintura (`1.0 / 30.0`).
pub const DEFAULT_INTERVAL: f64 = 1.0 / 30.0;

/// Ventana de la barra con su hilo de pintura.
pub struct DropWindow {
    ui: Weak<DropUI>,
    self_weak: Weak<DropWindow>,
    window: Arc<LayeredWindow>,
    stop: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
    events: Mutex<Option<Receiver<WindowEvent>>>,
}

impl DropWindow {
    /// Crea la ventana (todavía no nace: hace falta [`DropWindow::start`]).
    pub fn new(ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32) -> Arc<Self> {
        let mut options = WindowOptions::new(width, height);
        options.x = x;
        options.y = y;
        options.click_through = false;
        options.title = String::from("drop");
        options.lazy_drag = false;
        options.interval = DEFAULT_INTERVAL;
        options.mouse_hook = false;
        let window = Arc::new(LayeredWindow::new(options));
        window.set_hooks(BarHooks { ui: ui.clone() });
        Arc::new_cyclic(|weak| Self {
            ui,
            self_weak: weak.clone(),
            window,
            stop: AtomicBool::new(false),
            thread: Mutex::new(None),
            events: Mutex::new(None),
        })
    }

    fn run(self: &Arc<Self>) {
        if !self.window.start() {
            tracing::warn!("barra: no se pudo crear la ventana");
            return;
        }
        if self.events.lock().is_none() {
            *self.events.lock() = self.window.events();
        }
        while !self.stop.load(Ordering::SeqCst) {
            self.pump_events();
            let interval = if self.window.is_visible() {
                let Some(ui) = self.ui.upgrade() else {
                    break;
                };
                if self.stop.load(Ordering::SeqCst) {
                    break;
                }
                let frame = ui.render_frame();
                if self.stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Some(rgba) = frame.to_rgba() {
                    self.window.present(rgba);
                }
                frame.interval()
            } else {
                0.05
            };
            thread::sleep(Duration::from_secs_f64(interval.clamp(0.001, 0.25)));
        }
    }

    fn pump_events(&self) {
        let events = self.events.lock();
        let Some(events) = events.as_ref() else {
            return;
        };
        while let Ok(event) = events.try_recv() {
            match event {
                WindowEvent::DpiChanged(dpi) => {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.set_scale((f64::from(dpi) / 96.0).max(1.0));
                    }
                }
                WindowEvent::DragEnd { x, y } => {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.drag_end(x, y);
                    }
                }
                _ => {}
            }
        }
    }
}

impl HudWindow for DropWindow {
    fn start(&self) -> bool {
        if self.thread.lock().is_some() {
            self.window.show();
            return true;
        }
        self.stop.store(false, Ordering::SeqCst);
        let weak = self.self_weak.clone();
        let spawned = thread::Builder::new()
            .name(String::from("ezagent-drop-paint"))
            .spawn(move || {
                if let Some(window) = weak.upgrade() {
                    window.run();
                }
            });
        match spawned {
            Ok(handle) => {
                *self.thread.lock() = Some(handle);
                true
            }
            Err(error) => {
                tracing::warn!("barra: no se pudo arrancar el hilo de pintura: {error}");
                false
            }
        }
    }

    fn show(&self) {
        self.window.show();
    }

    fn hide(&self) {
        self.window.hide();
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let current = thread::current().id();
        if let Some(handle) = self.thread.lock().take() {
            if handle.thread().id() != current {
                let _ = handle.join();
            }
        }
        self.window.stop(true);
    }

    fn set_pos(&self, x: i32, y: i32) {
        self.window.set_pos(x, y);
    }

    fn resize(&self, width: i32, height: i32) {
        self.window.resize(width, height);
    }

    fn position(&self) -> (i32, i32) {
        self.window.position()
    }

    fn size(&self) -> (i32, i32) {
        self.window.size()
    }

    fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    fn dragging_moved(&self) -> bool {
        let state = self.window.state.lock();
        state.dragging && state.drag_moved
    }

    fn shift_drag(&self, dx: i32, dy: i32) {
        let mut state = self.window.state.lock();
        state.drag_off = (state.drag_off.0 + dx, state.drag_off.1 + dy);
    }

    fn set_keyboard_enabled(&self, enabled: bool) {
        self.window.set_keyboard_enabled(enabled);
    }

    fn copy_text(&self, text: &str) -> bool {
        self.window.copy_text(text)
    }
}

impl Drop for DropWindow {
    fn drop(&mut self) {
        if self.thread.lock().is_some() {
            self.stop();
        }
    }
}

struct BarHooks {
    ui: Weak<DropUI>,
}

impl WindowHooks for BarHooks {
    fn on_pointer_down(&mut self, x: i32, y: i32) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            let scale = ui.bs();
            ui.pointer_down(x as f64 / scale, y as f64 / scale)
        })
    }

    fn on_pointer_up(&mut self, x: i32, y: i32) {
        if let Some(ui) = self.ui.upgrade() {
            let scale = ui.bs();
            ui.pointer_up(x as f64 / scale, y as f64 / scale);
        }
    }

    fn on_wheel(&mut self, x: i32, y: i32, delta: i32) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            let scale = ui.bs();
            ui.bar_wheel(x as f64 / scale, y as f64 / scale, delta)
        })
    }

    fn cursor_hand(&mut self, x: i32, y: i32) -> bool {
        self.ui.upgrade().is_some_and(|ui| ui.cursor_hand(x, y))
    }

    fn clamp_position(&mut self, x: i32, y: i32, _width: i32) -> (i32, i32) {
        self.ui.upgrade().map_or((x, y), |ui| ui.drag_target(x, y))
    }
}
