//! Port of `src/ezagent/voice/hud/layered.py` — WndProc y mensajes (F5-01).
//!
//! Una sola clase (`ezagent-layered-window`) y un solo WndProc: cada mensaje se
//! despacha a la instancia dueña del HWND. Aquí viven también las constantes
//! `WM_*` y la traducción de mensajes a eventos del canal público.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Weak};

use parking_lot::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, LoadCursorW, RegisterClassExW, CREATESTRUCTW, CS_HREDRAW,
    CS_VREDRAW, HTTRANSPARENT, IDC_ARROW, MA_ACTIVATE, MA_NOACTIVATE, WM_CHAR, WM_CLOSE,
    WM_DPICHANGED, WM_DROPFILES, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCHITTEST, WM_PAINT,
    WM_RBUTTONUP, WM_SYSCHAR, WM_SYSKEYDOWN, WNDCLASSEXW,
};

use super::drag;
use super::keyboard;
use super::layered::{LayeredWindow, WindowEvent};
use super::pointer;

/// Nombre de la clase de ventana.
pub const CLASS_NAME: &str = "ezagent-layered-window";

/// `WM_NULL`: despierta `MsgWaitForMultipleObjects` sin efecto visible.
pub const WM_NULL: u32 = 0x0000;

/// `WM_MOUSELEAVE` (el crate `windows` no lo expone como constante).
pub const WM_MOUSELEAVE: u32 = 0x02A3;

static WINDOWS: LazyLock<Mutex<BTreeMap<usize, Weak<LayeredWindow>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static BY_HWND: LazyLock<Mutex<BTreeMap<isize, Weak<LayeredWindow>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static CLASS_ATOM: AtomicUsize = AtomicUsize::new(0);
static CLASS_LOCK: Mutex<()> = Mutex::new(());

/// `GetModuleHandleW(None)`: instancia del proceso.
pub(crate) fn instance() -> HINSTANCE {
    // SAFETY: `PCWSTR::null()` pide el módulo del proceso actual.
    match unsafe { GetModuleHandleW(PCWSTR::null()) } {
        Ok(module) => HINSTANCE(module.0),
        Err(_) => HINSTANCE::default(),
    }
}

/// Registra una ventana por su id antes de `CreateWindowExW`.
pub(crate) fn register(wid: usize, window: &Arc<LayeredWindow>) {
    WINDOWS.lock().insert(wid, Arc::downgrade(window));
}

/// Da de baja la ventana al terminar el bucle.
pub(crate) fn unregister(wid: usize, hwnd: isize) {
    WINDOWS.lock().remove(&wid);
    BY_HWND.lock().remove(&hwnd);
}

/// Fija el HWND de la ventana y lo indexa para el despacho de mensajes.
pub(crate) fn attach(wid: usize, hwnd: HWND) {
    let window = WINDOWS.lock().get(&wid).and_then(Weak::upgrade);
    if let Some(window) = window {
        window.set_hwnd(hwnd.0 as isize);
        BY_HWND
            .lock()
            .insert(hwnd.0 as isize, Arc::downgrade(&window));
    }
}

/// `_ensure_class`: registra la clase una sola vez por proceso.
pub fn ensure_class() -> bool {
    let _guard = CLASS_LOCK.lock();
    if CLASS_ATOM.load(Ordering::SeqCst) != 0 {
        return true;
    }
    let cursor = {
        // SAFETY: IDC_ARROW es un recurso del sistema válido.
        unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default()
    };
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance(),
        hCursor: cursor,
        lpszClassName: w!("ezagent-layered-window"),
        ..Default::default()
    };
    // SAFETY: `class` es local, vive durante la llamada y sus campos son válidos.
    let atom = unsafe { RegisterClassExW(&class) };
    if atom != 0 {
        CLASS_ATOM.store(atom as usize, Ordering::SeqCst);
        return true;
    }
    // SAFETY: consulta del último error del hilo.
    if unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS {
        CLASS_ATOM.store(1, Ordering::SeqCst);
        return true;
    }
    false
}

