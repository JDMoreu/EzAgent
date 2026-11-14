//! Port of `src/ezagent/voice/tray.py` (F5-16).
//!
//! Bandeja del sistema con `Shell_NotifyIconW` sobre una ventana solo de
//! mensajes. El hilo `ezagent-tray` no toca la UI: cada clic publica `config`,
//! `shape`, `restart` o `quit` por el callback de la aplicación.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetCursorPos, PeekMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, HMENU,
    HWND_MESSAGE, MENU_ITEM_FLAGS, MF_DEFAULT, MF_SEPARATOR, MF_STRING, MSG, PM_REMOVE,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_LBUTTONUP, WM_NULL, WM_QUIT, WM_RBUTTONUP, WNDCLASSW,
};

/// Lado del icono de la bandeja, igual que `ICON_SIZE` en Python.
pub const ICON_SIZE: u32 = 64;

/// Tooltip inicial (`_TITLE` en Python).
pub const TITLE: &str = "EzAgent · activo";

const MENU_CONFIG: usize = 1;
const MENU_SHAPE: usize = 2;
const MENU_RESTART: usize = 3;
const MENU_QUIT: usize = 4;
const MENU_ENTRIES: [(usize, &str); 4] = [
    (MENU_CONFIG, "Configurar"),
    (MENU_SHAPE, "Cambiar forma (círculo / píldora)"),
    (MENU_RESTART, "Reiniciar EzAgent"),
    (MENU_QUIT, "Cerrar"),
];

const TRAY_ID: u32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;
const TRAY_SET_TITLE: u32 = WM_APP + 2;
const INK: [u8; 3] = [0x07, 0x16, 0x1B];
const INK_TOP: [u8; 3] = [0x0D, 0x2A, 0x31];
const MINT: [u8; 3] = [0x1F, 0xAE, 0x9F];
const WINDOW_CLASS: PCWSTR = w!("EzAgentTrayWindow");

mod icon;
mod image;

use icon::icon_from_rgba;

pub use image::{load_icon_image, IconImage};

/// Bandeja del sistema con Configurar / Reiniciar / Cerrar.
pub struct TrayIcon {
    on_action: Arc<dyn Fn(String) + Send + Sync>,
    image_path: Option<PathBuf>,
    title: Arc<Mutex<String>>,
    shared: Arc<Shared>,
}

impl TrayIcon {
    pub fn new(
        on_action: impl Fn(String) + Send + Sync + 'static,
        image_path: Option<PathBuf>,
    ) -> Self {
        Self {
            on_action: Arc::new(on_action),
            image_path,
            title: Arc::new(Mutex::new(TITLE.to_string())),
            shared: Arc::new(Shared::default()),
        }
    }

    /// Tooltip distinto del inicial, como el `title=` del constructor Python.
    pub fn with_title(self, title: impl Into<String>) -> Self {
        *self.title.lock() = title.into();
        self
    }

    /// En Windows `Shell_NotifyIconW` siempre existe.
    pub fn available(&self) -> bool {
        true
    }

    pub fn running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// Crea la ventana y el icono en el hilo `ezagent-tray`; `true` si el hilo
    /// arrancó (igual que Python, que no espera al registro de Win32).
    pub fn start(&self) -> bool {
        if self.running() {
            return true;
        }
        let image = load_icon_image(self.image_path.as_deref());
        let shared = Arc::clone(&self.shared);
        let title = Arc::clone(&self.title);
        let on_action = Arc::clone(&self.on_action);
        shared.stopping.store(false, Ordering::Release);
        shared.running.store(true, Ordering::Release);
        let spawned = thread::Builder::new()
            .name("ezagent-tray".to_string())
            .spawn(move || run_tray(image, on_action, title, shared))
            .is_ok();
        if !spawned {
            self.shared.running.store(false, Ordering::Release);
        }
        spawned
    }

