//! Port of `src/ezagent/voice/hud/menu.py 2339-3100` (F5-10a): marco,
//! cabecera, grupos, filas de paleta, selector, medidor y texto.
//!
//! `sections.rs` pinta el marco, la cabecera, los grupos y reparte las filas;
//! `controls.rs` pinta las filas de esta mitad. `advanced.rs` (F5-10b) completa
//! las filas restantes, la lista de selectores, el pie y la barra de
//! desplazamiento implementando [`FrameExtras`].
//!
//! # Contexto de pintura para F5-10b
//!
//! [`PaintContext`] envuelve una capa `float32` RGBA premultiplicada con la
//! escala, el tema, el ancho extra `dx` y la paleta del menú. Sus métodos son
//! los ayudantes de `menu.py` ya portados en `ez-gfx` (`_text_at`,
//! `_fill_round_rect`, `_fill_round_gradient`, `_stroke_round_rect`,
//! `_fill_circle`, `draw_icon`) más `button_colors` (las variantes C-07).
//! F5-10b debe reutilizarlos en vez de dibujar a mano.
//!
//! [`FrameExtras`] son los ganchos que faltan: filas de deslizador, interruptor
//! y acción, lista de selectores, atajos, secreto y botón de copiar, pie y
//! barra de desplazamiento. [`NoExtras`] los deja vacíos para que el marco de
//! esta tarea se pueda pintar y probar solo; F5-10b implementa el trait en
//! `advanced.rs` y pinta con [`Menu::draw_with`].

use std::collections::BTreeMap;

use ez_gfx::sdf::{self, FillOptions, Layer};
use ez_gfx::text::{self, Align, FloatSurface, FontStyle, TextOptions};
use ez_gfx::Rgba;

use crate::model::menu::geometry::Menu;
use crate::tokens::{Color, ColorMap, R_GROUP};

pub mod advanced;
pub mod controls;
pub mod sections;

/// Layout vertical del menú, tal y como lo devuelve `menu_layout`.
pub type Layout = BTreeMap<String, crate::layout::Rect>;

/// Etiquetas de sección y cabecera (`LABEL_X`).
pub const LABEL_X: f64 = 23.0;
/// Borde derecho del valor de un selector (`SELECT_VALUE_X1`).
pub const SELECT_VALUE_X1: f64 = 267.0;
/// Ancho máximo del valor de un selector (`SELECT_VALUE_MAX_W`).
pub const SELECT_VALUE_MAX_W: f64 = 130.0;
/// Centro horizontal del chevron del selector (`CHEVRON_CX`).
pub const CHEVRON_CX: f64 = 281.0;
/// Lado del chevron del selector (`CHEVRON_SIZE`).
pub const CHEVRON_SIZE: f64 = 16.0;
/// Centro vertical de la primera línea de una fila de texto (`TEXT_LINE1_CY`).
pub const TEXT_LINE1_CY: f64 = 17.0;
/// Ancho del cursor de edición (`TEXT_CARET_W`).
pub const TEXT_CARET_W: f64 = 1.5;
/// Alto del cursor de edición (`TEXT_CARET_H`).
pub const TEXT_CARET_H: f64 = 16.0;
/// Puntos máximos de una clave secreta (`DOT_MAX`).
pub const DOT_MAX: usize = 22;
/// Paso entre puntos de una clave secreta (`DOT_PITCH`).
pub const DOT_PITCH: f64 = 8.0;
/// Lado del icono de la cabecera (`HIDE_ICON_SIZE`).
pub const HIDE_ICON_SIZE: f64 = 16.0;
/// Escala del icono de la cabecera al pulsar (`HIDE_ICON_PRESS_SCALE`).
pub const HIDE_ICON_PRESS_SCALE: f64 = 0.92;
/// Velo del botón de la cabecera en oscuro (`HIDE_HOVER_DARK`).
pub const HIDE_HOVER_DARK: f64 = 0.08;
/// Velo del botón de la cabecera pulsado en oscuro (`HIDE_PRESS_DARK`).
pub const HIDE_PRESS_DARK: f64 = 0.14;
/// Velo del botón de la cabecera en claro (`HIDE_HOVER_LIGHT`).
pub const HIDE_HOVER_LIGHT: f64 = 0.06;
/// Velo del botón de la cabecera pulsado en claro (`HIDE_PRESS_LIGHT`).
pub const HIDE_PRESS_LIGHT: f64 = 0.10;
/// Opacidad de una fila deshabilitada (`DISABLED_OPACITY`).
pub const DISABLED_OPACITY: f64 = 0.40;
/// Grosor de las hairlines de fila (`HAIRLINE_PX`).
pub const HAIRLINE_PX: f64 = 1.0;
/// Diámetro del pulgar del medidor (`METER_THUMB`).
pub const METER_THUMB: f64 = 14.0;
/// Mezcla del blanco/negro al pasar por un botón (`BUTTON_HOVER_MIX`).
pub const BUTTON_HOVER_MIX: f64 = 0.08;
/// Escala de un botón pulsado (`BUTTON_PRESS_SCALE`).
pub const BUTTON_PRESS_SCALE: f64 = 0.96;

