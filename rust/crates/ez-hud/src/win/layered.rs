//! Port of `src/ezagent/voice/hud/layered.py` — ventana *layered* (F5-01).
//!
//! Ventana `WS_EX_LAYERED` con alpha por píxel (`UpdateLayeredWindow` sobre un
//! DIB BGRA premultiplicado) y bucle de dibujo propio. El dibujo no vive aquí:
//! el dueño construye un [`RgbaFrame`] y lo entrega con
//! [`LayeredWindow::present`]; los mensajes de puntero, teclado y rueda llegan
//! por el canal de [`WindowEvent`] y, cuando hacen falta respuestas síncronas
//! (arrastre, cursor, límites), por [`WindowHooks`].
//!
//! Todas las llamadas de Win32 se hacen en el hilo de la ventana. Los métodos
//! públicos (`show`, `set_pos`, `present`…) solo cambian estado y despiertan la
//! cola de mensajes, así que nunca bloquean al llamador.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle, ThreadId};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use super::frame_stats::FrameStats;
use super::pointer::{self, HitRegion};
use super::wndproc;

mod paint;
mod run;
mod types;

pub(crate) use run::screen_cursor;
pub use types::{RgbaFrame, WindowEvent, WindowHooks, WindowOptions, DEFAULT_INTERVAL, FPS_ACTIVE};

use paint::Dib;
use types::State;

/// Ventana *layered* con su hilo y su cola de mensajes.
pub struct LayeredWindow {
    pub title: String,
    pub lazy_drag: bool,
    pub max_height: i32,
    pub click_through: bool,
    interval: f64,
    want_mouse_hook: bool,
    wid: usize,
    hwnd: AtomicIsize,
    pub(crate) state: Mutex<State>,
    dib: Mutex<Dib>,
    hooks: Mutex<Box<dyn WindowHooks>>,
    events_tx: Sender<WindowEvent>,
    events_rx: Mutex<Option<Receiver<WindowEvent>>>,
    pending_frame: Mutex<Option<RgbaFrame>>,
    frame_stats: Mutex<FrameStats>,
    created: Mutex<bool>,
    created_cv: Condvar,
    stop: AtomicBool,
    ok: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
    owner: Mutex<Option<ThreadId>>,
    mouse_hook: Mutex<isize>,
}

