//! Port of `src/ezagent/voice/hud/menu.py 2724-3100` (F5-10a): filas de
//! selector, medidor, paleta, muestra de color y texto.
//!
//! Las filas de deslizador, interruptor y acción, el valor de un atajo, el
//! secreto y el botón de copiar son de F5-10b: aquí solo se llaman los ganchos
//! de [`FrameExtras`].

use ez_gfx::text::{self, Align, FS_BODY, FS_ROW, FS_SECONDARY};

use crate::model::menu::geometry::Menu;
use crate::model::menu::rects::{
    METER_TRACK_H, METER_TRACK_Y, ROW_X0, ROW_X1, SLIDER_TRACK_X0, SLIDER_TRACK_X1, TEXT_LINE2_CY,
};
use crate::model::settings::{
    SettingValue, COLOR_KEYS, LABELS, METER_DB_CEIL, METER_DB_FLOOR, SECRET_KEYS,
};
use crate::paint::flow;
use crate::paint::hotkey_field::HOTKEY_KEYS;
use crate::tokens::{Color, ColorMap};

use super::{
    fit_text, FrameExtras, PaintContext, CHEVRON_CX, CHEVRON_SIZE, DISABLED_OPACITY, DOT_MAX,
    DOT_PITCH, FS_VALUE, SELECT_VALUE_MAX_W, SELECT_VALUE_X1, TEXT_CARET_H, TEXT_CARET_W,
    TEXT_LINE1_CY,
};

/// `label(key)` de `settings.LABELS`; el menú solo pinta claves conocidas.
fn label(key: &str) -> &'static str {
    LABELS.get(key).copied().expect("la fila tiene etiqueta")
}

/// `str(value or "")`.
fn str_or_empty(value: Option<&SettingValue>) -> String {
    match value {
        Some(value) if value.truthy() => value.python_str(),
        _ => String::new(),
    }
}

/// `float(value or 0.0)` para el umbral del medidor.
fn float_or_zero(value: Option<&SettingValue>) -> f64 {
    match value {
        None => 0.0,
        Some(SettingValue::Bool(flag)) => {
            if *flag {
                1.0
            } else {
                0.0
            }
        }
        Some(SettingValue::Int(number)) => *number as f64,
        Some(SettingValue::Float(number)) => *number,
        // DECISION: un texto no numérico cae a 0.0 en vez del `ValueError` de
        // `float()`; `min_rms` se normaliza como número al cargar los ajustes.
        Some(SettingValue::Text(text)) => text.parse().unwrap_or(0.0),
        Some(SettingValue::None) => 0.0,
    }
}

/// `_draw_select_row`.
#[allow(clippy::too_many_arguments)]
pub fn draw_select_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    _row_y0: f64,
    center_y: f64,
    alpha: f64,
    disabled: bool,
) {
    let dim = if disabled { DISABLED_OPACITY } else { 1.0 };
    draw_select_label(ctx, key, center_y, alpha * dim);
    draw_select_value(menu, ctx, key, center_y, alpha, dim);
    draw_select_chevrons(menu, ctx, key, center_y, alpha, dim);
}

/// Etiqueta de la fila de selector, atenuada si está deshabilitada.
fn draw_select_label(ctx: &mut PaintContext<'_>, key: &str, center_y: f64, label_alpha: f64) {
    let name = ctx.color("TEXT");
    ctx.text_at(
        label(key),
        FS_ROW,
        name,
        ROW_X0,
        center_y,
        label_alpha,
        Align::Left,
    );
}

/// Valor actual, a la derecha; el micrófono vacío se marca en rojo.
fn draw_select_value(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    alpha: f64,
    dim: f64,
) {
    let value = menu
        .settings
        .value_label(key)
        .expect("clave de ajuste conocida");
    let (value_color, value_alpha) =
        if key == "input_device_index" && menu.settings.devices().is_empty() {
            (ctx.color("ERR"), alpha)
        } else {
            (ctx.color("TEXT_2"), alpha * dim)
        };
    let fitted = fit_text(&value, FS_BODY, SELECT_VALUE_MAX_W + menu.dx, menu.scale);
    ctx.text_at(
        &fitted,
        FS_BODY,
        value_color,
        SELECT_VALUE_X1 + menu.dx,
        center_y,
        value_alpha,
        Align::Right,
    );
}

