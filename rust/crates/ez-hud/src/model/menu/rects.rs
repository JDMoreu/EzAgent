//! Port of `src/ezagent/voice/hud/menu.py 1422-1476` (F5-08), más
//! `_palette_pills` (2756-2765), que `hit` necesita para las muestras de tema.
//!
//! Rectángulos de control en coordenadas lógicas de ventana.
//!
//! // DECISION: `_palette_pills` es geometría y `hit` la necesita antes de que
//! // exista F5-10 (pintura); se expone aquí para que la pintura la reutilice.

use std::collections::BTreeMap;

use crate::layout;
use crate::tokens::Rect;

use super::geometry::Menu;

/// Contenedor de grupo (`GROUP_X0` del Python).
pub const GROUP_X0: f64 = 19.0;
/// Borde derecho del contenedor de grupo sin el ancho extra.
pub const GROUP_X1: f64 = 301.0;
/// Contenido de fila (`ROW_X0`).
pub const ROW_X0: f64 = 31.0;
/// Borde derecho del contenido de fila sin el ancho extra.
pub const ROW_X1: f64 = 289.0;
/// Diámetro del botón «–» de la cabecera.
pub const HIDE_SIZE: f64 = 24.0;
/// Centro horizontal del botón «–».
pub const HIDE_CX: f64 = 289.0;
/// Borde izquierdo del botón «Guardar».
pub const SAVE_BUTTON_X0: f64 = 23.0;
/// Borde derecho del botón «Guardar».
pub const SAVE_BUTTON_X1: f64 = SAVE_BUTTON_X0 + 72.0;
/// Alto de un botón capsular.
pub const BUTTON_H: f64 = 28.0;
/// Ancho mínimo de un botón capsular.
pub const BUTTON_MIN_W: f64 = 72.0;
/// Margen interior horizontal de un botón.
pub const BUTTON_PAD_X: f64 = 14.0;
/// Borde derecho del botón «Probar».
pub const TEST_BUTTON_X1: f64 = ROW_X1;
/// Barras de la mini onda de «Hablando…».
pub const TEST_WAVE_BARS: usize = 5;
/// Ancho de cada barra de la mini onda.
pub const TEST_WAVE_BAR_W: f64 = 2.0;
/// Hueco entre barras de la mini onda.
pub const TEST_WAVE_GAP: f64 = 2.0;
/// Separación entre la onda y el texto «Hablando…».
pub const TEST_WAVE_GAP_TEXT: f64 = 6.0;
/// Centro vertical de la segunda línea de una fila de texto.
pub const TEXT_LINE2_CY: f64 = 37.0;
/// Diámetro del botón «copiar» de una clave secreta.
pub const COPY_BTN: f64 = 26.0;
/// Inicio de la pista de un slider.
pub const SLIDER_TRACK_X0: f64 = ROW_X0;
/// Fin de la pista de un slider sin el ancho extra.
pub const SLIDER_TRACK_X1: f64 = ROW_X1;
/// Grosor de la pista de un slider.
pub const SLIDER_TRACK_H: f64 = 4.0;
/// Zona sensible vertical alrededor de la pista.
pub const SLIDER_TRACK_PAD: f64 = 10.0;
/// Centro vertical de la segunda línea de un slider.
pub const SLIDER_LINE2_CY: f64 = 38.0;
/// Origen vertical de la pista del medidor dentro de su fila.
pub const METER_TRACK_Y: f64 = 36.0;
/// Grosor de la pista del medidor.
pub const METER_TRACK_H: f64 = 4.0;
/// Zona sensible vertical alrededor de la pista del medidor.
pub const METER_TRACK_PAD: f64 = 10.0;
/// Las tres muestras de la fila de paletas, en orden de dibujo.
pub const PALETTES: [(&str, &str); 3] = [
    ("light", "Claro"),
    ("dark", "Oscuro"),
    ("custom", "Personalizado"),
];

/// Punto dentro de un rectángulo cerrado, como `a <= x <= b` en Python.
pub(crate) fn inside(rect: Rect, x: f64, y: f64) -> bool {
    rect.x0 <= x && x <= rect.x1 && rect.y0 <= y && y <= rect.y1
}