impl LayeredWindow {
    /// Crea la ventana (todavía no nace: hace falta `start`).
    pub fn new(options: WindowOptions) -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let wid = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            title: options.title.clone(),
            lazy_drag: options.lazy_drag,
            max_height: options.max_height,
            click_through: options.click_through,
            interval: options.interval,
            want_mouse_hook: options.mouse_hook,
            wid,
            hwnd: AtomicIsize::new(0),
            state: Mutex::new(State::new(
                options.width,
                options.height,
                options.x,
                options.y,
            )),
            dib: Mutex::new(Dib::default()),
            hooks: Mutex::new(Box::new(())),
            events_tx,
            events_rx: Mutex::new(Some(events_rx)),
            pending_frame: Mutex::new(None),
            frame_stats: Mutex::new(FrameStats::new(options.title)),
            created: Mutex::new(false),
            created_cv: Condvar::new(),
            stop: AtomicBool::new(false),
            ok: AtomicBool::new(false),
            thread: Mutex::new(None),
            owner: Mutex::new(None),
            mouse_hook: Mutex::new(0),
        }
    }

    /// Canal de eventos (solo se entrega una vez).
    pub fn events(&self) -> Option<Receiver<WindowEvent>> {
        self.events_rx.lock().take()
    }

    /// Instala las respuestas síncronas del dueño.
    pub fn set_hooks<H: WindowHooks + 'static>(&self, hooks: H) {
        *self.hooks.lock() = Box::new(hooks);
    }

    /// Reemplaza las zonas interactivas (`_hits`).
    pub fn set_hit_regions(&self, hits: Vec<HitRegion>) {
        self.state.lock().hits = hits;
    }

    /// Arranca el hilo de la ventana; `false` si no hay sesión gráfica.
    ///
    /// Una ventana que se arranca es una ventana que se quiere ver: el Python
    /// la dejaba creada pero oculta y el bucle no pintaba nada.
    pub fn start(self: &Arc<Self>) -> bool {
        if !super::available() {
            return false;
        }
        {
            let mut state = self.state.lock();
            state.visible = true;
            state.dirty = true;
        }
        if self.is_running() {
            self.show();
            return true;
        }
        self.stop.store(false, Ordering::SeqCst);
        self.ok.store(false, Ordering::SeqCst);
        *self.created.lock() = false;
        let window = Arc::clone(self);
        let name = format!("ezagent-{}", self.title);
        let handle = thread::Builder::new()
            .name(name)
            .spawn(move || window.run())
            .expect("no se pudo crear el hilo de la ventana");
        *self.thread.lock() = Some(handle);
        let mut created = self.created.lock();
        if !*created {
            self.created_cv
                .wait_for(&mut created, Duration::from_secs_f64(4.0));
        }
        self.ok.load(Ordering::SeqCst)
    }

    /// Detiene el hilo y, opcionalmente, cierra la ventana.
    pub fn stop(&self, destroy: bool) {
        self.stop.store(true, Ordering::SeqCst);
        let current = thread::current().id();
        let owner = *self.owner.lock();
        let handle = self.thread.lock().take();
        if let Some(handle) = handle {
            if Some(current) != owner {
                let _ = handle.join();
            }
        }
        let hwnd = self.hwnd();
        if destroy && hwnd != 0 {
            // SAFETY: WM_CLOSE a una ventana propia; si ya no existe, falla sin daño.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        self.state.lock().visible = false;
    }

    pub fn show(&self) {
        {
            let mut state = self.state.lock();
            state.visible = true;
            state.dirty = true;
        }
        self.wake();
    }

    pub fn hide(&self) {
        {
            let mut state = self.state.lock();
            state.visible = false;
            state.dirty = true;
        }
        self.wake();
    }

    pub fn set_pos(&self, x: i32, y: i32) {
        let lazy = self.lazy_drag && self.hwnd() != 0;
        {
            let mut state = self.state.lock();
            state.x = x;
            state.y = y;
            if !lazy {
                state.dirty = true;
            }
        }
        self.wake();
    }

    /// Cambia el tamaño (el DIB se recrea en el hilo de dibujo).
    pub fn resize(&self, width: i32, height: i32) {
        {
            let mut state = self.state.lock();
            if (width, height) == (state.w, state.h) {
                return;
            }
            state.w = width;
            state.h = height;
            state.dirty = true;
        }
        self.wake();
    }

    /// Entrega un fotograma RGBA; se presenta en el hilo de la ventana.
    pub fn present(&self, frame: RgbaFrame) {
        *self.pending_frame.lock() = Some(frame);
        self.state.lock().dirty = true;
        self.wake();
    }

    /// Último motivo de fallo nativo (vacío si todo fue bien).
    pub fn diag(&self) -> String {
        self.state.lock().diag.clone()
    }

    pub fn is_visible(&self) -> bool {
        self.state.lock().visible
    }

    pub fn size(&self) -> (i32, i32) {
        let state = self.state.lock();
        (state.w, state.h)
    }

    pub fn position(&self) -> (i32, i32) {
        let state = self.state.lock();
        (state.x, state.y)
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd.load(Ordering::SeqCst)
    }

    /// Activa el teclado temporalmente (retira `WS_EX_NOACTIVATE`).
    pub fn set_keyboard_enabled(&self, enabled: bool) {
        super::keyboard::set_keyboard_enabled(self, enabled);
    }

    /// Texto del portapapeles (Unicode), o cadena vacía.
    pub fn clipboard_text(&self) -> String {
        super::keyboard::clipboard_text(self.hwnd())
    }

    /// Pone `text` en el portapapeles (Unicode). `false` si no se pudo.
    pub fn copy_text(&self, text: &str) -> bool {
        super::keyboard::copy_text(self.hwnd(), text)
    }

    /// Resumen de tiempos de cuadro (se registra cada `REPORT_SECS`).
    pub fn frame_summary(&self) -> String {
        self.frame_stats.lock().summary()
    }

    pub(crate) fn x(&self) -> i32 {
        self.state.lock().x
    }

    pub(crate) fn y(&self) -> i32 {
        self.state.lock().y
    }

    pub(crate) fn hwnd_raw(&self) -> HWND {
        HWND(self.hwnd() as *mut c_void)
    }

    pub(crate) fn set_hwnd(&self, hwnd: isize) {
        self.hwnd.store(hwnd, Ordering::SeqCst);
    }

    pub(crate) fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub(crate) fn emit(&self, event: WindowEvent) {
        let _ = self.events_tx.send(event);
    }

    pub(crate) fn alpha_at(&self, x: i32, y: i32) -> u8 {
        let (width, height) = {
            let state = self.state.lock();
            (state.w, state.h)
        };
        if !(0 <= x && x < width && 0 <= y && y < height) {
            return 0;
        }
        let dib = self.dib.lock();
        dib.last.as_ref().map_or(0, |frame| frame.alpha_at(x, y))
    }

    pub(crate) fn hit_action(&self, x: i32, y: i32) -> Option<String> {
        let hits = &self.state.lock().hits;
        pointer::hit_action(hits, x, y)
    }

    pub(crate) fn set_hover(&self, hover: Option<String>) -> bool {
        let mut state = self.state.lock();
        if state.hover == hover {
            return false;
        }
        state.hover = hover;
        state.dirty = true;
        true
    }

    pub(crate) fn set_diag(&self, diag: String) {
        self.state.lock().diag = diag;
    }

    pub(crate) fn on_pointer_down(&self, x: i32, y: i32) -> bool {
        self.hooks.lock().on_pointer_down(x, y)
    }

    pub(crate) fn on_pointer_move(&self, x: i32, y: i32) {
        self.hooks.lock().on_pointer_move(x, y);
    }

    pub(crate) fn on_pointer_up(&self, x: i32, y: i32) {
        self.hooks.lock().on_pointer_up(x, y);
    }

    pub(crate) fn on_wheel(&self, x: i32, y: i32, delta: i32) -> bool {
        self.hooks.lock().on_wheel(x, y, delta)
    }

    pub(crate) fn on_global_wheel(&self, x: i32, y: i32, delta: i32) -> bool {
        self.hooks.lock().on_global_wheel(x, y, delta)
    }

    pub(crate) fn on_key(&self, vk: u32, ctrl: bool) -> bool {
        self.hooks.lock().on_key(vk, ctrl)
    }

    pub(crate) fn on_char(&self, character: char) -> bool {
        self.hooks.lock().on_char(character)
    }

    pub(crate) fn on_paste(&self, text: &str) -> bool {
        self.hooks.lock().on_paste(text)
    }

    pub(crate) fn cursor_hand(&self, x: i32, y: i32) -> bool {
        self.hooks.lock().cursor_hand(x, y)
    }

    pub(crate) fn cursor_ident(&self, x: i32, y: i32) -> u16 {
        self.hooks.lock().cursor_ident(x, y)
    }

    pub(crate) fn clamp_position(&self, x: i32, y: i32) -> (i32, i32) {
        let width = self.state.lock().w;
        self.hooks.lock().clamp_position(x, y, width)
    }

    pub(crate) fn mouse_hook(&self) -> isize {
        *self.mouse_hook.lock()
    }

    pub(crate) fn set_mouse_hook(&self, hook: isize) {
        *self.mouse_hook.lock() = hook;
    }

    pub(crate) fn take_mouse_hook(&self) -> isize {
        std::mem::take(&mut *self.mouse_hook.lock())
    }

    pub(crate) fn wake(&self) {
        let hwnd = self.hwnd();
        if hwnd != 0 {
            // SAFETY: WM_NULL a una ventana propia: solo despierta su cola.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    wndproc::WM_NULL,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }

    fn is_running(&self) -> bool {
        self.thread
            .lock()
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(lazy_drag: bool, hwnd: isize) -> LayeredWindow {
        let mut options = WindowOptions::new(100, 100);
        options.lazy_drag = lazy_drag;
        let window = LayeredWindow::new(options);
        window.set_hwnd(hwnd);
        window
    }

    fn position_and_dirty(window: &LayeredWindow) -> ((i32, i32), bool) {
        let state = window.state.lock();
        ((state.x, state.y), state.dirty)
    }

    #[test]
    fn moving_a_window_only_repaints_when_its_frame_depends_on_position() {
        for (lazy, expected_dirty) in [(true, false), (false, true)] {
            let window = window(lazy, 0x7FFF_0001);
            window.state.lock().dirty = false;
            window.set_pos(40, 60);
            assert_eq!(
                position_and_dirty(&window),
                ((40, 60), expected_dirty),
                "lazy={lazy}"
            );
        }
    }

    #[test]
    fn a_lazy_window_without_a_handle_still_marks_dirty() {
        let window = window(true, 0);
        window.state.lock().dirty = false;
        window.set_pos(5, 5);
        assert_eq!(position_and_dirty(&window), ((5, 5), true));
    }
}