/// Chevron que gira de abajo a arriba al abrir el selector.
fn draw_select_chevrons(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    alpha: f64,
    dim: f64,
) {
    let fraction = if menu.selector_key.as_deref() == Some(key) {
        menu.chevron
            .get(key)
            .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let chevron_alpha = alpha * dim;
    let chevron = ctx.color("TEXT_3");
    if fraction < 0.999 {
        ctx.draw_icon(
            "chevron-down",
            CHEVRON_CX + menu.dx,
            center_y,
            CHEVRON_SIZE,
            chevron,
            chevron_alpha * (1.0 - fraction),
        );
    }
    if fraction > 0.001 {
        ctx.draw_icon(
            "chevron-up",
            CHEVRON_CX + menu.dx,
            center_y,
            CHEVRON_SIZE,
            chevron,
            chevron_alpha * fraction,
        );
    }
}

/// `_meter_fraction`: convierte un RMS lineal a la posición dBFS del medidor.
pub fn meter_fraction(value: f64) -> f64 {
    let rms = value.max(0.0);
    let db = if rms <= 0.0 {
        METER_DB_FLOOR
    } else {
        20.0 * rms.log10()
    };
    ((db - METER_DB_FLOOR) / (METER_DB_CEIL - METER_DB_FLOOR)).clamp(0.0, 1.0)
}

/// `_draw_meter_row`.
pub fn draw_meter_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    _key: &str,
    row_y0: f64,
    alpha: f64,
    _now: f64,
) {
    let threshold = float_or_zero(menu.settings.get("min_rms"));
    let threshold_db = if threshold > 0.0 {
        20.0 * threshold.log10()
    } else {
        METER_DB_FLOOR
    };
    draw_meter_labels(ctx, row_y0, threshold_db, alpha);
    draw_meter_track(menu, ctx, row_y0, alpha, threshold);
}

/// Etiqueta y umbral en dB de la fila del medidor.
fn draw_meter_labels(ctx: &mut PaintContext<'_>, row_y0: f64, threshold_db: f64, alpha: f64) {
    let name = ctx.color("TEXT");
    ctx.text_at(
        label("input_meter"),
        FS_ROW,
        name,
        ROW_X0,
        row_y0 + 17.0,
        alpha,
        Align::Left,
    );
    let value = format!("{threshold_db:.0} dB");
    let value_color = ctx.color("TEXT_2");
    ctx.text_at(
        &value,
        FS_VALUE,
        value_color,
        ROW_X1 + ctx.dx,
        row_y0 + 17.0,
        alpha,
        Align::Right,
    );
}

/// Pista, nivel en vivo y pulgar del umbral.
fn draw_meter_track(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    row_y0: f64,
    alpha: f64,
    threshold: f64,
) {
    let x0 = SLIDER_TRACK_X0;
    let x1 = SLIDER_TRACK_X1 + menu.dx;
    let cy = row_y0 + METER_TRACK_Y + METER_TRACK_H / 2.0;
    let track = ctx.color("TRACK");
    ctx.fill_round_rect(
        [x0, cy - METER_TRACK_H / 2.0, x1, cy + METER_TRACK_H / 2.0],
        METER_TRACK_H / 2.0,
        track,
        alpha,
    );
    let level = meter_fraction(menu.meter_level);
    if level > 0.0 {
        let mint = ctx.color("MINT_BTN");
        ctx.fill_round_rect(
            [
                x0,
                cy - METER_TRACK_H / 2.0,
                x0 + (x1 - x0) * level,
                cy + METER_TRACK_H / 2.0,
            ],
            METER_TRACK_H / 2.0,
            mint,
            alpha,
        );
    }
    let threshold_fraction = meter_fraction(threshold);
    let thumb_cx = x0 + (x1 - x0) * threshold_fraction;
    let thumb_color = if menu.theme == "light" {
        Color::Rgb([255, 255, 255])
    } else {
        ctx.color("TEXT")
    };
    ctx.fill_circle(thumb_cx, cy, super::METER_THUMB, thumb_color, alpha);
    if menu.theme == "light" {
        let line = ctx.color("LINE");
        let radius = super::METER_THUMB / 2.0;
        ctx.stroke_round_rect(
            [
                thumb_cx - radius,
                cy - radius,
                thumb_cx + radius,
                cy + radius,
            ],
            radius,
            line,
            alpha,
        );
    }
}