    /// Cambia el tooltip sin bloquear si la bandeja aún no existe.
    pub fn set_title(&self, title: &str) {
        *self.title.lock() = title.to_string();
        let hwnd = *self.shared.hwnd.lock();
        if let Some(hwnd) = hwnd {
            // SAFETY: el HWND lo creó el hilo de la bandeja y solo se encola un aviso.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    TRAY_SET_TITLE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }

    pub fn stop(&self) {
        self.shared.stopping.store(true, Ordering::Release);
        let hwnd = self.shared.hwnd.lock().take();
        self.shared.running.store(false, Ordering::Release);
        if let Some(hwnd) = hwnd {
            // SAFETY: WM_CLOSE lo procesa la ventana solo de mensajes de este módulo.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
struct Shared {
    hwnd: Mutex<Option<isize>>,
    running: AtomicBool,
    stopping: AtomicBool,
}

struct TrayState {
    hwnd: HWND,
    menu: HMENU,
    icon: HICON,
    on_action: Arc<dyn Fn(String) + Send + Sync>,
    title: Arc<Mutex<String>>,
}

thread_local! {
    // La ventana solo de mensajes y su estado viven en el hilo `ezagent-tray`.
    static TRAY_STATE: RefCell<Option<TrayState>> = const { RefCell::new(None) };
}

fn run_tray(
    image: IconImage,
    on_action: Arc<dyn Fn(String) + Send + Sync>,
    title: Arc<Mutex<String>>,
    shared: Arc<Shared>,
) {
    let hwnd = setup(image, on_action, title);
    let mut published = None;
    let started = match hwnd {
        Some(hwnd) if add_icon() => {
            let id = hwnd.0 as isize;
            *shared.hwnd.lock() = Some(id);
            published = Some(id);
            true
        }
        _ => false,
    };
    if !started {
        let slot = shared.hwnd.lock();
        if slot.is_none() {
            shared.running.store(false, Ordering::Release);
        }
    }
    if started && !shared.stopping.load(Ordering::Acquire) {
        message_loop(&shared.stopping);
    }
    remove_icon();
    if let Some(state) = TRAY_STATE.with(|cell| cell.borrow_mut().take()) {
        // SAFETY: los recursos pertenecen a este hilo y ya no se usan.
        unsafe {
            if !state.menu.is_invalid() {
                let _ = DestroyMenu(state.menu);
            }
            if !state.icon.is_invalid() {
                let _ = DestroyIcon(state.icon);
            }
            let _ = DestroyWindow(state.hwnd);
        }
    }
    let mut slot = shared.hwnd.lock();
    if let Some(published) = published {
        if *slot == Some(published) {
            slot.take();
            shared.running.store(false, Ordering::Release);
        }
    }
}

fn setup(
    image: IconImage,
    on_action: Arc<dyn Fn(String) + Send + Sync>,
    title: Arc<Mutex<String>>,
) -> Option<HWND> {
    let hwnd = create_window()?;
    TRAY_STATE.with(|cell| {
        *cell.borrow_mut() = Some(TrayState {
            hwnd,
            menu: HMENU::default(),
            icon: HICON::default(),
            on_action,
            title,
        });
    });
    let menu = build_menu()?;
    with_state_mut(|state| state.menu = menu);
    let icon = icon_from_rgba(&image)?;
    with_state_mut(|state| state.icon = icon);
    Some(hwnd)
}

fn create_window() -> Option<HWND> {
    // SAFETY: el nombre del módulo es estático y el proceso sigue vivo.
    let instance = unsafe { GetModuleHandleW(PCWSTR::null()).ok()? };
    let instance = HINSTANCE(instance.0);
    if !register_class(instance) {
        return None;
    }
    // SAFETY: la clase está registrada en este hilo; `HWND_MESSAGE` crea una
    // ventana solo de mensajes, sin UI visible.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WINDOW_CLASS,
            w!("EzAgent"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
        .ok()
    }
}

fn register_class(instance: HINSTANCE) -> bool {
    static CLASS_ATOM: AtomicU16 = AtomicU16::new(0);
    if CLASS_ATOM.load(Ordering::Acquire) != 0 {
        return true;
    }
    let class = WNDCLASSW {
        hInstance: instance,
        lpfnWndProc: Some(wndproc),
        lpszClassName: WINDOW_CLASS,
        ..Default::default()
    };
    // SAFETY: `class` y el nombre estático viven durante la llamada.
    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        return false;
    }
    CLASS_ATOM.store(atom, Ordering::Release);
    true
}

fn build_menu() -> Option<HMENU> {
    // SAFETY: solo se crea y rellena un menú propiedad de este hilo.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for (index, (id, label)) in MENU_ENTRIES.iter().enumerate() {
            if index == 3 && AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).is_err() {
                let _ = DestroyMenu(menu);
                return None;
            }
            let flags = if *id == MENU_CONFIG {
                MENU_ITEM_FLAGS(MF_STRING.0 | MF_DEFAULT.0)
            } else {
                MF_STRING
            };
            let text = HSTRING::from(*label);
            if AppendMenuW(menu, flags, *id, &text).is_err() {
                let _ = DestroyMenu(menu);
                return None;
            }
        }
        Some(menu)
    }
}

fn menu_action(command: usize) -> Option<&'static str> {
    match command {
        MENU_CONFIG => Some("config"),
        MENU_SHAPE => Some("shape"),
        MENU_RESTART => Some("restart"),
        MENU_QUIT => Some("quit"),
        _ => None,
    }
}

