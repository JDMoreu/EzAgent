//! Port of `src/ezagent/voice/hud/layered.py` — geometría del escritorio (F5-01).
//!
//! `work_area` (área de trabajo, sin la barra de tareas), `monitor_rect`
//! (`rcMonitor` del monitor bajo el cursor), `taskbar_rect` y `clamp_pos`, que
//! mantiene la ventana dentro de la pantalla. La lógica pura (elegir monitor a
//! partir de rectángulos y recortar posiciones) no toca Win32 y se prueba aquí.
//!
//! DX-7: con `EZAGENT_TEST_MONITOR=secondary` todas las áreas que usan las
//! ventanas (barra, chat, menú y vista rápida) son las del primer monitor no
//! principal. Sin la variable —y en la app de la persona— todo sigue igual.

use std::mem::size_of;

use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, GetWindowRect, SystemParametersInfoW, MONITORINFOF_PRIMARY,
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// `(left, top, right, bottom)` en píxeles físicos.
pub type Rect = (i32, i32, i32, i32);

/// Separación al borde del área de trabajo, en unidades lógicas.
pub const EDGE_MARGIN: i32 = 18;

/// Lo que devuelve el Python si la consulta de Win32 falla.
pub const DEFAULT_WORK_AREA: Rect = (0, 0, 1920, 1080);

/// Variable **solo para pruebas**: `secondary` mueve las ventanas del HUD al
/// primer monitor no principal. Cualquier otro valor se ignora.
///
/// La app de la persona no la define. DX-7b: la variable no puede pedir la
/// pantalla principal; los humos validan la secundaria antes de abrir nada.
pub const TEST_MONITOR_ENV: &str = "EZAGENT_TEST_MONITOR";

/// Monitor que piden las pruebas con [`TEST_MONITOR_ENV`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenChoice {
    /// Monitor principal (solo para la lógica pura de [`choose_monitor`]).
    Primary,
    /// Primer monitor no principal, en el orden de `EnumDisplayMonitors`.
    Secondary,
}

/// Interpreta el valor de [`TEST_MONITOR_ENV`] (sin leer el entorno).
///
/// DX-7b: solo `secondary` activa el modo de prueba; cualquier otro valor se
/// ignora y la app queda en su pantalla normal.
pub fn screen_choice(value: Option<&str>) -> Option<ScreenChoice> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "secondary" => Some(ScreenChoice::Secondary),
        _ => None,
    }
}

/// Monitor que pide el entorno, si pide alguno.
pub fn requested_screen() -> Option<ScreenChoice> {
    let value = std::env::var(TEST_MONITOR_ENV).ok();
    screen_choice(value.as_deref())
}

/// Elige `(rcMonitor, rcWork)` del monitor pedido.
///
/// La lista es `(rcMonitor, rcWork, es_principal)`. `Primary` toma el primer
/// principal (o el primero de la lista si ninguno lo está); `Secondary` toma el
/// primer no principal y devuelve `None` si no hay ninguno.
pub fn choose_monitor(
    monitors: &[(Rect, Rect, bool)],
    choice: ScreenChoice,
) -> Option<(Rect, Rect)> {
    match choice {
        ScreenChoice::Primary => monitors
            .iter()
            .find(|(_, _, primary)| *primary)
            .or_else(|| monitors.first())
            .map(|(rect, work, _)| (*rect, *work)),
        ScreenChoice::Secondary => monitors
            .iter()
            .find(|(_, _, primary)| !*primary)
            .map(|(rect, work, _)| (*rect, *work)),
    }
}

/// Comprueba que hay un monitor secundario y devuelve su `rcMonitor`.
///
/// Falla cerrado: una lista vacía o sin monitor no principal devuelve el
/// motivo, para que un humo no cree ventanas. La lista es la misma que usa
/// [`choose_monitor`].
pub fn require_secondary(monitors: &[(Rect, Rect, bool)]) -> Result<Rect, String> {
    if monitors.is_empty() {
        return Err(String::from(
            "no se pudieron enumerar los monitores del escritorio; no se puede verificar la secundaria",
        ));
    }
    choose_monitor(monitors, ScreenChoice::Secondary)
        .map(|(rect, _)| rect)
        .ok_or_else(|| {
            String::from("no hay una pantalla secundaria disponible; los humos no abren ventanas en la principal")
        })
}

/// `true` si `rect` está entero dentro de `monitor`.
pub fn rect_inside(rect: Rect, monitor: Rect) -> bool {
    rect.0 >= monitor.0 && rect.1 >= monitor.1 && rect.2 <= monitor.2 && rect.3 <= monitor.3
}

/// Monitores del escritorio con `EnumDisplayMonitors` + `GetMonitorInfoW`.
fn desktop_monitors() -> Vec<(Rect, Rect, bool)> {
    // SAFETY: el callback solo escribe en el `Vec` que `desktop_monitors`
    // mantiene vivo durante `EnumDisplayMonitors`; `info` es local y lleva su
    // `cbSize`.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` es el puntero al `Vec` de `desktop_monitors`.
        let found = unsafe { &mut *(data.0 as *mut Vec<(Rect, Rect, bool)>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `monitor` lo entrega Windows y `info` es local con su `cbSize`.
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            found.push((
                as_rect(info.rcMonitor),
                as_rect(info.rcWork),
                info.dwFlags & MONITORINFOF_PRIMARY != 0,
            ));
        }
        BOOL::from(true)
    }

    let mut found = Vec::new();
    // SAFETY: el callback solo escribe en `found`, vivo durante la llamada.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut found as *mut Vec<(Rect, Rect, bool)> as isize),
        )
    };
    found
}

