//! Port of `src/ezagent/voice/hotkeys.py` (F5-16).
//!
//! Atajos globales de Windows con `RegisterHotKey`, sin dependencias externas.
//! Un hilo registra la tecla y bombea `WM_HOTKEY`; el resto de la app consume
//! los eventos por un canal `crossbeam-channel`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use ez_core::pyvalue::{py_strip, python_repr};
use parking_lot::Mutex;
use serde_json::Value;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PeekMessageW, MSG, PM_NOREMOVE, PM_REMOVE, WM_HOTKEY,
};

const MOD_ALT_VALUE: u32 = MOD_ALT.0;
const MOD_CONTROL_VALUE: u32 = MOD_CONTROL.0;
const MOD_SHIFT_VALUE: u32 = MOD_SHIFT.0;
const MOD_WIN_VALUE: u32 = MOD_WIN.0;
const MOD_NOREPEAT_VALUE: u32 = MOD_NOREPEAT.0;

/// Código virtual de una tecla nombrada, o `None` si no está en el mapa.
fn key_code(name: &str) -> Option<u32> {
    match name {
        "space" => Some(0x20),
        "enter" => Some(0x0D),
        "esc" | "escape" => Some(0x1B),
        "tab" => Some(0x09),
        "backspace" => Some(0x08),
        "delete" => Some(0x2E),
        "insert" => Some(0x2D),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "left" => Some(0x25),
        "right" => Some(0x27),
        _ => {
            if let Some(number) = name.strip_prefix('f') {
                if let Ok(value) = number.parse::<u32>() {
                    if (1..=24).contains(&value) && number == value.to_string() {
                        return Some(0x6F + value);
                    }
                }
            }
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(letter), None) if letter.is_ascii_lowercase() => {
                    Some(u32::from(letter.to_ascii_uppercase()))
                }
                (Some(digit), None) if digit.is_ascii_digit() => Some(u32::from(digit)),
                _ => None,
            }
        }
    }
}

/// `parse_hotkey`: devuelve `(modificadores, tecla)` o el mismo texto de error
/// que el `ValueError` de Python.
pub fn parse_hotkey(spec: &str) -> Result<(u32, u32), String> {
    let mut parts = Vec::new();
    for part in spec.split('+') {
        let trimmed = py_strip(part);
        if trimmed.is_empty() {
            continue;
        }
        parts.push(trimmed.to_lowercase());
    }
    if parts.is_empty() {
        return Err("Hotkey vacío.".to_string());
    }
    let mut modifiers = 0u32;
    let mut key = None;
    for part in parts {
        match part.as_str() {
            "alt" | "option" => modifiers |= MOD_ALT_VALUE,
            "ctrl" | "control" => modifiers |= MOD_CONTROL_VALUE,
            "shift" => modifiers |= MOD_SHIFT_VALUE,
            "win" | "windows" | "super" => modifiers |= MOD_WIN_VALUE,
            _ => match key_code(&part) {
                Some(code) => key = Some(code),
                None => {
                    let shown = python_repr(&Value::String(part));
                    return Err(format!("Tecla no reconocida en hotkey: {shown}"));
                }
            },
        }
    }
    match key {
        Some(key) => Ok((modifiers, key)),
        None => {
            let shown = python_repr(&Value::String(spec.to_string()));
            Err(format!("Falta la tecla final en hotkey: {shown}"))
        }
    }
}