impl Menu {
    /// Círculo de 24 px del botón «–», centrado en la cabecera.
    pub fn hide_rect(&self, layout: &BTreeMap<String, layout::Rect>) -> Rect {
        let header = layout
            .get("header")
            .expect("el layout siempre tiene cabecera");
        let cy = (header.y0 + header.y1) / 2.0;
        let radius = HIDE_SIZE / 2.0;
        Rect::new(
            (HIDE_CX + self.dx) - radius,
            cy - radius,
            (HIDE_CX + self.dx) + radius,
            cy + radius,
        )
    }

    /// Botón «Guardar» del pie, centrado verticalmente.
    pub fn save_rect(&self, layout: &BTreeMap<String, layout::Rect>) -> Rect {
        let footer = layout.get("footer").expect("el layout siempre tiene pie");
        let cy = (footer.y0 + footer.y1) / 2.0;
        Rect::new(
            SAVE_BUTTON_X0,
            cy - BUTTON_H / 2.0,
            SAVE_BUTTON_X1,
            cy + BUTTON_H / 2.0,
        )
    }

    /// Ancho del contenido del botón «Probar»: onda + «Hablando…».
    pub fn test_content_width(&self) -> f64 {
        let wave =
            TEST_WAVE_BARS as f64 * TEST_WAVE_BAR_W + (TEST_WAVE_BARS as f64 - 1.0) * TEST_WAVE_GAP;
        // `text_size` mide con la fuente real, como el Python. // DECISION: una
        // escala inválida devuelve 0 en vez del `ValueError` del Python; la
        // escala ya se valida en el constructor y en `set_scale`.
        let talk = ez_gfx::text::text_size("Hablando…", "FS_SECONDARY", self.scale, None, "left")
            .map_or(0.0, |(width, _height)| width);
        wave + TEST_WAVE_GAP_TEXT + talk
    }

    /// Ancho animado de «Probar» entre el mínimo y el que cabe su contenido.
    pub fn test_width(&self) -> f64 {
        let fraction = self.testing_t.value.clamp(0.0, 1.0);
        let dynamic = BUTTON_MIN_W.max(BUTTON_PAD_X * 2.0 + self.test_content_width());
        BUTTON_MIN_W + (dynamic - BUTTON_MIN_W) * fraction
    }

    /// Rectángulo de «Probar»; si la sección no está a la vista, queda fuera
    /// de todo.
    pub fn test_rect(&self, layout: &BTreeMap<String, layout::Rect>, scroll: f64) -> Rect {
        let Some(row) = layout.get("test_voice") else {
            return Rect::new(-10.0, -10.0, -9.0, -9.0);
        };
        let cy = (row.y0 + row.y1) / 2.0 - scroll;
        let width = self.test_width();
        Rect::new(
            (TEST_BUTTON_X1 + self.dx) - width,
            cy - BUTTON_H / 2.0,
            TEST_BUTTON_X1 + self.dx,
            cy + BUTTON_H / 2.0,
        )
    }

    /// Rectángulo de un botón de acción alineado a la derecha de la fila.
    pub fn action_rect(
        &self,
        layout: &BTreeMap<String, layout::Rect>,
        key: &str,
        scroll: f64,
    ) -> Rect {
        let row = layout.get(key).expect("la acción visible tiene fila");
        let cy = (row.y0 + row.y1) / 2.0 - scroll;
        Rect::new(
            (TEST_BUTTON_X1 + self.dx) - BUTTON_MIN_W,
            cy - BUTTON_H / 2.0,
            TEST_BUTTON_X1 + self.dx,
            cy + BUTTON_H / 2.0,
        )
    }

    /// Botón «copiar» de una fila secreta, a la derecha de la línea de valor.
    pub fn copy_rect(&self, row_y0: f64) -> Rect {
        let right = ROW_X1 + self.dx + 2.0;
        let cy = row_y0 + TEXT_LINE2_CY;
        let half = COPY_BTN / 2.0;
        Rect::new(right - COPY_BTN, cy - half, right, cy + half)
    }