/// `_palette_stops`: fondo, principal, secundario y acento de cada tema.
pub fn palette_stops(menu: &Menu, value: &str) -> [[f32; 3]; 4] {
    let mut value = value;
    if value == "custom" {
        let [primary, secondary, accent, background] = menu.settings.custom_colors();
        if let Some(palette) = flow::custom_palette(&primary, &secondary, &accent, &background) {
            return [palette[0], palette[1], palette[2], palette[3]];
        }
        value = "light";
    }
    let palette = flow::colors_for(if value == "light" { "light" } else { "dark" }, "idle");
    [palette[0], palette[1], palette[2], palette[3]]
}

/// `_draw_palette_row`.
pub fn draw_palette_row(menu: &Menu, ctx: &mut PaintContext<'_>, row_y0: f64, alpha: f64) {
    let active = str_or_empty(menu.settings.get("theme"));
    for (value, name, pill) in menu.palette_pills(row_y0) {
        let stops = palette_stops(menu, value);
        ctx.fill_round_gradient([pill.x0, pill.y0, pill.x1, pill.y1], 12.0, &stops, alpha);
        if value == active {
            for inset in [-3.0, -2.0] {
                let mint = ctx.color("MINT");
                ctx.stroke_round_rect(
                    [
                        pill.x0 + inset,
                        pill.y0 + inset,
                        pill.x1 - inset,
                        pill.y1 - inset,
                    ],
                    12.0 - inset,
                    mint,
                    alpha,
                );
            }
        }
        let color = if value == active {
            ctx.color("MINT")
        } else {
            ctx.color("TEXT_2")
        };
        ctx.text_at(
            name,
            FS_SECONDARY,
            color,
            (pill.x0 + pill.x1) / 2.0,
            row_y0 + 52.0,
            alpha,
            Align::Center,
        );
    }
}

/// `_draw_color_swatch`: muestra redonda del color de la fila.
pub fn draw_color_swatch(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    alpha: f64,
) {
    let editing = menu.editing_key.as_deref() == Some(key);
    let raw = if editing {
        menu.edit_buffer.clone()
    } else {
        str_or_empty(menu.settings.get(key))
    };
    let stored = str_or_empty(menu.settings.get(key));
    let Some(rgb) = flow::parse_hex(&raw).or_else(|| flow::parse_hex(&stored)) else {
        return;
    };
    let cx = ROW_X1 + menu.dx - 11.0;
    let cy = row_y0 + 26.0;
    let line = ctx.color("LINE");
    ctx.fill_circle(cx, cy, 24.0, line, alpha);
    let color = Color::Rgb([rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]);
    ctx.fill_circle(cx, cy, 22.0, color, alpha);
}

/// `pal["TEXT"]` al editar; si no, `TEXT_2` con valor y `TEXT_3` vacío.
pub(super) fn value_color(palette: &ColorMap, editing: bool, raw: &str) -> Color {
    let key = if editing {
        "TEXT"
    } else if raw.is_empty() {
        "TEXT_3"
    } else {
        "TEXT_2"
    };
    *palette
        .get(key)
        .expect("la paleta del menú tiene todas las claves")
}

/// `_draw_text_row`: etiqueta, valor, muestra de color y cursor de edición.
pub fn draw_text_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    alpha: f64,
    now: f64,
    extras: &mut dyn FrameExtras,
) {
    draw_text_label(menu, ctx, key, row_y0, alpha);
    if HOTKEY_KEYS.contains(&key) {
        extras.draw_hotkey_value(menu, ctx, key, row_y0, now, alpha);
        return;
    }
    let secret = SECRET_KEYS.contains(&key);
    let (raw, display, editing) = text_row_content(menu, key);
    draw_text_value(
        menu, ctx, key, &raw, &display, editing, secret, row_y0, alpha, now, extras,
    );
    if COLOR_KEYS.contains(&key) {
        draw_color_swatch(menu, ctx, key, row_y0, alpha);
    }
    if editing {
        draw_text_caret(menu, ctx, &raw, secret, row_y0 + TEXT_LINE2_CY, alpha, now);
    }
}