/// Área de trabajo del monitor pedido, si existe.
fn chosen_area(choice: ScreenChoice) -> Option<(Rect, Rect)> {
    choose_monitor(&desktop_monitors(), choice)
}

/// Comprueba el monitor secundario real antes de crear ventanas o procesos.
///
/// Falla cerrado si [`TEST_MONITOR_ENV`] no pide `secondary`, si Windows no
/// enumera monitores o si no hay ninguno no principal. Devuelve su `rcMonitor`.
pub fn require_secondary_monitor() -> Result<Rect, String> {
    if requested_screen() != Some(ScreenChoice::Secondary) {
        return Err(format!("{TEST_MONITOR_ENV} no pide el monitor secundario"));
    }
    require_secondary(&desktop_monitors())
}

fn as_rect(rect: RECT) -> Rect {
    (rect.left, rect.top, rect.right, rect.bottom)
}

/// Área de trabajo, ya sin la barra de tareas.
///
/// Sin [`TEST_MONITOR_ENV`] usa `SPI_GETWORKAREA` (la principal). Con
/// `secondary` usa el `rcWork` del primer monitor no principal; si no hay,
/// avisa por `tracing` y devuelve la principal.
///
/// El código anterior usaba `0x0048`, que no es esa constante: la llamada
/// fallaba y la muesca se colaba bajo la barra.
pub fn work_area() -> Rect {
    if requested_screen() == Some(ScreenChoice::Secondary) {
        match chosen_area(ScreenChoice::Secondary) {
            Some((_, work)) => return work,
            None => {
                tracing::warn!(
                    "EZAGENT_TEST_MONITOR=secondary sin monitor secundario; se usa la principal"
                );
            }
        }
    }
    primary_work_area()
}

/// `SPI_GETWORKAREA`: el área de trabajo de la pantalla principal.
fn primary_work_area() -> Rect {
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
        as_rect(rect)
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
    // DX-7: en las pruebas, el monitor es el que pide la variable, aunque el
    // puntero esté en el otro.
    if let Some(choice) = requested_screen() {
        if let Some((rect, _)) = chosen_area(choice) {
            return rect;
        }
    }
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
    // DX-7: la barra de tareas vive en la principal. Recortar contra ella
    // movería las ventanas de la secundaria (su `y` cae dentro del rango del
    // área vertical) hacia la mitad de la pantalla.
    if requested_screen() == Some(ScreenChoice::Secondary)
        && chosen_area(ScreenChoice::Secondary).is_some()
    {
        return None;
    }
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

    #[test]
    fn test_monitor_variable_only_accepts_secondary() {
        assert_eq!(
            screen_choice(Some("secondary")),
            Some(ScreenChoice::Secondary)
        );
        assert_eq!(
            screen_choice(Some(" SECONDARY ")),
            Some(ScreenChoice::Secondary)
        );
        assert_eq!(screen_choice(Some("primary")), None);
        assert_eq!(screen_choice(Some("DISPLAY2")), None);
        assert_eq!(screen_choice(None), None);
    }

    #[test]
    fn secondary_validation_accepts_a_vertical_secondary() {
        let primary = ((0, 0, 1920, 1080), (0, 0, 1920, 1040), true);
        let secondary = ((-1080, -393, 0, 1527), (-1080, -393, 0, 1527), false);
        assert_eq!(require_secondary(&[primary, secondary]), Ok(secondary.0));
    }

    #[test]
    fn secondary_validation_fails_closed_without_a_secondary() {
        let primary = ((0, 0, 1920, 1080), (0, 0, 1920, 1040), true);
        let error = require_secondary(&[primary]).expect_err("sin secundaria");
        assert!(error.contains("secundaria"));
        assert!(require_secondary(&[]).is_err());
    }

    #[test]
    fn monitor_choice_uses_the_first_secondary() {
        let primary = ((0, 0, 1920, 1080), (0, 0, 1920, 1040), true);
        let secondary = ((-1080, -393, 0, 1527), (-1080, -393, 0, 1527), false);
        let monitors = [primary, secondary];
        assert_eq!(
            choose_monitor(&monitors, ScreenChoice::Primary),
            Some((primary.0, primary.1))
        );
        assert_eq!(
            choose_monitor(&monitors, ScreenChoice::Secondary),
            Some((secondary.0, secondary.1))
        );
    }

    #[test]
    fn vertical_secondary_on_the_left_keeps_its_negative_coordinates() {
        let secondary = ((-1080, -393, 0, 1527), (-1080, -393, 0, 1527), false);
        let monitors = [secondary, ((0, 0, 1920, 1080), (0, 0, 1920, 1080), true)];
        let (rect, work) = choose_monitor(&monitors, ScreenChoice::Secondary).expect("secundaria");
        assert_eq!(rect, secondary.0);
        // El recorte se queda dentro de la vertical, con `x` e `y` negativos.
        assert_eq!(clamp_in(work, None, -5000, -5000, 100), (-1071, -384));
        assert_eq!(clamp_in(work, None, 5000, 5000, 100), (-109, 1418));
    }

    #[test]
    fn without_secondary_the_choice_is_none() {
        let primary = ((0, 0, 1920, 1080), (0, 0, 1920, 1040), true);
        assert_eq!(choose_monitor(&[primary], ScreenChoice::Secondary), None);
        assert_eq!(choose_monitor(&[], ScreenChoice::Primary), None);
    }

    #[test]
    fn rect_inside_checks_every_edge() {
        let monitor = (-1080, -393, 0, 1527);
        assert!(rect_inside((-1000, -300, -100, 1000), monitor));
        assert!(!rect_inside((-1000, -300, 10, 1000), monitor));
        assert!(!rect_inside((-1000, -300, -100, 2000), monitor));
        assert!(!rect_inside((-1100, -300, -100, 1000), monitor));
    }
}
