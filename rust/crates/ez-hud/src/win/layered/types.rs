//! Tipos del módulo `layered` (F5-01).
//!
//! Fotogramas RGBA, eventos del canal, respuestas síncronas del dueño y
//! parámetros de creación. Se separaron de `layered.rs` para respetar el límite
//! de líneas por módulo.

use super::super::pointer::HitRegion;

/// `FPS_ACTIVE` del Python (no lo usa el propio módulo).
pub const FPS_ACTIVE: u32 = 30;

/// Presupuesto por defecto entre fotogramas (`1.0 / 20.0`).
pub const DEFAULT_INTERVAL: f64 = 1.0 / 20.0;

/// Fotograma RGBA de 8 bits por canal (sin premultiplicar).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaFrame {
    /// Valida `w * h * 4` bytes exactos.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 || data.len() != width as usize * height as usize * 4 {
            return None;
        }
        Some(Self {
            width,
            height,
            data,
        })
    }

    /// Fotograma de un color (pruebas y ventanas vacías).
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            data.extend_from_slice(&rgba);
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Formato del DIB: BGRA con los canales de color premultiplicados por alfa.
    ///
    /// DECISION: redondeo al entero más cercano (`(c*a + 127) / 255`); el Python
    /// recibía el BGRA ya premultiplicado del dibujo y no define este paso.
    pub fn to_bgra_premultiplied(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len());
        for pixel in self.data.as_chunks::<4>().0 {
            let alpha = pixel[3] as u32;
            let premultiply = |channel: u8| ((channel as u32 * alpha + 127) / 255) as u8;
            out.extend_from_slice(&[
                premultiply(pixel[2]),
                premultiply(pixel[1]),
                premultiply(pixel[0]),
                pixel[3],
            ]);
        }
        out
    }
}

/// Mensajes que produce la ventana (el canal es la API asíncrona).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    /// Clic sobre una zona interactiva (`on_click`).
    Click(String),
    /// Archivos soltados sobre la ventana (`on_files`).
    Files(Vec<std::path::PathBuf>),
    /// Pérdida de foco mientras se editaba (`on_focus_lost`).
    FocusLost,
    /// Cambio de DPI (`on_dpi_changed`).
    DpiChanged(u32),
    /// Botón derecho suelto (antes `_right_cb`).
    RightClick,
    /// Empieza un arrastre de ventana.
    DragStart { x: i32, y: i32 },
    /// La ventana se movió arrastrada.
    DragMove { x: i32, y: i32 },
    /// Terminó el arrastre (antes `on_window_drag_end`/`_moved_cb`).
    DragEnd { x: i32, y: i32 },
    /// Cambió la zona interactiva bajo el puntero.
    Hover(Option<String>),
    /// Rueda dentro de la ventana.
    Wheel { x: i32, y: i32, delta: i32 },
}

/// Respuestas síncronas del dueño de la ventana.
///
/// Son las sobrescrituras del Python que devuelven un valor; todo lo demás
/// viaja por el canal de [`WindowEvent`]. Las implementaciones por defecto son
/// las de `LayeredWindow` (ninguna zona interactiva ni teclado).
pub trait WindowHooks: Send {
    fn on_pointer_down(&mut self, _x: i32, _y: i32) -> bool {
        false
    }
    fn on_pointer_move(&mut self, _x: i32, _y: i32) {}
    fn on_pointer_up(&mut self, _x: i32, _y: i32) {}
    fn on_wheel(&mut self, _x: i32, _y: i32, _delta: i32) -> bool {
        false
    }
    fn on_global_wheel(&mut self, _x: i32, _y: i32, _delta: i32) -> bool {
        false
    }
    fn on_key(&mut self, _vk: u32, _ctrl: bool) -> bool {
        false
    }
    fn on_char(&mut self, _character: char) -> bool {
        false
    }
    fn on_paste(&mut self, _text: &str) -> bool {
        false
    }
    fn cursor_hand(&mut self, _x: i32, _y: i32) -> bool {
        false
    }
    fn cursor_ident(&mut self, _x: i32, _y: i32) -> u16 {
        0
    }
    fn clamp_position(&mut self, x: i32, y: i32, width: i32) -> (i32, i32) {
        super::super::monitor::clamp_pos(x, y, width)
    }
}