/// Atajo global: registra `spec` y publica `event` por `events` en cada pulsación.
pub struct GlobalHotkey {
    pub spec: String,
    pub event: String,
    pub events: Sender<String>,
    pub hk_id: i32,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl GlobalHotkey {
    pub fn new(
        spec: impl Into<String>,
        event: impl Into<String>,
        events: Sender<String>,
        hk_id: i32,
    ) -> Self {
        Self {
            spec: spec.into(),
            event: event.into(),
            events,
            hk_id,
            stop: Arc::new(AtomicBool::new(false)),
            error: Arc::new(Mutex::new(None)),
            thread: Mutex::new(None),
        }
    }

    /// Arranca el hilo que registra la tecla. Como en Python, un `spec` inválido
    /// deja el error en [`GlobalHotkey::error`] sin lanzar nada.
    pub fn start(&self) {
        let spec = self.spec.clone();
        let event = self.event.clone();
        let events = self.events.clone();
        let stop = Arc::clone(&self.stop);
        let error = Arc::clone(&self.error);
        let hk_id = self.hk_id;
        let handle = std::thread::spawn(move || run(spec, event, events, hk_id, stop, error));
        *self.thread.lock() = Some(handle);
    }

    /// Suelta la tecla. Con `wait` espera hasta un segundo, igual que Python.
    pub fn stop(&self, wait: bool) {
        self.stop.store(true, Ordering::Release);
        if !wait {
            return;
        }
        let handle = self.thread.lock().take();
        if let Some(handle) = handle {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }

    /// Texto del último fallo de registro, si lo hubo.
    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }
}

fn run(
    spec: String,
    event: String,
    events: Sender<String>,
    hk_id: i32,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    let (modifiers, key) = match parse_hotkey(&spec) {
        Ok(parsed) => parsed,
        Err(message) => {
            *error.lock() = Some(message);
            return;
        }
    };
    // La cola de mensajes del hilo debe existir antes de registrar.
    // SAFETY: `probe` es un MSG válido de esta pila; la llamada solo crea la cola
    // del hilo cuando aún no existe.
    let registered = unsafe {
        let mut probe = MSG::default();
        let _ = PeekMessageW(&mut probe, None, 0, 0, PM_NOREMOVE);
        RegisterHotKey(
            None,
            hk_id,
            HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT_VALUE),
            key,
        )
        .is_ok()
    };
    if !registered {
        *error.lock() = Some(format!("No se pudo registrar {spec} (¿lo usa otra app?)"));
        return;
    }
    let mut message = MSG::default();
    while !stop.load(Ordering::Acquire) {
        // SAFETY: `message` es un MSG válido de esta pila y el hilo es su dueño.
        let received = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) };
        if received.as_bool() {
            if message.message == WM_HOTKEY && message.wParam.0 as i32 == hk_id {
                let _ = events.send(event.clone());
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    // SAFETY: el registro pertenece a este hilo y se libera antes de terminar.
    unsafe {
        let _ = UnregisterHotKey(None, hk_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[test]
    fn parses_modifiers_and_letters_case_insensitively() {
        assert_eq!(parse_hotkey("ctrl+alt+e"), Ok((0x0003, 0x45)));
        assert_eq!(parse_hotkey(" Ctrl + Alt + Space "), Ok((0x0003, 0x20)));
        assert_eq!(parse_hotkey("SHIFT+z"), Ok((0x0004, 0x5A)));
        assert_eq!(parse_hotkey("0"), Ok((0, 0x30)));
    }

    #[test]
    fn accepts_aliases_and_function_keys() {
        assert_eq!(parse_hotkey("option+control+f1"), Ok((0x0003, 0x70)));
        assert_eq!(parse_hotkey("windows+super+win+f24"), Ok((0x0008, 0x87)));
        assert_eq!(parse_hotkey("escape"), Ok((0, 0x1B)));
        assert_eq!(parse_hotkey("esc"), Ok((0, 0x1B)));
        assert_eq!(parse_hotkey("pageup"), Ok((0, 0x21)));
    }

    #[test]
    fn rejects_empty_and_unknown_parts_with_python_texts() {
        assert_eq!(parse_hotkey(""), Err("Hotkey vacío.".to_string()));
        assert_eq!(parse_hotkey(" + + "), Err("Hotkey vacío.".to_string()));
        assert_eq!(
            parse_hotkey("ctrl+alt+ñ"),
            Err("Tecla no reconocida en hotkey: 'ñ'".to_string())
        );
        assert_eq!(
            parse_hotkey("f25"),
            Err("Tecla no reconocida en hotkey: 'f25'".to_string())
        );
        assert_eq!(
            parse_hotkey("ctrl+alt"),
            Err("Falta la tecla final en hotkey: 'ctrl+alt'".to_string())
        );
    }

    #[test]
    fn reports_invalid_spec_from_the_worker_thread() {
        let (events, _receiver) = unbounded();
        let hotkey = GlobalHotkey::new("ctrl+alt", "toggle", events, 47201);
        hotkey.start();
        let mut seen = None;
        for _ in 0..100 {
            seen = hotkey.error();
            if seen.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        hotkey.stop(true);
        assert_eq!(
            seen,
            Some("Falta la tecla final en hotkey: 'ctrl+alt'".to_string())
        );
    }

    #[test]
    fn stop_without_wait_does_not_block() {
        let (events, _receiver) = unbounded();
        let hotkey = GlobalHotkey::new("ctrl+alt+q", "cancel", events, 47202);
        hotkey.stop(false);
        assert!(hotkey.error().is_none());
    }
}
