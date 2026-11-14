//! Port of `src/ezagent/voice/hud/layered.py` — teclado y portapapeles (F5-01).
//!
//! El teclado solo se activa mientras se edita un campo de texto: se retira
//! `WS_EX_NOACTIVATE`, se toma el foco y, al terminar, se devuelve el foco a la
//! ventana anterior con un pequeño aplazamiento (para que el `WM_CHAR` del
//! Enter no acabe en la app de antes). También vive aquí el portapapeles.

use std::ffi::c_void;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, IsWindow, SetForegroundWindow, SetWindowLongPtrW,
    ShowWindow, GWL_EXSTYLE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
};

use super::layered::LayeredWindow;

/// `CF_UNICODETEXT`.
const CF_UNICODETEXT: u32 = 13;
/// `VK_CONTROL`.
const VK_CONTROL: u16 = 0x11;
/// `VK_V`.
const VK_V: u32 = 86;
/// Aplazamiento para devolver el foco (segundos).
const RESTORE_FOCUS_DELAY: f64 = 0.08;

/// `GetAsyncKeyState(VK_CONTROL) & 0x8000`.
pub fn ctrl_down() -> bool {
    // SAFETY: consulta de estado de tecla sin punteros.
    unsafe { (GetAsyncKeyState(VK_CONTROL as i32) as u16) & 0x8000 != 0 }
}

/// `vk == 86 and ctrl` (Ctrl+V).
pub fn is_paste_shortcut(vk: u32, ctrl: bool) -> bool {
    vk == VK_V && ctrl
}

/// Activa el teclado temporalmente retirando `WS_EX_NOACTIVATE`.
pub fn set_keyboard_enabled(window: &LayeredWindow, enabled: bool) {
    window.state.lock().keyboard = enabled;
    window.wake();
}

/// Aplica el cambio de teclado en el hilo de la ventana.
pub(crate) fn apply_keyboard(window: &LayeredWindow, enabled: bool, was_enabled: bool) {
    let hwnd = window.hwnd();
    if hwnd == 0 {
        return;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    if enabled && !was_enabled {
        {
            let mut state = window.state.lock();
            state.restore_foreground_at = 0.0;
            state.restore_foreground_hwnd = 0;
        }
        // SAFETY: consulta Win32 sin punteros.
        let previous = unsafe { GetForegroundWindow() };
        window.state.lock().previous_foreground = previous.0 as isize;
    }
    // SAFETY: la ventana vive hasta el final del bucle; solo se ajusta su estilo.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let style = if enabled {
        style & !(WS_EX_NOACTIVATE.0 as isize)
    } else {
        style | WS_EX_NOACTIVATE.0 as isize
    };
    // SAFETY: misma ventana viva, valor de estilo calculado arriba.
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
    if enabled {
        // SAFETY: mostrar/activar una ventana propia es válido en su hilo.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
    } else {
        let previous = std::mem::take(&mut window.state.lock().previous_foreground);
        let current = hwnd.0 as isize;
        if previous != 0 && previous != current {
            let previous_hwnd = HWND(previous as *mut c_void);
            // SAFETY: `IsWindow` acepta cualquier identificador y no lo desreferencia.
            if unsafe { IsWindow(Some(previous_hwnd)) }.as_bool() {
                let mut state = window.state.lock();
                state.restore_foreground_hwnd = previous;
                state.restore_foreground_at = super::monotonic_seconds() + RESTORE_FOCUS_DELAY;
            }
        }
    }
}

/// Devuelve el foco a la ventana anterior cuando toca.
pub(crate) fn restore_focus_if_due(window: &LayeredWindow, now: f64) {
    let previous = {
        let mut state = window.state.lock();
        if state.restore_foreground_at == 0.0 || now < state.restore_foreground_at {
            return;
        }
        state.restore_foreground_at = 0.0;
        std::mem::take(&mut state.restore_foreground_hwnd)
    };
    let hwnd = window.hwnd();
    if previous == 0 || hwnd == 0 || previous == hwnd {
        return;
    }
    // SAFETY: consultas Win32 sobre identificadores vivos o nulos.
    if unsafe { GetForegroundWindow() }.0 as isize != hwnd {
        return;
    }
    let previous_hwnd = HWND(previous as *mut c_void);
    // SAFETY: `IsWindow` no desreferencia el identificador.
    if unsafe { IsWindow(Some(previous_hwnd)) }.as_bool() {
        // SAFETY: solo se activa una ventana que Windows considera existente.
        unsafe {
            let _ = SetForegroundWindow(previous_hwnd);
        }
    }
}

