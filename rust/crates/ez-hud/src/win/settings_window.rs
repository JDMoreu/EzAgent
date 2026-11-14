//! Port of `src/ezagent/voice/hud/settings_window.py` (F5-12): la ventana.
//!
//! Estado de la ventana de ajustes que necesita Win32 o que reparte eventos:
//! tamaño y posición recordados, arrastre, redimensionado desde bordes y
//! esquinas, doble clic en la cabecera y el cursor de cada borde. La geometría
//! y el dibujo viven en [`crate::paint::settings_window`]; aquí solo hay estado
//! y la consulta de modificadores del teclado (`GetAsyncKeyState`).

use crate::paint::hotkey_field::Mods;
use crate::paint::settings_window as sw;

use sw::{Edges, Pixels, WorkArea};

/// Segundos entre dos clics en la cabecera para volver al tamaño de fábrica.
pub const DOUBLE_CLICK: f64 = 0.4;

/// Rectángulo físico de la ventana.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Posición y tamaño que se guardan en `voice.json` (`settings_x`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedGeometry {
    pub x: i32,
    pub y: i32,
    pub w: i64,
    pub h: i64,
}

/// Qué hacer con un clic dentro de la ventana.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Zona libre: el llamador arrastra la ventana.
    Drag,
    /// Borde o esquina: el llamador debe llamar a
    /// [`SettingsWindow::begin_resize`] con las coordenadas de pantalla.
    Resize(Edges),
    /// Fila de la barra lateral: muestra esa sección.
    Section(&'static str),
    /// Doble clic en la cabecera: tamaño de fábrica y centrada.
    Reset,
}

/// Ventana de la tarjeta de ajustes.
pub struct SettingsWindow {
    work: WorkArea,
    scale: f64,
    rect: Rect,
    resizing: Option<(Edges, i32, i32, Pixels)>,
    header_click: f64,
    dragging: Option<(i32, i32)>,
}