fn with_state_mut(action: impl FnOnce(&mut TrayState)) {
    TRAY_STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            action(state);
        }
    });
}

fn dispatch(action: &str) {
    let callback = TRAY_STATE.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|state| Arc::clone(&state.on_action))
    });
    if let Some(callback) = callback {
        // Un pánico del callback no debe cruzar la frontera FFI.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(action.to_string())
        }));
    }
}

fn show_menu() {
    let command = TRAY_STATE.with(|cell| {
        let borrowed = cell.borrow();
        let state = borrowed.as_ref()?;
        if state.menu.is_invalid() {
            return None;
        }
        // SAFETY: el menú y la ventana son de este hilo; `POINT` lo llena la API.
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            let _ = SetForegroundWindow(state.hwnd);
            let picked = TrackPopupMenu(
                state.menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                state.hwnd,
                None,
            );
            let _ = PostMessageW(Some(state.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let picked = picked.0 as usize;
            (picked != 0).then_some(picked)
        }
    });
    if let Some(action) = command.and_then(menu_action) {
        dispatch(action);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        TRAY_CALLBACK => {
            match lparam.0 as u32 {
                WM_LBUTTONUP => dispatch("config"),
                WM_RBUTTONUP => show_menu(),
                _ => {}
            }
            LRESULT(0)
        }
        TRAY_SET_TITLE => {
            update_tooltip();
            LRESULT(0)
        }
        WM_CLOSE => {
            remove_icon();
            // SAFETY: `hwnd` es la ventana solo de mensajes de este hilo.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: termina el bucle de mensajes de este hilo.
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => {
            // SAFETY: comportamiento por omisión para los mensajes que no atendemos.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

fn message_loop(stopping: &AtomicBool) {
    let mut message = MSG::default();
    while !stopping.load(Ordering::Acquire) {
        // SAFETY: `message` es válido y la cola pertenece a este hilo.
        let received = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) };
        if !received.as_bool() {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        if message.message == WM_QUIT {
            break;
        }
        // SAFETY: `message` lo llenó PeekMessageW en este mismo hilo.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn notify_data(state: &TrayState) -> NOTIFYICONDATAW {
    let title = state.title.lock();
    let mut tip = [0u16; 128];
    fill_tip(&mut tip, &title);
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: state.hwnd,
        uID: TRAY_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        hIcon: state.icon,
        szTip: tip,
        ..Default::default()
    }
}

fn fill_tip(buffer: &mut [u16; 128], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().take(buffer.len() - 1).collect();
    buffer[..encoded.len()].copy_from_slice(&encoded);
}

fn add_icon() -> bool {
    TRAY_STATE.with(|cell| {
        let borrowed = cell.borrow();
        let Some(state) = borrowed.as_ref() else {
            return false;
        };
        let data = notify_data(state);
        // SAFETY: `data` es una estructura válida para la llamada.
        unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() }
    })
}

fn remove_icon() {
    TRAY_STATE.with(|cell| {
        let borrowed = cell.borrow();
        if let Some(state) = borrowed.as_ref() {
            let data = notify_data(state);
            // SAFETY: `data` es válida; borrar un icono ausente solo devuelve falso.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
        }
    });
}

fn update_tooltip() {
    TRAY_STATE.with(|cell| {
        let borrowed = cell.borrow();
        if let Some(state) = borrowed.as_ref() {
            let data = notify_data(state);
            // SAFETY: `data` es válida y describe el icono ya registrado.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_title_matches_python_and_tip_truncates() {
        assert_eq!(TITLE, "EzAgent · activo");
        let mut tip = [0u16; 128];
        fill_tip(&mut tip, &"x".repeat(200));
        assert_eq!(tip[126], u16::from(b'x'));
        assert_eq!(tip[127], 0);
        let mut short = [0u16; 128];
        fill_tip(&mut short, TITLE);
        let chars = TITLE.encode_utf16().count();
        let decoded = String::from_utf16(&short[..chars]).expect("utf16");
        assert_eq!(decoded, TITLE);
    }

    #[test]
    fn tray_starts_stopped_and_available() {
        let tray = TrayIcon::new(|_| {}, None);
        assert!(tray.available());
        assert!(!tray.running());
        tray.set_title("EzAgent · en espera");
        tray.stop();
        assert!(!tray.running());
    }
}
