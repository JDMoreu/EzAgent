//! `CardWindow`: la ventana de la tarjeta de ajustes (`drop.py 896-958`).
//!
//! Aloja el menú y reparte el puntero, el teclado y la rueda a [`DropUI`]. Se
//! pinta aparte de la barra (`lazy_drag`: moverla no la repinta) y solo instala
//! el gancho global de rueda si Windows no la entrega a la ventana bajo el
//! puntero.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;

use crate::win::layered::{LayeredWindow, WindowEvent, WindowHooks, WindowOptions};

use super::ui::{CardText, DropUI, HudWindow, PointerPhase};

/// Intervalo inicial de pintura (`1.0 / 60.0`).
pub const DEFAULT_INTERVAL: f64 = 1.0 / 60.0;

/// Ventana de la tarjeta con su hilo de pintura.
pub struct CardWindow {
    ui: Weak<DropUI>,
    self_weak: Weak<CardWindow>,
    window: Arc<LayeredWindow>,
    stop: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
    events: Mutex<Option<Receiver<WindowEvent>>>,
}

impl CardWindow {
    /// Crea la ventana (todavía no nace: hace falta [`CardWindow::start`]).
    pub fn new(ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32) -> Arc<Self> {
        let mut options = WindowOptions::new(width, height);
        options.x = x;
        options.y = y;
        options.click_through = false;
        options.title = String::from("card");
        options.lazy_drag = true;
        options.interval = DEFAULT_INTERVAL;
        options.mouse_hook = !crate::win::wheel::wheel_follows_pointer();
        let window = Arc::new(LayeredWindow::new(options));
        window.set_hooks(CardHooks {
            ui: ui.clone(),
            window: Arc::downgrade(&window),
        });
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
            tracing::warn!("tarjeta: no se pudo crear la ventana");
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
                let frame = ui.render_card();
                if self.stop.load(Ordering::SeqCst) {
                    break;
                }
                if frame.width > 1 && frame.height > 1 {
                    if let Some(rgba) = frame.to_rgba() {
                        self.window.present(rgba);
                    }
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
                WindowEvent::DragEnd { .. } => {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.card_moved();
                    }
                }
                WindowEvent::FocusLost => {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.card_focus_lost();
                    }
                }
                _ => {}
            }
        }
    }
}

impl HudWindow for CardWindow {
    fn start(&self) -> bool {
        if self.thread.lock().is_some() {
            self.window.show();
            return true;
        }
        self.stop.store(false, Ordering::SeqCst);
        let weak = self.self_weak.clone();
        let spawned = thread::Builder::new()
            .name(String::from("ezagent-card-paint"))
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
                tracing::warn!("tarjeta: no se pudo arrancar el hilo de pintura: {error}");
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

    fn set_keyboard_enabled(&self, enabled: bool) {
        self.window.set_keyboard_enabled(enabled);
    }

    fn copy_text(&self, text: &str) -> bool {
        self.window.copy_text(text)
    }
}

impl Drop for CardWindow {
    fn drop(&mut self) {
        if self.thread.lock().is_some() {
            self.stop();
        }
    }
}

struct CardHooks {
    ui: Weak<DropUI>,
    window: Weak<LayeredWindow>,
}

impl CardHooks {
    fn pointer(&self, phase: PointerPhase, x: i32, y: i32) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.card_pointer(phase, x, y))
    }

    fn text(&self, text: CardText) -> bool {
        self.ui.upgrade().is_some_and(|ui| ui.card_text(&text))
    }
}

impl WindowHooks for CardHooks {
    fn on_pointer_down(&mut self, x: i32, y: i32) -> bool {
        self.pointer(PointerPhase::Down, x, y)
    }

    fn on_pointer_move(&mut self, x: i32, y: i32) {
        self.pointer(PointerPhase::Move, x, y);
    }

    fn on_pointer_up(&mut self, x: i32, y: i32) {
        self.pointer(PointerPhase::Up, x, y);
    }

    fn on_wheel(&mut self, x: i32, y: i32, delta: i32) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.card_wheel(x, y, delta))
    }

    fn on_global_wheel(&mut self, x: i32, y: i32, delta: i32) -> bool {
        let Some(window) = self.window.upgrade() else {
            return false;
        };
        let (window_x, window_y) = window.position();
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.card_wheel(x - window_x, y - window_y, delta))
    }

    fn on_key(&mut self, vk: u32, ctrl: bool) -> bool {
        self.text(CardText::Key(vk, ctrl))
    }

    fn on_char(&mut self, character: char) -> bool {
        self.text(CardText::Char(character))
    }

    fn on_paste(&mut self, text: &str) -> bool {
        self.text(CardText::Paste(text.to_string()))
    }

    fn cursor_hand(&mut self, x: i32, y: i32) -> bool {
        self.ui.upgrade().is_some_and(|ui| ui.card_hand(x, y))
    }

    fn cursor_ident(&mut self, x: i32, y: i32) -> u16 {
        self.ui
            .upgrade()
            .map_or(0, |ui| ui.card_cursor(x, y) as u16)
    }

    fn clamp_position(&mut self, x: i32, y: i32, _width: i32) -> (i32, i32) {
        self.ui.upgrade().map_or((x, y), |ui| ui.clamp_card(x, y))
    }
}