impl SettingsWindow {
    /// Ventana con el tamaño guardado (o el de fábrica) y la posición guardada
    /// (o centrada), ambos acotados al área de trabajo.
    // DECISION: `_card_logical_size` + `_card_position` del Python se juntan en
    // un constructor; el Python guarda los dos en `cfg` y los relee al abrir.
    pub fn new(
        work: WorkArea,
        scale: f64,
        size: Option<(f64, f64)>,
        position: Option<(i32, i32)>,
    ) -> Self {
        let (want_w, want_h) = size
            .filter(|(w, h)| *w > 0.0 && *h > 0.0)
            .unwrap_or((sw::DEFAULT_W, sw::DEFAULT_H));
        let (logical_w, logical_h) = sw::clamp_size(want_w, want_h, work, scale);
        let (w, h) = (round_i32(logical_w * scale), round_i32(logical_h * scale));
        let (x, y) = position.map_or_else(
            || sw::centered(work, w, h),
            |(x, y)| sw::clamp_position(x, y, w, h, work, scale),
        );
        Self {
            work,
            scale,
            rect: Rect { x, y, w, h },
            resizing: None,
            header_click: 0.0,
            dragging: None,
        }
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn work(&self) -> WorkArea {
        self.work
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// `(ancho, alto)` lógicos actuales de la ventana.
    pub fn logical_size(&self) -> (f64, f64) {
        (
            self.rect.w as f64 / self.scale,
            self.rect.h as f64 / self.scale,
        )
    }

    /// `(ancho, alto visible)` lógicos del menú dentro de la ventana.
    pub fn content_size(&self) -> (f64, f64) {
        let (logical_w, logical_h) = self.logical_size();
        sw::content_size(logical_w, logical_h)
    }

    /// Posición y tamaño que hay que recordar en `voice.json`.
    pub fn saved_geometry(&self) -> SavedGeometry {
        SavedGeometry {
            x: self.rect.x,
            y: self.rect.y,
            w: round_i64(self.rect.w as f64 / self.scale),
            h: round_i64(self.rect.h as f64 / self.scale),
        }
    }

    /// Aplica un rectángulo físico nuevo (tras arrastrar o redimensionar).
    pub fn apply(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.rect = Rect { x, y, w, h };
    }

    /// Cambio de DPI: recupera el tamaño lógico y mantiene la ventana a la vista.
    pub fn set_scale(&mut self, scale: f64) {
        let (logical_w, logical_h) = self.logical_size();
        self.scale = scale.max(1.0);
        let (logical_w, logical_h) = sw::clamp_size(logical_w, logical_h, self.work, self.scale);
        let (w, h) = (
            round_i32(logical_w * self.scale),
            round_i32(logical_h * self.scale),
        );
        let (x, y) = sw::clamp_position(self.rect.x, self.rect.y, w, h, self.work, self.scale);
        self.rect = Rect { x, y, w, h };
    }

    /// Mantiene una franja de la ventana dentro del área de trabajo.
    pub fn clamp(&mut self) {
        let (x, y) = sw::clamp_position(
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
            self.work,
            self.scale,
        );
        self.rect.x = x;
        self.rect.y = y;
    }

    /// Doble clic en la cabecera: tamaño de fábrica y centrada.
    pub fn reset(&mut self) {
        let (logical_w, logical_h) =
            sw::clamp_size(sw::DEFAULT_W, sw::DEFAULT_H, self.work, self.scale);
        let (w, h) = (
            round_i32(logical_w * self.scale),
            round_i32(logical_h * self.scale),
        );
        let (x, y) = sw::centered(self.work, w, h);
        self.rect = Rect { x, y, w, h };
        self.header_click = 0.0;
    }

    /// Reparte un clic de la ventana en coordenadas locales.
    ///
    /// El menú tiene prioridad en el Python (`menu.hit`); el llamador debe
    /// consultarlo antes y no invocar esto si el clic cae en un control.
    pub fn pointer_down(&mut self, x: i32, y: i32, now: f64) -> Hit {
        if let Some((edges, _, _, _)) = self.resizing {
            return Hit::Resize(edges);
        }
        let (lx, ly) = self.logical_point(x, y);
        if let Some(edges) = self.edges_at(x, y) {
            return Hit::Resize(edges);
        }
        if let Some(section) = sw::item_at(lx, ly) {
            return Hit::Section(section);
        }
        if sw::in_header(lx, ly) {
            if now - self.header_click < DOUBLE_CLICK {
                self.header_click = 0.0;
                self.reset();
                return Hit::Reset;
            }
            self.header_click = now;
        }
        Hit::Drag
    }

    /// Borde o esquina bajo `(x, y)` en coordenadas locales de la ventana.
    pub fn edges_at(&self, x: i32, y: i32) -> Option<Edges> {
        let (lx, ly) = self.logical_point(x, y);
        let (logical_w, logical_h) = self.logical_size();
        sw::resize_edges(lx, ly, logical_w, logical_h)
    }

    /// Empieza a redimensionar desde `edges` con el puntero en `(screen_x,
    /// screen_y)` (físicos de pantalla, como `card._screen_cursor()`).
    pub fn begin_resize(&mut self, edges: Edges, screen_x: i32, screen_y: i32) {
        self.resizing = Some((
            edges,
            screen_x,
            screen_y,
            (self.rect.x, self.rect.y, self.rect.w, self.rect.h),
        ));
    }

    /// Mueve el borde agarrado hasta el puntero (físicos); `true` si cambió.
    pub fn resize_to(&mut self, x: i32, y: i32) -> bool {
        let Some((edges, start_x, start_y, start)) = self.resizing else {
            return false;
        };
        let limits = (
            round_i32(sw::MIN_W * self.scale),
            round_i32(sw::MIN_H * self.scale),
            self.work.2 - self.work.0,
            self.work.3 - self.work.1,
        );
        let (nx, ny, nw, nh) = sw::resized_rect(edges, start, (x - start_x, y - start_y), limits);
        if (nx, ny, nw, nh) == (self.rect.x, self.rect.y, self.rect.w, self.rect.h) {
            return false;
        }
        self.rect = Rect {
            x: nx,
            y: ny,
            w: nw,
            h: nh,
        };
        true
    }

    /// Suelta el borde; devuelve la geometría que hay que guardar.
    pub fn end_resize(&mut self) -> Option<SavedGeometry> {
        self.resizing.take().map(|_| self.saved_geometry())
    }

    /// Empieza a arrastrar la ventana desde `(x, y)` (físicos de pantalla).
    pub fn move_start(&mut self, x: i32, y: i32) {
        self.dragging = Some((x - self.rect.x, y - self.rect.y));
    }

    /// Arrastra la ventana; `true` si se movió.
    pub fn move_to(&mut self, x: i32, y: i32) -> bool {
        let Some((offset_x, offset_y)) = self.dragging else {
            return false;
        };
        let (nx, ny) = sw::clamp_position(
            x - offset_x,
            y - offset_y,
            self.rect.w,
            self.rect.h,
            self.work,
            self.scale,
        );
        if (nx, ny) == (self.rect.x, self.rect.y) {
            return false;
        }
        self.rect.x = nx;
        self.rect.y = ny;
        true
    }

    /// Suelta el arrastre; devuelve la geometría que hay que guardar.
    ///
    /// Un arrastre no cuenta como primer clic de la cabecera (`card_moved`).
    pub fn move_end(&mut self) -> Option<SavedGeometry> {
        self.header_click = 0.0;
        self.dragging.take().map(|_| self.saved_geometry())
    }

    /// `card_cursor`: `IDC_*` si el punto está en un borde; 0 si no.
    pub fn cursor_at(&self, x: i32, y: i32) -> i32 {
        if let Some((edges, _, _, _)) = self.resizing {
            return sw::cursor_for(Some(edges));
        }
        let (lx, ly) = self.logical_point(x, y);
        let (logical_w, logical_h) = self.logical_size();
        sw::cursor_for(sw::resize_edges(lx, ly, logical_w, logical_h))
    }

    /// Carga y aplica el cursor del borde bajo `(x, y)`.
    ///
    /// Fuera de un borde no toca el cursor: la mano o la flecha las decide el
    /// llamador (el menú tiene prioridad, como `card_cursor` en el Python).
    pub fn apply_cursor(&self, x: i32, y: i32) {
        let ident = self.cursor_at(x, y);
        if ident != 0 {
            crate::win::pointer::set_cursor(false, ident as u16);
        }
    }

    fn logical_point(&self, x: i32, y: i32) -> (f64, f64) {
        (x as f64 / self.scale, y as f64 / self.scale)
    }
}

fn round_i32(value: f64) -> i32 {
    value.round_ties_even() as i32
}

fn round_i64(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Modificadores pulsados ahora mismo (`hotkey_field.os_modifiers`).
pub fn os_modifiers() -> Mods {
    let mut mods = Mods::empty();
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        let held = |vk: i32| -> bool {
            // SAFETY: GetAsyncKeyState solo lee el estado del teclado; acepta
            // cualquier código de tecla y no usa punteros ni inicialización.
            let state = unsafe { GetAsyncKeyState(vk) } as u16;
            state & 0x8000 != 0
        };
        if held(0x11) {
            mods.insert("ctrl");
        }
        if held(0x12) {
            mods.insert("alt");
        }
        if held(0x10) {
            mods.insert("shift");
        }
        if held(0x5B) || held(0x5C) {
            mods.insert("win");
        }
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: WorkArea = (0, 0, 1920, 1040);

    fn window() -> SettingsWindow {
        SettingsWindow::new(WORK, 1.0, None, None)
    }

    #[test]
    fn the_factory_window_opens_centred_and_saved_sizes_are_clamped() {
        let default = window();
        assert_eq!(
            default.rect(),
            Rect {
                x: 580,
                y: 210,
                w: 760,
                h: 620
            }
        );
        assert_eq!(default.logical_size(), (760.0, 620.0));
        assert_eq!(default.content_size(), (576.0, 616.0));

        let tiny = SettingsWindow::new(WORK, 1.0, Some((100.0, 100.0)), None);
        assert_eq!(
            tiny.rect(),
            Rect {
                x: 680,
                y: 320,
                w: 560,
                h: 400
            }
        );
        let saved = SettingsWindow::new(WORK, 1.0, Some((1200.0, 800.0)), None);
        assert_eq!(
            saved.rect(),
            Rect {
                x: 360,
                y: 120,
                w: 1200,
                h: 800
            }
        );
        let invalid = SettingsWindow::new(WORK, 1.0, Some((0.0, -5.0)), None);
        assert_eq!(invalid.rect(), default.rect());
    }

    #[test]
    fn a_saved_position_is_kept_on_screen() {
        let kept = SettingsWindow::new(WORK, 1.0, None, Some((300, 200)));
        assert_eq!((kept.rect().x, kept.rect().y), (300, 200));
        let clamped = SettingsWindow::new(WORK, 1.0, None, Some((9999, 9999)));
        assert_eq!((clamped.rect().x, clamped.rect().y), (1824, 944));
        let far = SettingsWindow::new(WORK, 1.0, None, Some((-9999, -9999)));
        assert_eq!((far.rect().x, far.rect().y), (-664, 0));
    }

    #[test]
    fn clicks_go_to_edges_sidebar_header_or_drag() {
        let mut window = window();
        assert_eq!(
            window.pointer_down(750, 300, 10.0),
            Hit::Resize((false, false, true, false))
        );
        assert_eq!(
            window.pointer_down(750, 300, 10.0),
            Hit::Resize((false, false, true, false))
        );
        window.end_resize();

        let mut window = self::window();
        assert_eq!(
            window.pointer_down(100, 80, 10.0),
            Hit::Section("appearance")
        );
        assert_eq!(window.pointer_down(100, 264, 10.0), Hit::Section("system"));
        assert_eq!(window.pointer_down(100, 106, 10.0), Hit::Drag);
        assert_eq!(window.pointer_down(500, 300, 10.0), Hit::Drag);
        assert_eq!(window.pointer_down(500, 20, 10.0), Hit::Drag);
        assert_eq!(window.pointer_down(500, 20, 10.2), Hit::Reset);
        assert_eq!(
            window.rect(),
            Rect {
                x: 580,
                y: 210,
                w: 760,
                h: 620
            }
        );
        assert_eq!(window.pointer_down(500, 20, 10.2), Hit::Drag);
    }

    #[test]
    fn dragging_keeps_a_strip_visible_and_reports_geometry() {
        let mut window = window();
        window.move_start(600, 250);
        assert!(window.move_to(900, 500));
        assert_eq!((window.rect().x, window.rect().y), (880, 460));
        assert!(window.move_to(-9999, -9999));
        assert_eq!((window.rect().x, window.rect().y), (-664, 0));
        let saved = window.move_end().expect("hubo arrastre");
        assert_eq!(
            saved,
            SavedGeometry {
                x: -664,
                y: 0,
                w: 760,
                h: 620
            }
        );
        assert!(!window.move_to(10, 10));
        assert_eq!(window.move_end(), None);
    }

    #[test]
    fn resizing_pins_the_opposite_edge_and_respects_the_minimum() {
        let mut window = window();
        let edges = window.edges_at(750, 300).expect("borde derecho");
        window.begin_resize(edges, 1_000, 1_000);
        assert!(window.resize_to(1_100, 1_040));
        assert_eq!(
            window.rect(),
            Rect {
                x: 580,
                y: 210,
                w: 860,
                h: 620
            }
        );
        assert_eq!(window.cursor_at(850, 340), sw::IDC_SIZEWE);
        assert_eq!(
            window.end_resize(),
            Some(SavedGeometry {
                x: 580,
                y: 210,
                w: 860,
                h: 620
            })
        );
        assert_eq!(window.end_resize(), None);

        let edges = window.edges_at(8, 300).expect("borde izquierdo");
        window.begin_resize(edges, 1_000, 1_000);
        assert!(window.resize_to(1_050, 1_000));
        assert_eq!(
            window.rect(),
            Rect {
                x: 630,
                y: 210,
                w: 810,
                h: 620
            }
        );
        window.end_resize();

        let edges = window.edges_at(804, 618).expect("esquina inferior derecha");
        window.begin_resize(edges, 1_000, 1_000);
        assert!(window.resize_to(9_999, 9_999));
        assert_eq!(window.rect().w, 1920);
        assert_eq!(window.rect().h, 1040);
        window.resize_to(-9999, -9999);
        assert_eq!(window.rect().w, 560);
        assert_eq!(window.rect().h, 400);
        assert!(!window.resize_to(-9999, -9999));
    }

    #[test]
    fn a_drag_clears_the_pending_header_click() {
        let mut window = window();
        assert_eq!(window.pointer_down(500, 20, 1.0), Hit::Drag);
        window.move_start(600, 250);
        window.move_to(620, 260);
        assert!(window.move_end().is_some());
        assert_eq!(window.pointer_down(500, 20, 1.3), Hit::Drag);
        assert_eq!(window.pointer_down(500, 20, 1.5), Hit::Reset);
    }

    #[test]
    fn cursors_follow_the_corner_or_edge() {
        let window = window();
        assert_eq!(window.cursor_at(750, 300), sw::IDC_SIZEWE);
        assert_eq!(window.cursor_at(380, 618), sw::IDC_SIZENS);
        assert_eq!(window.cursor_at(20, 20), sw::IDC_SIZENWSE);
        assert_eq!(window.cursor_at(750, 618), sw::IDC_SIZENWSE);
        assert_eq!(window.cursor_at(8, 618), sw::IDC_SIZENESW);
        assert_eq!(window.cursor_at(380, 300), 0);
    }

    #[test]
    fn a_new_dpi_rescales_the_same_logical_window() {
        let mut window = window();
        window.set_scale(2.0);
        assert_eq!(window.scale(), 2.0);
        assert_eq!(
            window.rect(),
            Rect {
                x: 580,
                y: 210,
                w: 1520,
                h: 1008
            }
        );
        assert_eq!(window.logical_size(), (760.0, 504.0));
        window.apply(300, 200, 1140, 900);
        assert_eq!(
            window.saved_geometry(),
            SavedGeometry {
                x: 300,
                y: 200,
                w: 570,
                h: 450
            }
        );
        window.apply(9999, 9999, 1140, 900);
        window.clamp();
        assert_eq!((window.rect().x, window.rect().y), (1728, 848));
    }

    #[test]
    fn os_modifiers_reads_a_bounded_set() {
        let mods = os_modifiers();
        assert!(mods.iter().count() <= 4);
        assert!(mods
            .iter()
            .all(|name| crate::paint::hotkey_field::MODIFIERS.contains(&name)));
    }
}
