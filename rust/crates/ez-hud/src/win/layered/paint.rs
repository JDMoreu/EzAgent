//! DIB y presentación de fotogramas de `layered` (F5-01).
//!
//! `Dib` es el `CreateDIBSection` con su DC y su memoria; aquí también vive el
//! `UpdateLayeredWindow` que hace visible un [`RgbaFrame`]. El DIB se reutiliza
//! mientras quepa: recrearlo por fotograma provocaba reallocaciones y *access
//! violations* en la animación de altura.

use std::ffi::c_void;
use std::mem::size_of;
use std::time::Instant;

use windows::Win32::Foundation::{GetLastError, COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HDC, HGDIOBJ,
    RGBQUAD,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, UpdateLayeredWindow, HWND_TOPMOST, SWP_NOACTIVATE, ULW_ALPHA,
};

use crate::pacing::paint_mark;

use super::super::pointer::BgraFrame;
use super::types::RgbaFrame;
use super::LayeredWindow;

/// DIB propio (las direcciones se guardan como enteros para que el estado
/// compartido siga siendo `Send`).
#[derive(Default)]
pub(super) struct Dib {
    pub(super) dc: usize,
    pub(super) bmp: usize,
    pub(super) old: usize,
    pub(super) bits: usize,
    pub(super) size: (i32, i32),
    pub(super) last: Option<BgraFrame>,
}

impl Dib {
    pub(super) fn create(&mut self, width: i32, height: i32) -> bool {
        self.release();
        if width <= 0 || height <= 0 {
            return false;
        }
        // SAFETY: `CreateCompatibleDC(None)` usa la pantalla; sin punteros propios.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.0.is_null() {
            return false;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            bmiColors: [RGBQUAD::default()],
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` describe un DIB de 32 bits y `bits` recibe el puntero.
        let bitmap =
            unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) };
        let bitmap = match bitmap {
            Ok(bitmap) if !bits.is_null() => bitmap,
            _ => {
                // SAFETY: el DC se creó arriba y no se usa más.
                unsafe {
                    let _ = DeleteDC(dc);
                }
                return false;
            }
        };
        // SAFETY: `bitmap` es un HBITMAP válido del DC recién creado.
        let old = unsafe { SelectObject(dc, HGDIOBJ(bitmap.0)) };
        self.dc = dc.0 as usize;
        self.bmp = bitmap.0 as usize;
        self.old = old.0 as usize;
        self.bits = bits as usize;
        self.size = (width, height);
        true
    }

    pub(super) fn release(&mut self) {
        // SAFETY: los identificadores son de este DIB y se liberan una sola vez.
        unsafe {
            if self.dc != 0 && self.old != 0 {
                SelectObject(
                    HDC(self.dc as *mut c_void),
                    HGDIOBJ(self.old as *mut c_void),
                );
            }
            if self.bmp != 0 {
                let _ = DeleteObject(HGDIOBJ(self.bmp as *mut c_void));
            }
            if self.dc != 0 {
                let _ = DeleteDC(HDC(self.dc as *mut c_void));
            }
        }
        // OJO: dejar `bits` apuntando a un DIB liberado era la carrera que
        // reventaba en memmove (access violation en el hilo de dibujo).
        *self = Self::default();
    }

    fn copy_bgra(&mut self, source: &[u8], width: i32, height: i32) -> bool {
        let (dib_width, dib_height) = self.size;
        let row = width as usize * 4;
        if self.bits == 0
            || width <= 0
            || height <= 0
            || width > dib_width
            || height > dib_height
            || source.len() < row * height as usize
        {
            return false;
        }
        let target = self.bits as *mut u8;
        if width == dib_width {
            // SAFETY: el DIB tiene `dib_width * dib_height * 4` bytes y el origen
            // trae al menos `row * height`.
            unsafe {
                std::ptr::copy_nonoverlapping(source.as_ptr(), target, row * height as usize);
            }
        } else {
            for index in 0..height as usize {
                // SAFETY: cada fila cabe en el DIB (width <= dib_width) y el
                // origen se lee hasta `row * height`.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        source.as_ptr().add(index * row),
                        target.add(index * dib_width as usize * 4),
                        row,
                    );
                }
            }
        }
        true
    }
}

impl LayeredWindow {
    pub(super) fn dib_create(&self) -> bool {
        let (width, height) = {
            let state = self.state.lock();
            (state.w, state.h)
        };
        self.dib.lock().create(width, height)
    }

    /// Extrae el fotograma pendiente y lo presenta (con su medición de cuadro).
    pub(super) fn paint_pending(&self, now: f64, interval: f64) {
        let frame = self.pending_frame.lock().take();
        let Some(frame) = frame else {
            // Sin fotograma no hay nada que presentar; el reloj avanza igual
            // para que `wait_delay` no quede negativo y el bucle no gire en vacío.
            let mark = paint_mark(now, self.state.lock().last_paint, interval);
            self.state.lock().last_paint = mark;
            return;
        };
        let started = Instant::now();
        self.paint(&frame);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.frame_stats.lock().add(elapsed_ms);
        let mark = paint_mark(now, self.state.lock().last_paint, interval);
        self.state.lock().last_paint = mark;
    }

    fn paint(&self, frame: &RgbaFrame) {
        let width = frame.width as i32;
        let height = frame.height as i32;
        let bgra = frame.to_bgra_premultiplied();
        let (resized, dc) = {
            let mut dib = self.dib.lock();
            let target_height = height.max(self.max_height);
            let needs_dib = dib.bits == 0 || dib.size.0 != width || dib.size.1 < height;
            if needs_dib && !dib.create(width, target_height) {
                // SAFETY: consulta del último error del hilo.
                let error = unsafe { GetLastError() }.0;
                drop(dib);
                self.set_diag(format!("CreateDIBSection fallo ({error})"));
                self.state.lock().dirty = false;
                return;
            }
            if !dib.copy_bgra(&bgra, width, height) {
                drop(dib);
                self.state.lock().dirty = false;
                return;
            }
            dib.last = Some(BgraFrame {
                width,
                height,
                pixels: bgra,
            });
            let mut state = self.state.lock();
            let resized = (width, height) != (state.w, state.h);
            state.w = width;
            state.h = height;
            (resized, dib.dc)
        };
        let hwnd = self.hwnd();
        if resized && hwnd != 0 {
            let (x, y) = self.position();
            // SAFETY: ajuste de tamaño de una ventana propia.
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd as *mut c_void),
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE,
                );
            }
        }
        if hwnd == 0 {
            self.state.lock().dirty = false;
            return;
        }
        let destination = POINT {
            x: self.x(),
            y: self.y(),
        };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let source = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: DIB y ventana vivos; los punteros son locales y válidos.
        let result = unsafe {
            UpdateLayeredWindow(
                HWND(hwnd as *mut c_void),
                None,
                Some(&destination),
                Some(&size),
                Some(HDC(dc as *mut c_void)),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        match result {
            Ok(()) => self.set_diag(String::new()),
            Err(_) => {
                // SAFETY: consulta del último error del hilo.
                let error = unsafe { GetLastError() }.0;
                self.set_diag(format!("UpdateLayeredWindow fallo ({error})"));
            }
        }
        self.state.lock().dirty = false;
    }
}