/// Primera línea: la etiqueta o el error de edición, truncado al ancho.
fn draw_text_label(menu: &Menu, ctx: &mut PaintContext<'_>, key: &str, row_y0: f64, alpha: f64) {
    let error = menu
        .edit_error
        .as_ref()
        .filter(|(error_key, _message)| error_key == key)
        .map_or("", |(_key, message)| message.as_str());
    let label_text = fit_text(
        if error.is_empty() { label(key) } else { error },
        FS_ROW,
        ROW_X1 + menu.dx - ROW_X0,
        menu.scale,
    );
    let label_color = if error.is_empty() {
        ctx.color("TEXT")
    } else {
        ctx.color("ERR")
    };
    ctx.text_at(
        &label_text,
        FS_ROW,
        label_color,
        ROW_X0,
        row_y0 + TEXT_LINE1_CY,
        alpha,
        Align::Left,
    );
}

/// `(raw, display, editing)` de la fila: el búfer al editar, el valor si no.
fn text_row_content(menu: &Menu, key: &str) -> (String, String, bool) {
    let editing = menu.editing_key.as_deref() == Some(key);
    if editing {
        let raw = menu.edit_buffer.clone();
        (raw.clone(), raw, true)
    } else {
        let raw = str_or_empty(menu.settings.get(key));
        let display = menu
            .settings
            .format_value(key, &SettingValue::Text(raw.clone()))
            .expect("clave de ajuste conocida");
        (raw, display, false)
    }
}

/// Segunda línea: fondo de edición, valor visible o secreto y botón de copiar.
#[allow(clippy::too_many_arguments)]
fn draw_text_value(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    raw: &str,
    display: &str,
    editing: bool,
    secret: bool,
    row_y0: f64,
    alpha: f64,
    now: f64,
    extras: &mut dyn FrameExtras,
) {
    let value_cy = row_y0 + TEXT_LINE2_CY;
    let max_w = (ROW_X1 + menu.dx) - ROW_X0;
    if editing {
        let track = ctx.color("TRACK");
        ctx.fill_round_rect(
            [
                ROW_X0 - 6.0,
                value_cy - 11.0,
                ROW_X1 + menu.dx + 6.0,
                value_cy + 11.0,
            ],
            6.0,
            track,
            alpha,
        );
    }
    if secret {
        extras.draw_secret_value(menu, ctx, raw, editing, value_cy, alpha);
        extras.draw_copy_button(menu, ctx, key, row_y0, now, alpha);
    } else {
        let color = value_color(ctx.palette, editing, raw);
        let shown = if display.is_empty() { " " } else { display };
        let fitted = fit_text(shown, FS_BODY, max_w, menu.scale);
        ctx.text_at(
            &fitted,
            FS_BODY,
            color,
            ROW_X0,
            value_cy,
            alpha,
            Align::Left,
        );
    }
}

/// Cursor de edición, parpadeante a 1 Hz; en secretos avanza por puntos.
fn draw_text_caret(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    raw: &str,
    secret: bool,
    value_cy: f64,
    alpha: f64,
    now: f64,
) {
    if ((now - menu.edit_t0) * 2.0) as i64 % 2 != 0 {
        return;
    }
    let cursor = menu.edit_cursor.min(raw.chars().count());
    let prefix: String = raw.chars().take(cursor).collect();
    let width = if secret {
        prefix.chars().count().min(DOT_MAX) as f64 * DOT_PITCH
    } else {
        let max_w = (ROW_X1 + menu.dx) - ROW_X0;
        max_w.min(
            text::text_size(&prefix, FS_BODY, menu.scale, None, "left")
                .map_or(0.0, |(width, _height)| width),
        )
    };
    let mint = ctx.color("MINT");
    ctx.fill_round_rect(
        [
            ROW_X0 + width,
            value_cy - TEXT_CARET_H / 2.0,
            ROW_X0 + width + TEXT_CARET_W,
            value_cy + TEXT_CARET_H / 2.0,
        ],
        TEXT_CARET_W / 2.0,
        mint,
        alpha,
    );
}