impl WindowHooks for () {}

/// Parámetros de creación (los del `__init__` del Python y atributos de clase).
#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub click_through: bool,
    pub title: String,
    pub lazy_drag: bool,
    pub max_height: i32,
    pub interval: f64,
    /// `_install_mouse_hook`: el Python base lo instala; las subclases lo omiten
    /// cuando la rueda ya sigue al puntero.
    pub mouse_hook: bool,
}

impl WindowOptions {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            x: 0,
            y: 0,
            click_through: false,
            title: String::from("ezagent"),
            lazy_drag: false,
            max_height: 0,
            interval: DEFAULT_INTERVAL,
            mouse_hook: true,
        }
    }
}

/// Estado mutable protegido (el Python confía en el GIL).
pub(crate) struct State {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
    pub(crate) visible: bool,
    pub(crate) keyboard: bool,
    pub(crate) applied_visible: bool,
    pub(crate) applied_keyboard: bool,
    pub(crate) native_pos: (i32, i32),
    pub(crate) dirty: bool,
    pub(crate) last_paint: f64,
    pub(crate) hover: Option<String>,
    pub(crate) hits: Vec<HitRegion>,
    pub(crate) dragging: bool,
    pub(crate) pointer_active: bool,
    pub(crate) drag_off: (i32, i32),
    pub(crate) drag_moved: bool,
    pub(crate) tracking: bool,
    pub(crate) previous_foreground: isize,
    pub(crate) restore_foreground_at: f64,
    pub(crate) restore_foreground_hwnd: isize,
    pub(crate) diag: String,
}

impl State {
    pub(crate) fn new(width: i32, height: i32, x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            w: width,
            h: height,
            visible: false,
            keyboard: false,
            applied_visible: false,
            applied_keyboard: false,
            native_pos: (x, y),
            dirty: true,
            last_paint: 0.0,
            hover: None,
            hits: Vec::new(),
            dragging: false,
            pointer_active: false,
            drag_off: (0, 0),
            drag_moved: false,
            tracking: false,
            previous_foreground: 0,
            restore_foreground_at: 0.0,
            restore_foreground_hwnd: 0,
            diag: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_conversion_premultiplies_and_swaps_channels() {
        let frame = RgbaFrame::new(1, 2, vec![255, 128, 0, 128, 10, 20, 30, 0]).expect("frame");
        let bgra = frame.to_bgra_premultiplied();
        assert_eq!(bgra, vec![0, 64, 128, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn opaque_rgba_is_just_bgra() {
        let frame = RgbaFrame::solid(1, 1, [10, 20, 30, 255]);
        assert_eq!(frame.to_bgra_premultiplied(), vec![30, 20, 10, 255]);
    }

    #[test]
    fn frames_require_exact_size() {
        assert!(RgbaFrame::new(2, 2, vec![0; 16]).is_some());
        assert!(RgbaFrame::new(2, 2, vec![0; 15]).is_none());
        assert!(RgbaFrame::new(0, 2, Vec::new()).is_none());
    }

    #[test]
    fn default_options_match_the_python_defaults() {
        let options = WindowOptions::new(320, 120);
        assert_eq!(options.title, "ezagent");
        assert!(!options.click_through);
        assert!(!options.lazy_drag);
        assert_eq!(options.max_height, 0);
        assert_eq!(options.interval, 1.0 / 20.0);
        assert!(options.mouse_hook);
        assert_eq!(options.interval, DEFAULT_INTERVAL);
        assert_eq!(FPS_ACTIVE, 30);
    }
}
