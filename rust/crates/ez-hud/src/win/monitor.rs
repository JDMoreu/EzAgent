//! Port of `src/ezagent/voice/hud/layered.py` — geometría del escritorio (F5-01).
//!
//! `work_area` (área de trabajo, sin la barra de tareas), `monitor_rect`
//! (`rcMonitor` del monitor bajo el cursor), `taskbar_rect` y `clamp_pos`, que
//! mantiene la ventana dentro de la pantalla. La lógica pura (elegir monitor a
//! partir de rectángulos y recortar posiciones) no toca Win32 y se prueba aquí.

use std::mem::size_of;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, GetWindowRect, SystemParametersInfoW, SPI_GETWORKAREA,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// `(left, top, right, bottom)` en píxeles físicos.
pub type Rect = (i32, i32, i32, i32);

/// Separación al borde del área de trabajo, en unidades lógicas.
pub const EDGE_MARGIN: i32 = 18;

/// Lo que devuelve el Python si la consulta de Win32 falla.
pub const DEFAULT_WORK_AREA: Rect = (0, 0, 1920, 1080);

/// Área de trabajo, ya sin la barra de tareas (`SPI_GETWORKAREA`).
///
/// El código anterior usaba `0x0048`, que no es esa constante: la llamada
/// fallaba y la muesca se colaba bajo la barra.
pub fn work_area() -> Rect {
    let mut rect = RECT::default();
    // SAFETY: `rect` es una estructura local viva durante la llamada y del tipo
    // que espera SPI_GETWORKAREA.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if result.is_ok() {
        (rect.left, rect.top, rect.right, rect.bottom)
    } else {
        DEFAULT_WORK_AREA
    }
}

/// `rcMonitor` (incluida la barra de tareas) del monitor bajo `(x, y)`.
///
/// La spec 4.1 pide `rcMonitor`, no `rcWork`: la barra cuelga del borde físico
/// superior. Sin coordenadas se usa el monitor bajo el cursor y
/// `MONITOR_DEFAULTTONEAREST` como respaldo.
pub fn monitor_rect(x: Option<i32>, y: Option<i32>) -> Rect {
    let (x, y) = match (x, y) {
        (Some(x), Some(y)) => (x, y),
        _ => {
            let mut point = windows::Win32::Foundation::POINT::default();
            // SAFETY: `point` es local y válido; si falla queda (0, 0).
            let _ = unsafe { GetCursorPos(&mut point) };
            (point.x, point.y)
        }
    };
    // SAFETY: llamada de consulta sin punteros propios.
    let monitor = unsafe {
        MonitorFromPoint(
            windows::Win32::Foundation::POINT { x, y },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` es local, vive durante la llamada y lleva su `cbSize`.
    if !monitor.0.is_null() && unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let rect = info.rcMonitor;
        (rect.left, rect.top, rect.right, rect.bottom)
    } else {
        work_area()
    }
}

/// Rectángulo de la barra de tareas, o `None`.
///
/// Hace falta porque con la barra auto-oculta `SPI_GETWORKAREA` devuelve la
/// pantalla COMPLETA: sin esto, la ventana se colocaría justo donde la barra
/// aparece al asomar el ratón.
pub fn taskbar_rect() -> Option<Rect> {
    // SAFETY: nombres constantes; el HWND devuelto se consulta de inmediato.
    let hwnd = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.ok()?;
    if hwnd.0.is_null() {
        return None;
    }
    let mut rect = RECT::default();
    // SAFETY: `rect` es local y válido durante la consulta.
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some((rect.left, rect.top, rect.right, rect.bottom))
}

/// Mantiene la ventana dentro del área de trabajo.
///
/// Sin esto, una posición guardada (o un arrastre) puede dejar la ventana fuera
/// de la pantalla o debajo de la barra de tareas, y el usuario se queda sin
/// poder recuperarla.
pub fn clamp_pos(x: i32, y: i32, size: i32) -> (i32, i32) {
    clamp_in(work_area(), taskbar_rect(), x, y, size)
}

/// `clamp_pos` sin Win32: área de trabajo y barra como datos.
pub fn clamp_in(work: Rect, taskbar: Option<Rect>, x: i32, y: i32, size: i32) -> (i32, i32) {
    let margin = (EDGE_MARGIN / 2).max(0);
    let low_x = work.0 + margin;
    let high_x = low_x.max(work.2 - size - margin);
    let x = x.max(low_x).min(high_x);
    let low_y = work.1 + margin;
    let high_y = low_y.max(work.3 - size - margin);
    let mut y = y.max(low_y).min(high_y);
    if let Some(taskbar) = taskbar {
        if taskbar.1 > work.1 + (work.3 - work.1).div_euclid(2) {
            y = y.min(taskbar.1 - size - margin); // por encima de la barra
        }
    }
    (x, y)
}

/// Monitor que contiene `(x, y)` o, si ninguno lo contiene, el más cercano.
///
/// El camino nativo usa `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)`; esta
/// función es su equivalente puro (para pruebas y para decidir sin sesión
/// gráfica). El desempate es el primero de la lista.
pub fn monitor_for_point(rects: &[Rect], x: i32, y: i32) -> Option<Rect> {
    if let Some(rect) = rects
        .iter()
        .copied()
        .find(|rect| x >= rect.0 && x < rect.2 && y >= rect.1 && y < rect.3)
    {
        return Some(rect);
    }
    rects.iter().copied().min_by_key(|rect| {
        let dx = (rect.0 - x).max(x - rect.2).max(0) as i64;
        let dy = (rect.1 - y).max(y - rect.3).max(0) as i64;
        dx * dx + dy * dy
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_the_window_inside_the_work_area() {
        let work = (0, 0, 1920, 1080);
        assert_eq!(clamp_in(work, None, 500, 400, 100), (500, 400));
        assert_eq!(clamp_in(work, None, -50, -50, 100), (9, 9));
        assert_eq!(clamp_in(work, None, 5000, 5000, 100), (1811, 971));
    }

    #[test]
    fn clamp_avoids_a_bottom_taskbar() {
        let work = (0, 0, 1920, 1080);
        let taskbar = (0, 1040, 1920, 1080);
        assert_eq!(clamp_in(work, Some(taskbar), 500, 1070, 100), (500, 931));
        // Una barra arriba no recorta el borde inferior.
        let top = (0, 0, 1920, 40);
        assert_eq!(clamp_in(work, Some(top), 500, 1070, 100), (500, 971));
    }

    #[test]
    fn clamp_survives_a_window_larger_than_the_screen() {
        let work = (0, 0, 800, 600);
        assert_eq!(clamp_in(work, None, 100, 100, 1000), (9, 9));
    }

    #[test]
    fn monitor_selection_prefers_the_containing_rect() {
        let rects = [(0, 0, 1920, 1080), (1920, 0, 3840, 1080)];
        assert_eq!(monitor_for_point(&rects, 100, 100), Some(rects[0]));
        assert_eq!(monitor_for_point(&rects, 2000, 100), Some(rects[1]));
        assert_eq!(monitor_for_point(&[], 0, 0), None);
    }

    #[test]
    fn monitor_selection_falls_back_to_the_nearest_rect() {
        let rects = [(0, 0, 1920, 1080), (1920, 0, 3840, 1080)];
        assert_eq!(monitor_for_point(&rects, -10, 500), Some(rects[0]));
        assert_eq!(monitor_for_point(&rects, 4000, 500), Some(rects[1]));
        assert_eq!(monitor_for_point(&rects, 1900, 500), Some(rects[0]));
    }
}