/// WndProc único de la clase; despacha a la instancia dueña del HWND.
// SAFETY: la firma `unsafe extern "system"` es la que exige `WNDPROC`; Windows
// garantiza que los punteros del mensaje son válidos durante la llamada.
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: en WM_NCCREATE `lparam` apunta a un CREATESTRUCTW válido.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        attach(create.lpCreateParams as usize, hwnd);
    } else {
        let window = BY_HWND
            .lock()
            .get(&(hwnd.0 as isize))
            .and_then(Weak::upgrade);
        if let Some(window) = window {
            if let Some(result) = dispatch(&window, msg, wparam, lparam) {
                return result;
            }
        }
    }
    // SAFETY: mensajes no atendidos van al procedimiento por defecto.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// `_on_message`: `None` deja que responda `DefWindowProcW`.
pub(crate) fn dispatch(
    window: &Arc<LayeredWindow>,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    let keyboard_open = window.state.lock().keyboard;
    match msg {
        WM_MOUSEACTIVATE => Some(LRESULT(if keyboard_open {
            MA_ACTIVATE as isize
        } else {
            MA_NOACTIVATE as isize
        })),
        WM_NCHITTEST if !window.click_through => {
            let (cx, cy) = super::layered::screen_cursor();
            if window.alpha_at(cx - window.x(), cy - window.y()) == 0 {
                Some(LRESULT(HTTRANSPARENT as isize))
            } else {
                Some(LRESULT(1))
            }
        }
        WM_PAINT => Some(LRESULT(0)),
        WM_DPICHANGED => Some(handle_dpi_changed(window, wparam)),
        WM_SYSCHAR if keyboard_open => Some(LRESULT(0)),
        WM_KEYDOWN | WM_SYSKEYDOWN if keyboard_open => handle_key(window, wparam),
        WM_CHAR if keyboard_open => handle_char(window, wparam),
        WM_KILLFOCUS if keyboard_open => {
            window.emit(WindowEvent::FocusLost);
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL if !window.click_through => handle_wheel(window, wparam, lparam),
        WM_DROPFILES => Some(handle_drop(window, wparam)),
        WM_MOUSEMOVE => Some(handle_mouse_move(window, lparam)),
        WM_MOUSELEAVE => Some(handle_mouse_leave(window)),
        WM_LBUTTONDOWN if !window.click_through => Some(handle_lbutton_down(window, lparam)),
        WM_LBUTTONUP if !window.click_through => Some(handle_lbutton_up(window, lparam)),
        WM_RBUTTONUP if !window.click_through => {
            window.emit(WindowEvent::RightClick);
            Some(LRESULT(0))
        }
        WM_CLOSE => {
            window.request_stop();
            Some(LRESULT(0))
        }
        _ => None,
    }
}

fn handle_dpi_changed(window: &Arc<LayeredWindow>, wparam: WPARAM) -> LRESULT {
    let dpi = super::dpi::dpi_from_wparam(wparam.0);
    window.emit(WindowEvent::DpiChanged(dpi));
    LRESULT(0)
}

fn handle_key(window: &Arc<LayeredWindow>, wparam: WPARAM) -> Option<LRESULT> {
    let vk = (wparam.0 & 0xFFFF) as u32;
    let ctrl = keyboard::ctrl_down();
    if keyboard::is_paste_shortcut(vk, ctrl)
        && window.on_paste(&keyboard::clipboard_text(window.hwnd()))
    {
        return Some(LRESULT(0));
    }
    if window.on_key(vk, ctrl) {
        Some(LRESULT(0))
    } else {
        None
    }
}

fn handle_char(window: &Arc<LayeredWindow>, wparam: WPARAM) -> Option<LRESULT> {
    // DECISION: el Python usa chr(), que acepta sustitutos sueltos; `char` de
    // Rust no, así que un sustituto suelto se ignora (no es texto escribible).
    if let Some(character) = char::from_u32((wparam.0 & 0xFFFF) as u32) {
        if window.on_char(character) {
            return Some(LRESULT(0));
        }
    }
    None
}

fn handle_wheel(window: &Arc<LayeredWindow>, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    let (sx, sy) = split_signed(lparam.0);
    let (x, y) = (sx - window.x(), sy - window.y());
    let handled = window.on_wheel(x, y, delta);
    window.emit(WindowEvent::Wheel { x, y, delta });
    if handled {
        Some(LRESULT(0))
    } else {
        None
    }
}

fn handle_drop(window: &Arc<LayeredWindow>, wparam: WPARAM) -> LRESULT {
    let drop = HDROP(wparam.0 as *mut c_void);
    // SAFETY: `drop` viene de WM_DROPFILES y solo se usa en este mensaje.
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    let mut paths = Vec::new();
    for index in 0..count {
        // SAFETY: índice válido dentro del mismo HDROP.
        let length = unsafe { DragQueryFileW(drop, index, None) };
        let mut buffer = vec![0u16; length as usize + 1];
        // SAFETY: el búfer tiene capacidad para la ruta y su terminador.
        unsafe {
            DragQueryFileW(drop, index, Some(&mut buffer));
        }
        if let Some(end) = buffer.iter().position(|unit| *unit == 0) {
            buffer.truncate(end);
        }
        paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
    }
    // SAFETY: se libera el HDROP una sola vez, al terminar de leerlo.
    unsafe { DragFinish(drop) };
    if !paths.is_empty() {
        window.emit(WindowEvent::Files(paths));
    }
    LRESULT(0)
}

fn handle_mouse_leave(window: &Arc<LayeredWindow>) -> LRESULT {
    window.state.lock().tracking = false;
    let pointer_active = window.state.lock().pointer_active;
    if !pointer_active && window.set_hover(None) {
        window.emit(WindowEvent::Hover(None));
    }
    LRESULT(0)
}

fn handle_mouse_move(window: &Arc<LayeredWindow>, lparam: LPARAM) -> LRESULT {
    let (x, y) = split_signed(lparam.0);
    if window.state.lock().pointer_active {
        window.on_pointer_move(x, y);
        let ident = window.cursor_ident(x, y);
        if ident != 0 {
            pointer::set_cursor(false, ident);
        }
        return LRESULT(0);
    }
    if window.state.lock().dragging && !drag::lbutton_down() {
        // Se perdió el WM_LBUTTONUP (lo consumió otro proceso): sin esto el
        // arrastre se queda pegado y la ventana sigue al cursor sola.
        {
            let mut state = window.state.lock();
            state.dragging = false;
            state.drag_moved = false;
        }
        // SAFETY: liberar la captura de este hilo no tiene requisitos.
        unsafe {
            let _ = ReleaseCapture();
        }
        window.emit(WindowEvent::DragEnd {
            x: window.x(),
            y: window.y(),
        });
    }
    if window.state.lock().dragging {
        drag_follow(window);
    } else if !window.click_through {
        track_mouse(window);
        let hand = window.cursor_hand(x, y);
        let ident = window.cursor_ident(x, y);
        pointer::set_cursor(hand, ident);
        let action = window.hit_action(x, y);
        if window.set_hover(action.clone()) {
            window.emit(WindowEvent::Hover(action));
        }
    }
    LRESULT(0)
}

fn drag_follow(window: &Arc<LayeredWindow>) {
    let (sx, sy) = super::layered::screen_cursor();
    let offset = window.state.lock().drag_off;
    let target = drag::drag_target((sx, sy), offset);
    if drag::drag_moved(target, (window.x(), window.y())) {
        window.state.lock().drag_moved = true;
    }
    let (nx, ny) = window.clamp_position(target.0, target.1);
    window.set_pos(nx, ny);
    if window.state.lock().drag_moved {
        window.emit(WindowEvent::DragMove {
            x: window.x(),
            y: window.y(),
        });
    }
}

fn handle_lbutton_down(window: &Arc<LayeredWindow>, lparam: LPARAM) -> LRESULT {
    let (x, y) = split_signed(lparam.0);
    if window.alpha_at(x, y) > 0 {
        if window.on_pointer_down(x, y) {
            window.state.lock().pointer_active = true;
            // SAFETY: capturar el ratón para una ventana propia es válido.
            unsafe {
                SetCapture(window.hwnd_raw());
            }
            return LRESULT(0);
        }
        let (sx, sy) = super::layered::screen_cursor();
        {
            let mut state = window.state.lock();
            state.dragging = true;
            state.drag_moved = false;
            state.drag_off = drag::drag_offset((sx, sy), (state.x, state.y));
        }
        // SAFETY: capturar el ratón para una ventana propia es válido.
        unsafe {
            SetCapture(window.hwnd_raw());
        }
        window.emit(WindowEvent::DragStart {
            x: window.x(),
            y: window.y(),
        });
    }
    LRESULT(0)
}

fn handle_lbutton_up(window: &Arc<LayeredWindow>, lparam: LPARAM) -> LRESULT {
    let (x, y) = split_signed(lparam.0);
    if window.state.lock().pointer_active {
        window.state.lock().pointer_active = false;
        window.on_pointer_up(x, y);
        // SAFETY: liberar la captura de este hilo no tiene requisitos.
        unsafe {
            let _ = ReleaseCapture();
        }
        return LRESULT(0);
    }
    let was_drag = window.state.lock().drag_moved;
    {
        let mut state = window.state.lock();
        state.dragging = false;
        state.drag_moved = false;
    }
    // SAFETY: liberar la captura de este hilo no tiene requisitos.
    unsafe {
        let _ = ReleaseCapture();
    }
    if was_drag {
        window.emit(WindowEvent::DragEnd {
            x: window.x(),
            y: window.y(),
        });
    } else if let Some(action) = window.hit_action(x, y) {
        window.emit(WindowEvent::Click(action));
    }
    LRESULT(0)
}

fn track_mouse(window: &Arc<LayeredWindow>) {
    if window.state.lock().tracking {
        return;
    }
    let hwnd = window.hwnd_raw();
    let mut event = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: `event` es local y la ventana está viva en su propio hilo.
    unsafe {
        let _ = TrackMouseEvent(&mut event);
    }
    window.state.lock().tracking = true;
}

/// Extrae dos enteros de 16 bits con signo (coordenadas de `lParam`).
pub(crate) fn split_signed(value: isize) -> (i32, i32) {
    let low = (value & 0xFFFF) as u16 as i16 as i32;
    let high = ((value >> 16) & 0xFFFF) as u16 as i16 as i32;
    (low, high)
}

/// Crea la ventana real (la llama el hilo de la ventana).
pub(crate) fn create_window(
    window: &Arc<LayeredWindow>,
    wid: usize,
    (x, y, width, height): (i32, i32, i32, i32),
    title: &str,
    click_through: bool,
) -> Option<HWND> {
    let mut style = windows::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED
        | windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW
        | windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST
        | windows::Win32::UI::WindowsAndMessaging::WS_EX_NOACTIVATE;
    if click_through {
        style |= windows::Win32::UI::WindowsAndMessaging::WS_EX_TRANSPARENT;
    }
    let title = windows::core::HSTRING::from(title);
    // SAFETY: la clase está registrada, el título vive durante la llamada y
    // `wid` viaja como lpCreateParams (entero, no puntero desreferenciado).
    let hwnd = unsafe {
        CreateWindowExW(
            style,
            w!("ezagent-layered-window"),
            &title,
            windows::Win32::UI::WindowsAndMessaging::WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance()),
            Some(wid as *const c_void),
        )
    };
    match hwnd {
        Ok(hwnd) => {
            attach(wid, hwnd);
            window.set_hwnd(hwnd.0 as isize);
            Some(hwnd)
        }
        Err(error) => {
            tracing::warn!("CreateWindowExW falló: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lparam_splits_into_signed_coordinates() {
        let packed = ((300_isize) << 16) | (-4_i32 as u16 as isize);
        assert_eq!(split_signed(packed), (-4, 300));
        assert_eq!(split_signed(0), (0, 0));
    }
}
