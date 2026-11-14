//! Port of `src/ezagent/voice/hud/layered.py` — arrastre de ventana (F5-01).
//!
//! La barra puede convertirse en píldora: al pulsar se guarda el desfase entre
//! el cursor y la esquina, cada movimiento recoloca la ventana y un desfase de
//! más de 2 px la marca como arrastrada (para no confundir un clic con un
//! arrastre).

use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY};

/// `_drag_off = (sx - x, sy - y)`.
pub fn drag_offset(screen: (i32, i32), window: (i32, i32)) -> (i32, i32) {
    (screen.0 - window.0, screen.1 - window.1)
}

/// Esquina nueva a partir del cursor y el desfase guardado.
pub fn drag_target(screen: (i32, i32), offset: (i32, i32)) -> (i32, i32) {
    (screen.0 - offset.0, screen.1 - offset.1)
}

/// `abs(nx - x) > 2 or abs(ny - y) > 2`.
pub fn drag_moved(target: (i32, i32), window: (i32, i32)) -> bool {
    (target.0 - window.0).abs() > 2 || (target.1 - window.1).abs() > 2
}

/// Estado REAL del botón izquierdo (no la cola de eventos).
pub fn lbutton_down() -> bool {
    // SAFETY: consulta de estado de tecla sin punteros.
    unsafe { (GetAsyncKeyState(VIRTUAL_KEY(0x01).0 as i32) as u16) & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_and_target_are_inverse() {
        let window = (100, 200);
        let screen = (130, 180);
        let offset = drag_offset(screen, window);
        assert_eq!(offset, (30, -20));
        assert_eq!(drag_target(screen, offset), window);
    }

    #[test]
    fn small_wobbles_do_not_count_as_a_drag() {
        let window = (100, 200);
        assert!(!drag_moved((102, 198), window));
        assert!(!drag_moved((98, 202), window));
        assert!(drag_moved((103, 200), window));
        assert!(drag_moved((100, 197), window));
    }
}
