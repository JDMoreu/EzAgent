//! Port of `src/ezagent/voice/hud/menu.py 3090-3872` (F5-10b): teclas, secreto,
//! deslizadores, interruptores, acciones, selectores, pie y desplazamiento.
//!
//! Implementa [`FrameExtras`] con las filas y adornos que F5-10a dejó como
//! ganchos. La pintura se reparte en tres submódulos: `rows` (filas y lista de
//! selectores), `footer` (pie y su botón) y `scroll` (degradados e indicador).

use ez_gfx::sdf::Layer;

use crate::model::menu::geometry::Menu;
use crate::model::menu::rects::BUTTON_H;
use crate::model::settings::SettingValue;
use crate::tokens::{Color, ColorMap, Rect};

use super::{FrameExtras, Layout, PaintContext};

mod footer;
mod keycaps;
mod rows;
mod scroll;
#[cfg(test)]
mod tests;

/// Diámetro del icono del botón «copiar» (`COPY_ICON`).
pub const COPY_ICON: f64 = 18.0;
/// Radio de un punto de clave secreta (`DOT_R`).
pub const DOT_R: f64 = 2.4;
/// Alto de una tecla dibujada (`KEYCAP_H`).
pub const KEYCAP_H: f64 = 22.0;
/// Margen interior horizontal de una tecla (`KEYCAP_PAD`).
pub const KEYCAP_PAD: f64 = 8.0;
/// Hueco entre teclas (`KEYCAP_GAP`).
pub const KEYCAP_GAP: f64 = 4.0;
/// Radio de las esquinas de una tecla (`6.0` del Python).
pub const KEYCAP_RADIUS: f64 = 6.0;
/// Origen vertical de la primera línea de un deslizador (`SLIDER_LINE1_TOP`).
pub const SLIDER_LINE1_TOP: f64 = 9.0;
/// Alto de la primera línea de un deslizador (`SLIDER_LINE1_H`).
pub const SLIDER_LINE1_H: f64 = 16.0;
/// Pulgar de un deslizador al pasar por su fila (`SLIDER_THUMB_HOVER`).
pub const SLIDER_THUMB_HOVER: f64 = 16.0;
/// Pulgar de un deslizador mientras se arrastra (`SLIDER_THUMB_DRAG`).
pub const SLIDER_THUMB_DRAG: f64 = 18.0;
/// Borde izquierdo del interruptor (`TOGGLE_X0`).
pub const TOGGLE_X0: f64 = 253.0;
/// Ancho de la pista del interruptor (`TOGGLE_W`).
pub const TOGGLE_W: f64 = 36.0;
/// Alto de la pista del interruptor (`TOGGLE_H`).
pub const TOGGLE_H: f64 = 20.0;
/// Diámetro del pulgar del interruptor (`TOGGLE_THUMB`).
pub const TOGGLE_THUMB: f64 = 16.0;
/// Margen interior del pulgar del interruptor (`TOGGLE_PAD`).
pub const TOGGLE_PAD: f64 = 2.0;
/// Origen horizontal del texto de la lista de un selector (`LIST_TEXT_X`).
pub const LIST_TEXT_X: f64 = 43.0;
/// Ancho máximo del texto de una opción (`LIST_TEXT_MAX_W`).
pub const LIST_TEXT_MAX_W: f64 = 222.0;
/// Centro horizontal del check de la opción elegida (`LIST_CHECK_CX`).
pub const LIST_CHECK_CX: f64 = 281.0;
/// Lado del check de la opción elegida (`LIST_CHECK_SIZE`).
pub const LIST_CHECK_SIZE: f64 = 16.0;
/// Origen horizontal del estado del pie (`FOOTER_STATUS_X`).
pub const FOOTER_STATUS_X: f64 = crate::model::menu::rects::SAVE_BUTTON_X1 + 12.0;
/// Diámetro del punto de «cambios sin guardar» (`FOOTER_DOT`).
pub const FOOTER_DOT: f64 = 6.0;
/// Separación entre el punto y el texto del pie (`FOOTER_DOT_GAP`).
pub const FOOTER_DOT_GAP: f64 = 6.0;
/// Tiempo que el check de «Guardar» queda visible (`SAVE_CHECK_MS`).
pub const SAVE_CHECK_MS: f64 = 600.0;
/// Fundido del check de «Guardar» (`SAVE_FADE_MS`).
pub const SAVE_FADE_MS: f64 = 120.0;
/// Ancho del indicador de scroll (`SCROLL_INDICATOR_W`).
pub const SCROLL_INDICATOR_W: f64 = 3.0;
/// Origen horizontal del indicador de scroll (`SCROLL_INDICATOR_X`).
pub const SCROLL_INDICATOR_X: f64 = 304.0;
/// Alfa máxima del indicador de scroll (`SCROLL_INDICATOR_ALPHA`).
pub const SCROLL_INDICATOR_ALPHA: f64 = 0.40;
/// Alfa del indicador de scroll en reposo (`SCROLL_INDICATOR_IDLE_ALPHA`).
pub const SCROLL_INDICATOR_IDLE_ALPHA: f64 = 0.12;
/// Alto mínimo del indicador de scroll (`SCROLL_INDICATOR_MIN_H`).
pub const SCROLL_INDICATOR_MIN_H: f64 = 24.0;
/// Alto de los degradados de los bordes del scroll (`SCROLL_GRADIENT_H`).
pub const SCROLL_GRADIENT_H: f64 = 12.0;
/// Alto mínimo de una barra de la mini onda (`TEST_WAVE_MIN_H`).
pub const TEST_WAVE_MIN_H: f64 = 3.0;
/// Alto máximo de una barra de la mini onda (`TEST_WAVE_MAX_H`).
pub const TEST_WAVE_MAX_H: f64 = 10.0;
/// Nivel mínimo de una barra de la mini onda (`TEST_WAVE_LEVEL_BASE`).
pub const TEST_WAVE_LEVEL_BASE: f64 = 0.35;
/// Recorrido del nivel de una barra (`TEST_WAVE_LEVEL_SPAN`).
pub const TEST_WAVE_LEVEL_SPAN: f64 = 0.65;
/// Velocidad de la fase de la mini onda (`TEST_WAVE_PHASE_SPEED`).
pub const TEST_WAVE_PHASE_SPEED: f64 = 9.0;
/// Desfase entre barras de la mini onda (`TEST_WAVE_PHASE_STEP`).
pub const TEST_WAVE_PHASE_STEP: f64 = 1.3;
/// Radio de un botón capsular (`BUTTON_RADIUS` = `BUTTON_H / 2`).
pub const BUTTON_RADIUS: f64 = BUTTON_H / 2.0;
/// Texto del botón de una acción destructiva armada (`ARMED_TEXT`).
pub const ARMED_TEXT: &str = "¿Seguro?";
/// `ACTION_BUTTON_TEXT`: rótulo del botón de cada acción.
pub const ACTION_BUTTON_TEXT: [(&str, &str); 2] =
    [("repeat_last", "Repetir"), ("new_conversation", "Borrar")];

