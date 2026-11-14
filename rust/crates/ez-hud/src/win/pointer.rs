//! Port of `src/ezagent/voice/hud/layered.py` — puntero (F5-01).
//!
//! Zonas interactivas (`_hits`), alpha del último fotograma (hit-test por
//! píxel) y el cursor del sistema. La parte pura se prueba sin Win32.

use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{LoadCursorW, SetCursor};

/// `IDC_ARROW` como identificador de recurso.
pub const IDC_ARROW: u16 = 32512;
/// `IDC_HAND` como identificador de recurso.
pub const IDC_HAND: u16 = 32649;

/// Una zona interactiva: `(x1, y1, x2, y2, accion)` con bordes inclusivos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub action: String,
}

impl HitRegion {
    pub fn new(x1: i32, y1: i32, x2: i32, y2: i32, action: impl Into<String>) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            action: action.into(),
        }
    }

    /// `x1 <= x <= x2 and y1 <= y <= y2`.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.x1 <= x && x <= self.x2 && self.y1 <= y && y <= self.y2
    }
}

/// Acción bajo el punto: la primera zona que lo contiene.
pub fn hit_action(hits: &[HitRegion], x: i32, y: i32) -> Option<String> {
    hits.iter()
        .find(|hit| hit.contains(x, y))
        .map(|hit| hit.action.clone())
}

/// Último fotograma presentado, en BGRA premultiplicado.
#[derive(Clone, Debug, Default)]
pub struct BgraFrame {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

impl BgraFrame {
    /// Alpha del píxel, 0 fuera del fotograma.
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if !(0 <= x && x < self.width && 0 <= y && y < self.height) {
            return 0;
        }
        let offset = ((y as usize * self.width as usize) + x as usize) * 4 + 3;
        self.pixels.get(offset).copied().unwrap_or(0)
    }
}

/// Cursor a cargar: `ident` manda; si es 0, mano o flecha.
pub fn cursor_ident_for(hand: bool, ident: u16) -> u16 {
    if ident != 0 {
        ident
    } else if hand {
        IDC_HAND
    } else {
        IDC_ARROW
    }
}

/// Carga y aplica un cursor del sistema (`IDC_*`).
pub fn set_cursor(hand: bool, ident: u16) {
    let resource = cursor_ident_for(hand, ident);
    // SAFETY: `resource` es un identificador `IDC_*` (no un puntero de memoria)
    // como exige `LoadCursorW` para los recursos del sistema.
    let loaded = unsafe { LoadCursorW(None, PCWSTR::from_raw(resource as usize as *const u16)) };
    if let Ok(cursor) = loaded {
        // SAFETY: cursor válido devuelto por LoadCursorW.
        unsafe {
            SetCursor(Some(cursor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_regions_are_inclusive_and_first_wins() {
        let hits = vec![
            HitRegion::new(10, 10, 20, 20, "a"),
            HitRegion::new(15, 15, 30, 30, "b"),
        ];
        assert_eq!(hit_action(&hits, 10, 10).as_deref(), Some("a"));
        assert_eq!(hit_action(&hits, 20, 20).as_deref(), Some("a"));
        assert_eq!(hit_action(&hits, 25, 25).as_deref(), Some("b"));
        assert_eq!(hit_action(&hits, 9, 10), None);
        assert_eq!(hit_action(&hits, 31, 25), None);
    }

    #[test]
    fn alpha_reads_the_last_frame_and_zero_outside() {
        let mut frame = BgraFrame {
            width: 2,
            height: 2,
            pixels: vec![0; 16],
        };
        frame.pixels[3] = 7; // (0, 0)
        frame.pixels[15] = 200; // (1, 1)
        assert_eq!(frame.alpha_at(0, 0), 7);
        assert_eq!(frame.alpha_at(1, 1), 200);
        assert_eq!(frame.alpha_at(2, 1), 0);
        assert_eq!(frame.alpha_at(0, -1), 0);
        assert_eq!(BgraFrame::default().alpha_at(0, 0), 0);
    }

    #[test]
    fn cursor_ident_prefers_the_explicit_resource() {
        assert_eq!(cursor_ident_for(false, 0), IDC_ARROW);
        assert_eq!(cursor_ident_for(true, 0), IDC_HAND);
        assert_eq!(cursor_ident_for(true, 32648), 32648);
    }
}