/// `BUTTON_VARIANTS`: `(variante, relleno, tinta)` de C-07.
pub const BUTTON_VARIANTS: [(&str, &str, &str); 4] = [
    ("primary", "MINT_BTN", "MINT_INK"),
    ("secondary", "TRACK", "TEXT"),
    ("amber", "WARM_BTN", "WARM_INK"),
    ("disabled", "TRACK", "TEXT_3"),
];

/// Radio del contenedor de grupo (`tokens.R_GROUP`).
pub const GROUP_RADIUS: f64 = R_GROUP as f64;

// Los valores numéricos usan la métrica exacta de `FS_MICRO` pero sin la
// transformación a mayúsculas del token: «1,6 s» conserva la unidad en
// minúscula (spec 7.4).
/// `FS_VALUE`: como `FS_MICRO`, sin versalitas.
pub const FS_VALUE: FontStyle = FontStyle {
    token: "FS_VALUE",
    family: text::FS_MICRO.family,
    size: text::FS_MICRO.size,
    weight: text::FS_MICRO.weight,
    tracking_em: text::FS_MICRO.tracking_em,
    uppercase: false,
};

/// `(color RGB o RGBA) -> Rgba` con alfa 255 cuando no lleva.
///
/// // DECISION: `paint::settings_window` tiene la misma conversión; se repite
/// // aquí para no atar la pintura del menú a la ventana de ajustes.
pub fn to_rgba(color: Color) -> Rgba {
    let channels = color.channels();
    Rgba::new(
        channels[0],
        channels[1],
        channels[2],
        color.alpha().unwrap_or(255),
    )
}

/// `(color RGB o RGBA) -> text::Color`.
pub fn text_color(color: Color) -> text::Color {
    match color {
        Color::Rgb(rgb) => text::Color::Rgb(rgb),
        Color::Rgba(rgba) => text::Color::Rgba(rgba),
    }
}

/// `(color RGB o RGBA) -> icons::Color`.
pub fn icon_color(color: Color) -> ez_gfx::icons::Color {
    let [r, g, b] = color.channels();
    match color.alpha() {
        None => ez_gfx::icons::Color::Rgb(r, g, b),
        Some(a) => ez_gfx::icons::Color::Rgba(r, g, b, a),
    }
}

/// `_fit_text`: trunca con «…» usando el ancho real de la tinta.
///
/// // DECISION: el `lru_cache(maxsize=512)` del Python queda fuera; las filas
/// // que lo usan miden pocas cadenas por fotograma.
pub fn fit_text(text: &str, style: FontStyle, max_width: f64, scale: f64) -> String {
    sdf::fit_text(text, max_width, |candidate| {
        text::text_size(candidate, style, scale, None, "left").map_or(0.0, |(width, _)| width)
    })
}

/// Contexto de pintura compartido por las tres capas del menú.
pub struct PaintContext<'a> {
    /// Capa destino (`float32` RGBA premultiplicada).
    pub layer: &'a mut Layer,
    /// Factor DPI (`dpi / 96`).
    pub scale: f64,
    /// Ancho extra de la ventana de ajustes.
    pub dx: f64,
    /// Tema del menú (`dark`, `light` o `auto`).
    pub theme: &'static str,
    /// Paleta del tema con las sobrescrituras de la aplicación.
    pub palette: &'a ColorMap,
}

impl<'a> PaintContext<'a> {
    /// Envuelve una capa con la escala, el ancho extra, el tema y la paleta.
    pub fn new(
        layer: &'a mut Layer,
        scale: f64,
        dx: f64,
        theme: &'static str,
        palette: &'a ColorMap,
    ) -> Self {
        Self {
            layer,
            scale,
            dx,
            theme,
            palette,
        }
    }

    /// `pal[key]`: la paleta del menú siempre trae todas las claves.
    pub fn color(&self, key: &str) -> Color {
        *self
            .palette
            .get(key)
            .expect("la paleta del menú tiene todas las claves")
    }

    /// `pal[key]` cuando la clave debe ser RGB.
    pub fn rgb(&self, key: &str) -> [u8; 3] {
        self.color(key).channels()
    }

    /// `_fill_round_rect`.
    pub fn fill_round_rect(&mut self, bounds: [f64; 4], radius: f64, color: Color, alpha: f64) {
        sdf::fill_round_rect(
            self.layer,
            bounds,
            radius,
            to_rgba(color),
            self.scale,
            alpha,
        );
    }

    /// `_fill_round_rect` con radios distintos y `replace`.
    pub fn fill_round_rect_opts(
        &mut self,
        bounds: [f64; 4],
        radius: f64,
        color: Color,
        alpha: f64,
        options: FillOptions,
    ) {
        sdf::fill_round_rect_opts(
            self.layer,
            bounds,
            radius,
            to_rgba(color),
            self.scale,
            alpha,
            options,
        );
    }