/// `float(value or 0.0)` de una fila numérica.
///
/// // DECISION: un texto no numérico cae a `0.0` en vez del `ValueError` de
/// // `float()`; los ajustes numéricos ya se normalizan al cargarse.
fn float_value(value: Option<&SettingValue>) -> f64 {
    match value {
        None => 0.0,
        Some(SettingValue::Bool(flag)) => f64::from(u8::from(*flag)),
        Some(SettingValue::Int(number)) => *number as f64,
        Some(SettingValue::Float(number)) => *number,
        Some(SettingValue::Text(text)) => text.parse().unwrap_or(0.0),
        Some(SettingValue::None) => 0.0,
    }
}

/// `str(value or "")` de una fila de texto.
fn str_or_empty(value: Option<&SettingValue>) -> String {
    match value {
        Some(value) if value.truthy() => value.python_str(),
        _ => String::new(),
    }
}

/// `_scaled_rect`: encoge un rectángulo desde su centro.
pub fn scaled_rect(rect: Rect, factor: f64) -> Rect {
    let (cx, cy) = rect.center();
    let half_w = rect.width() * factor / 2.0;
    let half_h = rect.height() * factor / 2.0;
    Rect::new(cx - half_w, cy - half_h, cx + half_w, cy + half_h)
}

/// Fondo de un botón al pasar el puntero: blanco en oscuro, negro en claro.
fn hover_target(theme: &str) -> Color {
    if theme == "light" {
        Color::Rgb([0, 0, 0])
    } else {
        Color::Rgb([255, 255, 255])
    }
}

/// Implementación completa de [`FrameExtras`] (F5-10b).
#[derive(Default)]
pub struct Advanced;

impl FrameExtras for Advanced {
    fn draw_slider_row(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        row_y0: f64,
        alpha: f64,
    ) {
        rows::draw_slider_row(menu, ctx, key, row_y0, alpha);
    }

    fn draw_toggle_row(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        center_y: f64,
        alpha: f64,
    ) {
        rows::draw_toggle_row(menu, ctx, key, center_y, alpha);
    }

    fn draw_action_row(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        _row_y0: f64,
        center_y: f64,
        _layout: &Layout,
        now: f64,
        _scroll: f64,
        alpha: f64,
    ) {
        rows::draw_action_row(menu, ctx, key, center_y, now, alpha);
    }

    fn draw_selector_list(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        row_y0: f64,
        _layout: &Layout,
        now: f64,
        alpha: f64,
    ) {
        rows::draw_selector_list(menu, ctx, key, row_y0, now, alpha);
    }

    fn draw_hotkey_value(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        row_y0: f64,
        now: f64,
        alpha: f64,
    ) {
        keycaps::draw_hotkey_value(menu, ctx, key, row_y0, now, alpha);
    }

    fn draw_secret_value(
        &mut self,
        _menu: &Menu,
        ctx: &mut PaintContext<'_>,
        raw: &str,
        editing: bool,
        value_cy: f64,
        alpha: f64,
    ) {
        keycaps::draw_secret_value(ctx, raw, editing, value_cy, alpha);
    }

    fn draw_copy_button(
        &mut self,
        menu: &Menu,
        ctx: &mut PaintContext<'_>,
        key: &str,
        row_y0: f64,
        now: f64,
        alpha: f64,
    ) {
        keycaps::draw_copy_button(menu, ctx, key, row_y0, now, alpha);
    }

    fn draw_footer(
        &mut self,
        menu: &mut Menu,
        ctx: &mut PaintContext<'_>,
        layout: &Layout,
        now: f64,
    ) {
        footer::draw_footer(menu, ctx, layout, now);
    }

    fn draw_scroll(
        &mut self,
        menu: &Menu,
        buf: &mut Layer,
        palette: &ColorMap,
        layout: &Layout,
        now: f64,
        scroll: f64,
    ) {
        scroll::draw_scroll(menu, buf, palette, layout, now, scroll);
    }
}
