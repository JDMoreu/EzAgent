//! Port of `src/ezagent/voice/hud/layered.py` — DPI (F5-01).
//!
//! `enable_dpi_awareness` (Per-Monitor V2 con caída a `SetProcessDpiAwareness`)
//! y el escalado lógico → físico que usan las ventanas del HUD: 1 unidad lógica
//! es 1 px a 96 ppp.

use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
};

/// Puntos por pulgada de referencia de las medidas lógicas.
pub const BASE_DPI: u32 = 96;

/// Per-Monitor V2 antes de crear Tk/Win32 (spec 4.4/11.5).
///
/// Si el sistema no expone `SetProcessDpiAwarenessContext`, cae a
/// `SetProcessDpiAwareness`. Debe llamarse una sola vez, al arrancar.
/// Devuelve `true` si alguna de las dos llamadas funcionó (el Python ignora el
/// resultado; aquí sirve para registrarlo).
pub fn enable_dpi_awareness() -> bool {
    // SAFETY: llamada Win32 sin punteros propios; el contexto es una constante.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
    {
        return true;
    }
    // SAFETY: llamada Win32 sin punteros propios.
    unsafe { SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) }.is_ok()
}

/// Factor de escala del monitor (`dpi / 96`).
pub fn scale_for_dpi(dpi: u32) -> f64 {
    dpi as f64 / BASE_DPI as f64
}

/// Medida lógica en píxeles físicos del monitor.
pub fn scale_value(value: f64, dpi: u32) -> f64 {
    value * scale_for_dpi(dpi)
}

/// DPI que viaja en el `wParam` de `WM_DPICHANGED` (palabra baja).
pub fn dpi_from_wparam(wparam: usize) -> u32 {
    (wparam & 0xFFFF) as u32
}

/// DPI del sistema, 96 si Windows no lo informa.
pub fn system_dpi() -> u32 {
    // SAFETY: consulta Win32 sin parámetros.
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 {
        BASE_DPI
    } else {
        dpi
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_units_scale_with_the_monitor_dpi() {
        assert_eq!(scale_for_dpi(96), 1.0);
        assert_eq!(scale_for_dpi(120), 1.25);
        assert_eq!(scale_for_dpi(144), 1.5);
        assert_eq!(scale_for_dpi(192), 2.0);
        assert_eq!(scale_value(18.0, 144), 27.0);
        assert_eq!(scale_value(10.0, 96), 10.0);
    }

    #[test]
    fn dpi_changed_uses_the_low_word() {
        assert_eq!(dpi_from_wparam(144), 144);
        assert_eq!(dpi_from_wparam(0x0090_0120), 0x0120);
    }
}
