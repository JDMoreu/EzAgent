//! Port of `src/ezagent/voice/hud/layered.py` — rueda del ratón (F5-01).
//!
//! Windows puede entregar `WM_MOUSEWHEEL` a la ventana bajo el puntero (valor
//! `MouseWheelRouting = 2`, el de fábrica en Windows 10/11) y entonces no hace
//! falta un gancho global de ratón. Ese gancho (`WH_MOUSE_LL`) obliga a Windows
//! a esperar a un hilo en CADA movimiento del sistema; era el *input lag* al
//! abrir el menú de la barra.

use std::ffi::c_void;
use std::sync::{Arc, LazyLock, Weak};

use parking_lot::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    RRF_RT_REG_DWORD,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_MOUSEWHEEL,
};

use super::layered::LayeredWindow;

/// Ventanas con gancho global instalado (el gancho no identifica su dueño).
static HOOK_OWNERS: LazyLock<Mutex<Vec<Weak<LayeredWindow>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// ¿Windows entrega la rueda a la ventana bajo el puntero, no a la activa?
pub fn wheel_follows_pointer() -> bool {
    // DECISION: el único objetivo soportado es Windows 10/11, así que el
    // chequeo `sys.getwindowsversion().major >= 10` del Python es siempre cierto.
    follows(wheel_routing(), registry_readable())
}

/// Regla del Python con los datos ya leídos (pura, para pruebas).
fn follows(routing: Option<i32>, registry_readable: bool) -> bool {
    match routing {
        // Sin el valor, Windows 10 usa su valor de fábrica: rueda bajo el puntero.
        None => registry_readable,
        Some(value) => value == 2,
    }
}

/// Valor de `MouseWheelRouting` (0/1/2), o `None` si no existe.
fn wheel_routing() -> Option<i32> {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `value` y `size` son locales y del tipo que declara RRF_RT_REG_DWORD.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\Desktop"),
            w!("MouseWheelRouting"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    if status == ERROR_SUCCESS {
        Some(value as i32)
    } else {
        None
    }
}

/// ¿Se puede leer la clave `Control Panel\Desktop`?
fn registry_readable() -> bool {
    let mut key = HKEY::default();
    // SAFETY: `key` es local; si la apertura falla no se cierra nada.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\Desktop"),
            Some(0),
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return false;
    }
    // SAFETY: la clave se abrió justo arriba y no se usa después.
    unsafe {
        let _ = RegCloseKey(key);
    }
    true
}

/// Instala el gancho global de rueda para esta ventana.
pub(crate) fn install_mouse_hook(window: &Arc<LayeredWindow>) {
    if window.mouse_hook() != 0 {
        return;
    }
    // SAFETY: el procedimiento es un `extern "system"` estático con la firma
    // HOOKPROC; el módulo propio se usa para WH_MOUSE_LL global.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_hook_proc),
            Some(super::wndproc::instance()),
            0,
        )
    };
    match hook {
        Ok(hook) => {
            HOOK_OWNERS.lock().push(Arc::downgrade(window));
            window.set_mouse_hook(hook.0 as isize);
        }
        Err(error) => tracing::warn!("SetWindowsHookExW falló: {error}"),
    }
}

/// Quita el gancho global de rueda de esta ventana.
pub(crate) fn uninstall_mouse_hook(window: &LayeredWindow) {
    HOOK_OWNERS
        .lock()
        .retain(|owner| !std::ptr::eq(owner.as_ptr(), window));
    let hook = window.take_mouse_hook();
    if hook != 0 {
        // SAFETY: `hook` lo devolvió SetWindowsHookExW y no se libera dos veces.
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(hook as *mut c_void));
        }
    }
}

/// Callback `WH_MOUSE_LL`: entrega la rueda a las ventanas interesadas.
// SAFETY: la firma `unsafe extern "system"` es la que exige `HOOKPROC`.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // DX-1d: un pánico no puede cruzar la frontera FFI.
    let swallowed = ez_core::panics::catch_panic(|| {
        if code >= 0 && wparam.0 as u32 == WM_MOUSEWHEEL {
            // SAFETY: con code >= 0 y WH_MOUSE_LL, `lparam` apunta a MSLLHOOKSTRUCT.
            let data = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let delta = ((data.mouseData >> 16) & 0xFFFF) as u16 as i16;
            let owners: Vec<Arc<LayeredWindow>> = HOOK_OWNERS
                .lock()
                .iter()
                .filter_map(Weak::upgrade)
                .collect();
            for window in owners {
                if window.on_global_wheel(data.pt.x, data.pt.y, delta as i32) {
                    return true;
                }
            }
        }
        false
    })
    .unwrap_or(false);
    if swallowed {
        return LRESULT(1);
    }
    // SAFETY: reenvío estándar; devuelve lo que responda el siguiente gancho.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_wheel_routing_is_pointer_following() {
        assert!(follows(Some(2), false));
        assert!(!follows(Some(0), true));
        assert!(!follows(Some(1), true));
    }

    #[test]
    fn missing_value_keeps_the_old_behaviour_only_if_unreadable() {
        assert!(follows(None, true));
        assert!(!follows(None, false));
    }
}