/// Texto Unicode del portapapeles, o cadena vacía.
pub fn clipboard_text(hwnd: isize) -> String {
    // SAFETY: consulta de formato sin punteros.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_err() {
        return String::new();
    }
    let owner = if hwnd != 0 {
        Some(HWND(hwnd as *mut c_void))
    } else {
        None
    };
    // SAFETY: el identificador de ventana es propio o nulo.
    if unsafe { OpenClipboard(owner) }.is_err() {
        return String::new();
    }
    let text = read_clipboard_text();
    // SAFETY: se cierra el portapapeles que se abrió justo antes.
    unsafe {
        let _ = CloseClipboard();
    }
    text
}

fn read_clipboard_text() -> String {
    // SAFETY: el portapapeles está abierto por este hilo y el formato existe.
    let handle = match unsafe { GetClipboardData(CF_UNICODETEXT) } {
        Ok(handle) => handle,
        Err(_) => return String::new(),
    };
    if handle.0.is_null() {
        return String::new();
    }
    let global = HGLOBAL(handle.0);
    // SAFETY: el bloque viene del portapapeles y sigue bloqueado hasta GlobalUnlock.
    let ptr = unsafe { GlobalLock(global) };
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: un CF_UNICODETEXT es una cadena UTF-16 terminada en nulo.
    let text = unsafe { wide_string(ptr as *const u16) };
    // SAFETY: se desbloquea el mismo bloque que se bloqueó.
    unsafe {
        let _ = GlobalUnlock(global);
    }
    text
}

/// Pone `text` en el portapapeles (Unicode). `false` si no se pudo.
pub fn copy_text(hwnd: isize, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let data = utf16_le_with_nul(text);
    // SAFETY: bloqueo de memoria global del tamaño pedido.
    let handle = match unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len()) } {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    // SAFETY: bloque recién asignado, aún no compartido.
    let ptr = unsafe { GlobalLock(handle) };
    if ptr.is_null() {
        // SAFETY: el bloque es nuestro y nadie más lo usa.
        unsafe {
            let _ = GlobalFree(Some(handle));
        }
        return false;
    }
    // SAFETY: `ptr` apunta a `data.len()` bytes movibles y bloqueados.
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        let _ = GlobalUnlock(handle);
    }
    let owner = if hwnd != 0 {
        Some(HWND(hwnd as *mut c_void))
    } else {
        None
    };
    // SAFETY: identificador propio o nulo.
    if unsafe { OpenClipboard(owner) }.is_err() {
        // SAFETY: el bloque sigue siendo nuestro.
        unsafe {
            let _ = GlobalFree(Some(handle));
        }
        return false;
    }
    // SAFETY: el portapapeles está abierto por este hilo.
    unsafe {
        let _ = EmptyClipboard();
    }
    // SAFETY: tras el éxito el portapapeles pasa a ser el dueño del bloque.
    let stored = unsafe { SetClipboardData(CF_UNICODETEXT, Some(HANDLE(handle.0))) }.is_ok();
    if !stored {
        // SAFETY: si no se aceptó, el bloque sigue siendo nuestro.
        unsafe {
            let _ = GlobalFree(Some(handle));
        }
    }
    // SAFETY: se cierra el portapapeles abierto arriba.
    unsafe {
        let _ = CloseClipboard();
    }
    stored
}

/// `(text + "\0").encode("utf-16-le")`.
pub(crate) fn utf16_le_with_nul(text: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(text.len() * 2 + 2);
    for unit in text.encode_utf16() {
        data.extend_from_slice(&unit.to_le_bytes());
    }
    data.extend_from_slice(&[0, 0]);
    data
}

/// `ctypes.wstring_at(ptr)`: cadena UTF-16 terminada en nulo.
///
/// # Safety
/// `ptr` debe apuntar a una cadena UTF-16 válida terminada en nulo.
// SAFETY: `unsafe fn` porque lee memoria cruda; el contrato está en la doc.
unsafe fn wide_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    // SAFETY: el contrato de la función garantiza el terminador en nulo.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: se leyó el terminador; el tramo es válido y de `len` elementos.
    let units = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_shortcut_needs_ctrl_and_v() {
        assert!(is_paste_shortcut(86, true));
        assert!(!is_paste_shortcut(86, false));
        assert!(!is_paste_shortcut(67, true));
    }

    #[test]
    fn clipboard_buffer_is_utf16le_with_a_terminator() {
        assert_eq!(utf16_le_with_nul("A"), vec![0x41, 0x00, 0x00, 0x00]);
        assert_eq!(utf16_le_with_nul("á"), vec![0xE1, 0x00, 0x00, 0x00]);
        assert_eq!(utf16_le_with_nul(""), vec![0x00, 0x00]);
    }
}