    /// `_fill_round_gradient`.
    pub fn fill_round_gradient(
        &mut self,
        bounds: [f64; 4],
        radius: f64,
        stops: &[[f32; 3]],
        alpha: f64,
    ) {
        sdf::fill_round_gradient(self.layer, bounds, radius, stops, self.scale, alpha);
    }

    /// `_stroke_round_rect`.
    pub fn stroke_round_rect(&mut self, bounds: [f64; 4], radius: f64, color: Color, alpha: f64) {
        sdf::stroke_round_rect(
            self.layer,
            bounds,
            radius,
            to_rgba(color),
            self.scale,
            alpha,
        );
    }

    /// `_fill_circle`.
    pub fn fill_circle(&mut self, cx: f64, cy: f64, diameter: f64, color: Color, alpha: f64) {
        sdf::fill_circle(
            self.layer,
            cx,
            cy,
            diameter,
            to_rgba(color),
            self.scale,
            alpha,
        );
    }

    /// `draw_icon` sobre la capa; los argumentos del HUD siempre son válidos.
    pub fn draw_icon(&mut self, name: &str, cx: f64, cy: f64, size: f64, color: Color, alpha: f64) {
        let width = self.layer.width() as usize;
        let height = self.layer.height() as usize;
        let _ = ez_gfx::icons::draw_icon(
            self.layer.as_raw_mut(),
            width,
            height,
            name,
            cx,
            cy,
            size,
            icon_color(color),
            self.scale,
            alpha,
        );
    }

    /// `_text_at`: centra la tinta en `center_y` y dibuja con `align`.
    #[allow(clippy::too_many_arguments)]
    pub fn text_at(
        &mut self,
        text: &str,
        style: FontStyle,
        color: Color,
        x: f64,
        center_y: f64,
        alpha: f64,
        align: Align,
    ) {
        if text.is_empty() || alpha <= 0.001 {
            return;
        }
        let height = text::text_size(text, style, self.scale, None, "left")
            .map_or(0.0, |(_width, height)| height);
        let width = self.layer.width() as usize;
        let rows = self.layer.height() as usize;
        let mut surface = FloatSurface::new(self.layer.as_raw_mut(), width, rows);
        let _ = text::draw_text(
            &mut surface,
            text,
            style,
            text_color(color),
            (x, center_y - height / 2.0),
            TextOptions {
                scale: self.scale,
                align,
                tracking: None,
                alpha: alpha.clamp(0.0, 1.0),
            },
        );
    }

    /// `_button_colors`: relleno y tinta de una variante C-07.
    pub fn button_colors(&self, variant: &str) -> (Color, Color) {
        let (_, fill, ink) = BUTTON_VARIANTS
            .iter()
            .find(|(name, _fill, _ink)| *name == variant)
            .expect("variante de botón conocida");
        (self.color(fill), self.color(ink))
    }
}

/// Ganchos de las filas y adornos que pinta F5-10b en `advanced.rs`.
///
/// No pinta nada por defecto: así F5-10a puede componer el marco y las filas
/// de su mitad sin depender de código que aún no existe. F5-10b implementa los
/// métodos que necesita y llama a [`Menu::draw_with`].
pub trait FrameExtras {
    /// `_draw_slider_row`.
    fn draw_slider_row(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _row_y0: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_toggle_row`.
    fn draw_toggle_row(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _center_y: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_action_row`.
    #[allow(clippy::too_many_arguments)]
    fn draw_action_row(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _row_y0: f64,
        _center_y: f64,
        _layout: &Layout,
        _now: f64,
        _scroll: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_selector_list`.
    #[allow(clippy::too_many_arguments)]
    fn draw_selector_list(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _row_y0: f64,
        _layout: &Layout,
        _now: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_hotkey_value`.
    fn draw_hotkey_value(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _row_y0: f64,
        _now: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_secret_value`.
    fn draw_secret_value(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _raw: &str,
        _editing: bool,
        _value_cy: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_copy_button`.
    fn draw_copy_button(
        &mut self,
        _menu: &Menu,
        _ctx: &mut PaintContext<'_>,
        _key: &str,
        _row_y0: f64,
        _now: f64,
        _alpha: f64,
    ) {
    }

    /// `_draw_footer`.
    fn draw_footer(
        &mut self,
        _menu: &mut Menu,
        _ctx: &mut PaintContext<'_>,
        _layout: &Layout,
        _now: f64,
    ) {
    }

    /// `_draw_scroll`: se pinta sobre la ventana, no sobre las capas.
    fn draw_scroll(
        &mut self,
        _menu: &Menu,
        _buf: &mut Layer,
        _palette: &ColorMap,
        _layout: &Layout,
        _now: f64,
        _scroll: f64,
    ) {
    }
}

/// Implementación vacía de [`FrameExtras`] para la primera mitad.
#[derive(Default)]
pub struct NoExtras;

impl FrameExtras for NoExtras {}

#[cfg(test)]
mod tests;