    /// Las tres muestras de la fila de paletas, en coordenadas del menú.
    pub fn palette_pills(&self, row_y0: f64) -> Vec<(&'static str, &'static str, Rect)> {
        let x0 = ROW_X0;
        let x1 = ROW_X1 + self.dx;
        let gap = 10.0;
        let width = (x1 - x0 - gap * 2.0) / 3.0;
        PALETTES
            .iter()
            .enumerate()
            .map(|(index, &(value, label))| {
                let left = x0 + index as f64 * (width + gap);
                (
                    value,
                    label,
                    Rect::new(left, row_y0 + 10.0, left + width, row_y0 + 36.0),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::settings::MenuSettings;

    use super::super::geometry::{MenuCallbacks, MenuOptions};
    use super::*;

    fn menu() -> Menu {
        let mut menu = Menu::new(
            MenuSettings::default(),
            MenuOptions::default(),
            MenuCallbacks::default(),
        )
        .expect("opciones válidas");
        menu.set_max_height(Some(2000.0));
        menu.set_groups(
            vec![(
                "prueba".to_string(),
                vec![
                    ("test_voice".to_string(), "action".to_string()),
                    ("repeat_last".to_string(), "action".to_string()),
                    ("speed".to_string(), "slider".to_string()),
                ],
            )],
            Some("Prueba"),
            Some(0.0),
        );
        menu.open(0.0);
        menu.height(2.0);
        menu
    }

    #[test]
    fn header_and_footer_rects_use_the_visible_boxes() {
        let menu = menu();
        let layout = menu.layout();
        assert_eq!(menu.hide_rect(&layout), Rect::new(277.0, 50.0, 301.0, 74.0));
        assert_eq!(
            menu.save_rect(&layout),
            Rect::new(23.0, 1988.0, 95.0, 2016.0)
        );
    }

    #[test]
    fn the_width_dx_moves_the_right_anchored_controls() {
        let mut menu = menu();
        menu.set_width(Some(420.0));
        let layout = menu.layout();
        let hide = menu.hide_rect(&layout);
        assert_eq!(hide.x1, 401.0);
        let row = layout.get("repeat_last").copied().expect("fila de acción");
        let action = menu.action_rect(&layout, "repeat_last", 0.0);
        assert_eq!(action.x1, 389.0);
        assert_eq!(action.width(), 72.0);
        assert!((action.center().1 - (row.y0 + row.y1) / 2.0).abs() < 1e-9);
        let copy = menu.copy_rect(100.0);
        assert_eq!(copy, Rect::new(365.0, 124.0, 391.0, 150.0));
    }

    #[test]
    fn the_test_button_grows_with_its_content() {
        let mut menu = menu();
        let layout = menu.layout();
        let rest = menu.test_rect(&layout, 0.0);
        assert_eq!(rest.width(), BUTTON_MIN_W);
        assert!(menu.test_content_width() > 24.0);
        assert!((menu.test_content_width() - 87.0).abs() < 1e-9);
        menu.set_testing(true, Some(2.0));
        menu.update(3.0);
        let testing = menu.test_rect(&layout, 0.0);
        assert!(testing.width() > rest.width());
        assert!((testing.width() - 115.0).abs() < 1e-9);
        assert_eq!(testing.x1, 289.0);
    }

    #[test]
    fn a_hidden_test_row_has_a_rect_outside_everything() {
        let mut menu = menu();
        menu.set_groups(
            vec![(
                "prueba".to_string(),
                vec![("repeat_last".to_string(), "action".to_string())],
            )],
            Some("Prueba"),
            Some(0.0),
        );
        let layout = menu.layout();
        assert_eq!(
            menu.test_rect(&layout, 0.0),
            Rect::new(-10.0, -10.0, -9.0, -9.0)
        );
    }

    #[test]
    fn palette_pills_divide_the_row_in_three_equal_parts() {
        let menu = menu();
        let pills = menu.palette_pills(200.0);
        assert_eq!(pills.len(), 3);
        assert_eq!(pills[0].0, "light");
        assert_eq!(pills[2].2.y0, 210.0);
        assert_eq!(pills[2].2.y1, 236.0);
        assert!((pills[1].2.x0 - (pills[0].2.x1 + 10.0)).abs() < 1e-9);
        assert!((pills[2].2.x1 - ROW_X1).abs() < 1e-9);
    }
}
