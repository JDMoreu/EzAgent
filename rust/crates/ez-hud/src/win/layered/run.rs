//! Hilo y bucle de la ventana de `layered` (F5-01).
//!
//! Aquí viven la creación nativa, el bucle de mensajes con su espera
//! `MsgWaitForMultipleObjects`, la sincronización de `show`/`set_pos`/teclado
//! con el hilo dueño y el `timeBeginPeriod(1)` del Python.

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use windows::Win32::Foundation::{GetLastError, HWND, POINT};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, SetWindowPos,
    ShowWindow, TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, QS_ALLINPUT, SWP_NOACTIVATE,
    SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
};

use crate::pacing::{paint_due, wait_delay};

use super::super::wndproc;
use super::LayeredWindow;

impl LayeredWindow {
    pub(super) fn run(self: Arc<Self>) {
        *self.owner.lock() = Some(thread::current().id());
        wndproc::register(self.wid, &self);
        let ok = self.run_loop();
        let hwnd = self.hwnd();
        wndproc::unregister(self.wid, hwnd);
        self.set_hwnd(0);
        self.ok.store(ok, Ordering::SeqCst);
        self.mark_created();
    }

    fn mark_created(&self) {
        *self.created.lock() = true;
        self.created_cv.notify_all();
    }

    fn run_loop(self: &Arc<Self>) -> bool {
        if !super::super::available() || !wndproc::ensure_class() {
            return false;
        }
        let (x, y, width, height) = {
            let state = self.state.lock();
            (state.x, state.y, state.w, state.h)
        };
        let hwnd = wndproc::create_window(
            self,
            self.wid,
            (x, y, width, height),
            &self.title,
            self.click_through,
        );
        let Some(hwnd) = hwnd else {
            return false;
        };
        // SAFETY: ventana recién creada y viva en este hilo.
        unsafe {
            DragAcceptFiles(hwnd, true);
        }
        if !self.dib_create() {
            // SAFETY: consulta del último error del hilo.
            let error = unsafe { GetLastError() }.0;
            self.set_diag(format!("CreateDIBSection fallo ({error})"));
            // SAFETY: la ventana es propia y se destruye una sola vez.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return false;
        }
        if self.want_mouse_hook {
            super::super::wheel::install_mouse_hook(self);
        }
        // SAFETY: resolución de temporizador pedida por el Python (winmm).
        unsafe {
            timeBeginPeriod(1);
        }
        self.sync_native();
        // La ventana ya nació: `start` puede volver mientras el bucle sigue.
        self.ok.store(true, Ordering::SeqCst);
        self.mark_created();
        self.message_loop();
        super::super::wheel::uninstall_mouse_hook(self);
        self.dib.lock().release();
        // SAFETY: fin del bucle: la ventana propia se destruye una sola vez.
        unsafe {
            let _ = DestroyWindow(hwnd);
            timeEndPeriod(1);
        }
        true
    }

    fn message_loop(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            self.pump();
            let now = super::super::monotonic_seconds();
            super::super::keyboard::restore_focus_if_due(self, now);
            self.sync_native();
            let interval = self.interval;
            let (visible, dirty, dragging, last_paint) = {
                let state = self.state.lock();
                (
                    state.visible,
                    state.dirty,
                    self.lazy_drag && state.dragging && state.drag_moved,
                    state.last_paint,
                )
            };
            if paint_due(visible, dragging, dirty, now, last_paint, interval) {
                self.paint_pending(now, interval);
            }
            let mut delay = wait_delay(
                visible,
                self.state.lock().last_paint,
                interval,
                super::super::monotonic_seconds(),
            );
            if dragging {
                delay = 1.0 / 120.0; // sin pintar: solo espera mensajes del ratón
            }
            if delay > 0.001 {
                let millis = (delay * 1000.0).max(1.0) as u32;
                // SAFETY: espera con cola de mensajes; sin handles propios.
                unsafe {
                    MsgWaitForMultipleObjects(None, false, millis, QS_ALLINPUT);
                }
            }
        }
    }

    fn sync_native(&self) {
        let hwnd = self.hwnd();
        if hwnd == 0 {
            return;
        }
        let hwnd = HWND(hwnd as *mut c_void);
        let show = {
            let mut state = self.state.lock();
            if state.visible == state.applied_visible {
                None
            } else {
                state.applied_visible = state.visible;
                Some((state.visible, state.x, state.y, state.w, state.h))
            }
        };
        if let Some((visible, x, y, width, height)) = show {
            if visible {
                // SAFETY: mostrar y reafirmar topmost una ventana propia.
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE,
                    );
                }
            } else {
                // SAFETY: ocultar una ventana propia.
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            self.state.lock().native_pos = (x, y);
        }
        let moved = {
            let mut state = self.state.lock();
            if (state.x, state.y) == state.native_pos {
                None
            } else {
                state.native_pos = (state.x, state.y);
                Some((state.x, state.y))
            }
        };
        if let Some((x, y)) = moved {
            // SAFETY: reafirmar topmost sin cambiar el tamaño de una ventana propia.
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        let keyboard = {
            let mut state = self.state.lock();
            if state.keyboard == state.applied_keyboard {
                None
            } else {
                let was = state.applied_keyboard;
                state.applied_keyboard = state.keyboard;
                Some((state.keyboard, was))
            }
        };
        if let Some((enabled, was)) = keyboard {
            super::super::keyboard::apply_keyboard(self, enabled, was);
        }
    }

    fn pump(&self) {
        let mut message = MSG::default();
        // SAFETY: bucle estándar; el MSG es local y del hilo actual.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: el mensaje lo produjo PeekMessageW para este hilo.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

/// `_screen_cursor`: posición del cursor en pantalla.
pub(crate) fn screen_cursor() -> (i32, i32) {
    let mut point = POINT::default();
    // SAFETY: `point` es local y válido durante la llamada.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
    }
    (point.x, point.y)
}
