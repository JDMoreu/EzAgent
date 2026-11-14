//! Filas y adornos de F5-10b: teclas, atajos, secretos, botón de copiar,
//! deslizadores, interruptores, acciones y lista de selectores.

use ez_gfx::text::{Align, FS_BODY, FS_ROW, FS_SECONDARY};

use crate::anim::lerp_rgb;
use crate::layout;
use crate::model::menu::geometry::{Menu, LIST_ITEM_H, LIST_VIEW_H};
use crate::model::menu::rects::{
    BUTTON_H, BUTTON_MIN_W, GROUP_X0, GROUP_X1, ROW_X0, ROW_X1, SLIDER_TRACK_H, SLIDER_TRACK_X0,
    SLIDER_TRACK_X1, TEST_BUTTON_X1, TEST_WAVE_BARS, TEST_WAVE_BAR_W, TEST_WAVE_GAP,
    TEST_WAVE_GAP_TEXT,
};
use crate::model::settings::{LABELS, SLIDERS};
use crate::tokens::{Color, Rect};

use super::super::{fit_text, PaintContext, BUTTON_HOVER_MIX, BUTTON_PRESS_SCALE, FS_VALUE};
use super::{
    float_value, hover_target, scaled_rect, ACTION_BUTTON_TEXT, ARMED_TEXT, BUTTON_RADIUS,
    LIST_CHECK_CX, LIST_CHECK_SIZE, LIST_TEXT_MAX_W, LIST_TEXT_X, SLIDER_LINE1_H, SLIDER_LINE1_TOP,
    SLIDER_THUMB_DRAG, TEST_WAVE_LEVEL_BASE, TEST_WAVE_LEVEL_SPAN, TEST_WAVE_MAX_H,
    TEST_WAVE_MIN_H, TEST_WAVE_PHASE_SPEED, TEST_WAVE_PHASE_STEP, TOGGLE_H, TOGGLE_PAD,
    TOGGLE_THUMB, TOGGLE_W, TOGGLE_X0,
};

/// `label(key)` de `settings.LABELS`; el menú solo pinta claves conocidas.
fn label(key: &str) -> &'static str {
    LABELS.get(key).copied().expect("la fila tiene etiqueta")
}

/// `_draw_slider_row`: etiqueta y valor arriba, pista y pulgar abajo.
pub fn draw_slider_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    alpha: f64,
) {
    draw_slider_header(menu, ctx, key, row_y0, alpha);
    draw_slider_track(menu, ctx, key, row_y0, alpha);
}

/// Etiqueta y valor de un deslizador, en su primera línea.
fn draw_slider_header(menu: &Menu, ctx: &mut PaintContext<'_>, key: &str, row_y0: f64, alpha: f64) {
    let line_cy = row_y0 + SLIDER_LINE1_TOP + SLIDER_LINE1_H / 2.0;
    let name = ctx.color("TEXT");
    ctx.text_at(
        label(key),
        FS_ROW,
        name,
        ROW_X0,
        line_cy,
        alpha,
        Align::Left,
    );
    let active = menu
        .slider_color
        .get(key)
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let value_color = lerp_rgb(ctx.rgb("TEXT_2"), ctx.rgb("MINT"), active);
    let value = menu.settings.format(key).expect("clave de ajuste conocida");
    ctx.text_at(
        &value,
        FS_VALUE,
        Color::Rgb(value_color),
        ROW_X1 + menu.dx,
        line_cy,
        alpha,
        Align::Right,
    );
}

/// Pista, relleno hasta el valor y pulgar del deslizador.
fn draw_slider_track(menu: &Menu, ctx: &mut PaintContext<'_>, key: &str, row_y0: f64, alpha: f64) {
    let track_cy = row_y0 + crate::model::menu::rects::SLIDER_LINE2_CY;
    let track = ctx.color("TRACK");
    ctx.fill_round_rect(
        [
            SLIDER_TRACK_X0,
            track_cy - SLIDER_TRACK_H / 2.0,
            SLIDER_TRACK_X1 + menu.dx,
            track_cy + SLIDER_TRACK_H / 2.0,
        ],
        SLIDER_TRACK_H / 2.0,
        track,
        alpha,
    );
    let spec = SLIDERS.get(key).expect("la fila es un deslizador");
    let value = float_value(menu.settings.get(key));
    let span = spec.maximum - spec.minimum;
    let fraction = if span <= 0.0 {
        0.0
    } else {
        (value - spec.minimum) / span
    };
    let fraction = fraction.clamp(0.0, 1.0);
    let width = (SLIDER_TRACK_X1 + menu.dx) - SLIDER_TRACK_X0;
    let thumb_cx = SLIDER_TRACK_X0 + fraction * width;
    if thumb_cx > SLIDER_TRACK_X0 + 0.01 {
        let mint = ctx.color("MINT_BTN");
        ctx.fill_round_rect(
            [
                SLIDER_TRACK_X0,
                track_cy - SLIDER_TRACK_H / 2.0,
                thumb_cx,
                track_cy + SLIDER_TRACK_H / 2.0,
            ],
            SLIDER_TRACK_H / 2.0,
            mint,
            alpha,
        );
    }
    draw_slider_thumb(menu, ctx, key, track_cy, thumb_cx, alpha);
}

/// Pulgar del deslizador, con anillo en el tema claro.
fn draw_slider_thumb(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    track_cy: f64,
    thumb_cx: f64,
    alpha: f64,
) {
    let rest = crate::model::menu::geometry::SLIDER_THUMB_REST;
    let diameter = SLIDER_THUMB_DRAG.min(
        menu.thumb
            .get(key)
            .map_or(rest, |tween| tween.value)
            .max(rest),
    );
    let radius = diameter / 2.0;
    let thumb_color = if menu.theme == "light" {
        Color::Rgb([255, 255, 255])
    } else {
        ctx.color("TEXT")
    };
    ctx.fill_circle(thumb_cx, track_cy, diameter, thumb_color, alpha);
    if menu.theme == "light" {
        let line = ctx.color("LINE");
        ctx.stroke_round_rect(
            [
                thumb_cx - radius,
                track_cy - radius,
                thumb_cx + radius,
                track_cy + radius,
            ],
            radius,
            line,
            alpha,
        );
    }
}

/// `_draw_toggle_row`: etiqueta, nota de ajustes avanzados y pista con pulgar.
pub fn draw_toggle_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    alpha: f64,
) {
    let name = ctx.color("TEXT");
    ctx.text_at(
        label(key),
        FS_ROW,
        name,
        ROW_X0,
        center_y,
        alpha,
        Align::Left,
    );
    if key == "show_advanced" {
        draw_advanced_note(menu, ctx, center_y, alpha);
    }
    draw_toggle_track(menu, ctx, key, center_y, alpha);
}

/// Nota «16 visibles» / «+16 ajustes» a la izquierda del interruptor.
fn draw_advanced_note(menu: &Menu, ctx: &mut PaintContext<'_>, center_y: f64, alpha: f64) {
    let hidden = layout::ADVANCED_ROWS.len();
    let note = if menu
        .settings
        .get("show_advanced")
        .is_some_and(|value| value.truthy())
    {
        format!("{hidden} visibles")
    } else {
        format!("+{hidden} ajustes")
    };
    let dim = ctx.color("TEXT_3");
    ctx.text_at(
        &note,
        FS_VALUE,
        dim,
        (TOGGLE_X0 + menu.dx) - 12.0,
        center_y,
        alpha,
        Align::Right,
    );
}

/// Pista y pulgar de un interruptor.
fn draw_toggle_track(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    alpha: f64,
) {
    let color_k = menu
        .toggle_color
        .get(key)
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let pos = menu
        .toggle_pos
        .get(key)
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let track_color = lerp_rgb(ctx.rgb("TRACK"), ctx.rgb("MINT_BTN"), color_k);
    let track_y0 = center_y - TOGGLE_H / 2.0;
    ctx.fill_round_rect(
        [
            TOGGLE_X0 + menu.dx,
            track_y0,
            TOGGLE_X0 + menu.dx + TOGGLE_W,
            track_y0 + TOGGLE_H,
        ],
        TOGGLE_H / 2.0,
        Color::Rgb(track_color),
        alpha,
    );
    let thumb_color = lerp_rgb(ctx.rgb("TEXT_2"), ctx.rgb("MINT_INK"), color_k);
    let travel = TOGGLE_W - TOGGLE_PAD * 2.0 - TOGGLE_THUMB;
    let thumb_cx = (TOGGLE_X0 + menu.dx) + TOGGLE_PAD + TOGGLE_THUMB / 2.0 + pos * travel;
    ctx.fill_circle(
        thumb_cx,
        center_y,
        TOGGLE_THUMB,
        Color::Rgb(thumb_color),
        alpha,
    );
}

/// `_draw_action_row`: la fila de «Probar» tiene su propio botón animado.
pub fn draw_action_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    now: f64,
    alpha: f64,
) {
    if key != "test_voice" {
        draw_generic_action_row(menu, ctx, key, center_y, alpha, menu.is_armed(key, now));
        return;
    }
    draw_test_button(menu, ctx, center_y, now, alpha);
}

/// Rótulo y botón «Probar», que se ensancha mientras se prueba la voz.
fn draw_test_button(menu: &Menu, ctx: &mut PaintContext<'_>, center_y: f64, now: f64, alpha: f64) {
    let name = ctx.color("TEXT");
    ctx.text_at(
        label("test_voice"),
        FS_ROW,
        name,
        ROW_X0,
        center_y,
        alpha,
        Align::Left,
    );
    let testing = menu.testing_t.value.clamp(0.0, 1.0);
    let width = menu.test_width();
    let rect = Rect::new(
        (TEST_BUTTON_X1 + menu.dx) - width,
        center_y - BUTTON_H / 2.0,
        TEST_BUTTON_X1 + menu.dx,
        center_y + BUTTON_H / 2.0,
    );
    let hover = menu
        .button_hover
        .get("test")
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0))
        * (1.0 - testing);
    let press = menu
        .press
        .get("test")
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let variant = if testing > 0.01 {
        "disabled"
    } else {
        "secondary"
    };
    let factor = 1.0 - (1.0 - BUTTON_PRESS_SCALE) * press;
    let draw_rect = scaled_rect(rect, factor);
    let (fill, text_color) = button_fill(menu, ctx, variant, hover);
    ctx.fill_round_rect(
        [draw_rect.x0, draw_rect.y0, draw_rect.x1, draw_rect.y1],
        BUTTON_RADIUS * factor,
        fill,
        alpha,
    );
    if menu.theme == "light" && variant == "secondary" {
        let line = ctx.color("LINE");
        ctx.stroke_round_rect(
            [draw_rect.x0, draw_rect.y0, draw_rect.x1, draw_rect.y1],
            BUTTON_RADIUS * factor,
            line,
            alpha,
        );
    }
    draw_test_content(menu, ctx, draw_rect, text_color, now, testing, alpha);
}

/// «Probar» mientras no hay prueba; la onda y «Hablando…» mientras dura.
fn draw_test_content(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    draw_rect: Rect,
    text_color: Color,
    now: f64,
    testing: f64,
    alpha: f64,
) {
    let (cx, cy) = draw_rect.center();
    if testing < 0.999 {
        ctx.text_at(
            "Probar",
            FS_SECONDARY,
            text_color,
            cx,
            cy,
            alpha * (1.0 - testing),
            Align::Center,
        );
    }
    if testing > 0.001 {
        draw_testing_wave(menu, ctx, draw_rect, text_color, now, testing, alpha);
    }
}

/// Relleno e tinta de un botón C-07 con la mezcla de hover.
fn button_fill(menu: &Menu, ctx: &PaintContext<'_>, variant: &str, hover: f64) -> (Color, Color) {
    let (fill, text) = ctx.button_colors(variant);
    if hover <= 0.001 {
        return (fill, text);
    }
    let mixed = lerp_rgb(
        fill.channels(),
        hover_target(menu.theme).channels(),
        BUTTON_HOVER_MIX * hover,
    );
    (Color::Rgb(mixed), text)
}

/// Mini onda de «Hablando…» dentro del botón «Probar».
fn draw_testing_wave(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    draw_rect: Rect,
    text_color: Color,
    now: f64,
    testing: f64,
    alpha: f64,
) {
    let content_w = menu.test_content_width();
    let wave_w =
        TEST_WAVE_BARS as f64 * TEST_WAVE_BAR_W + (TEST_WAVE_BARS as f64 - 1.0) * TEST_WAVE_GAP;
    let (center_x, bar_cy) = draw_rect.center();
    let start_x = center_x - content_w / 2.0;
    for index in 0..TEST_WAVE_BARS {
        let phase = now * TEST_WAVE_PHASE_SPEED + index as f64 * TEST_WAVE_PHASE_STEP;
        let level = TEST_WAVE_LEVEL_BASE + TEST_WAVE_LEVEL_SPAN * phase.sin().abs();
        let height = TEST_WAVE_MIN_H + (TEST_WAVE_MAX_H - TEST_WAVE_MIN_H) * level;
        let bar_x = start_x + index as f64 * (TEST_WAVE_BAR_W + TEST_WAVE_GAP);
        let mint = ctx.color("MINT");
        ctx.fill_round_rect(
            [
                bar_x,
                bar_cy - height / 2.0,
                bar_x + TEST_WAVE_BAR_W,
                bar_cy + height / 2.0,
            ],
            TEST_WAVE_BAR_W / 2.0,
            mint,
            alpha * testing,
        );
    }
    ctx.text_at(
        "Hablando…",
        FS_SECONDARY,
        text_color,
        start_x + wave_w + TEST_WAVE_GAP_TEXT,
        bar_cy,
        alpha * testing,
        Align::Left,
    );
}

/// `_draw_generic_action_row`: etiqueta y un botón a la derecha.
pub(super) fn draw_generic_action_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    center_y: f64,
    alpha: f64,
    armed: bool,
) {
    let name = ctx.color("TEXT");
    ctx.text_at(
        label(key),
        FS_ROW,
        name,
        ROW_X0,
        center_y,
        alpha,
        Align::Left,
    );
    let rect = Rect::new(
        (TEST_BUTTON_X1 + menu.dx) - BUTTON_MIN_W,
        center_y - BUTTON_H / 2.0,
        TEST_BUTTON_X1 + menu.dx,
        center_y + BUTTON_H / 2.0,
    );
    let hover = menu
        .row_hover
        .get(key)
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let variant = if armed { "amber" } else { "secondary" };
    let (fill, text_color) = button_fill(menu, ctx, variant, hover);
    ctx.fill_round_rect(
        [rect.x0, rect.y0, rect.x1, rect.y1],
        BUTTON_RADIUS,
        fill,
        alpha,
    );
    if menu.theme == "light" {
        let line = ctx.color("LINE");
        ctx.stroke_round_rect(
            [rect.x0, rect.y0, rect.x1, rect.y1],
            BUTTON_RADIUS,
            line,
            alpha,
        );
    }
    let (cx, cy) = rect.center();
    ctx.text_at(
        action_caption(key, armed),
        FS_SECONDARY,
        text_color,
        cx,
        cy,
        alpha,
        Align::Center,
    );
}

/// `ARMED_TEXT`, `ACTION_BUTTON_TEXT.get(key, "Abrir")`.
fn action_caption(key: &str, armed: bool) -> &'static str {
    if armed {
        return ARMED_TEXT;
    }
    ACTION_BUTTON_TEXT
        .iter()
        .find(|(action, _text)| *action == key)
        .map_or("Abrir", |(_action, text)| *text)
}

/// `_draw_selector_list`: opciones visibles del selector abierto.
pub fn draw_selector_list(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    now: f64,
    alpha: f64,
) {
    let options = menu.selector_options(key);
    if options.is_empty() {
        return;
    }
    let base_h = layout::row_height("select").expect("tipo de fila conocido");
    let top = row_y0 + f64::from(base_h);
    let visible = menu
        .selector_extra
        .value
        .min(options.len() as f64 * LIST_ITEM_H)
        .max(0.0);
    if visible <= 0.01 {
        return;
    }
    let list_scroll = menu
        .list_scroll
        .min((options.len() as f64 * LIST_ITEM_H - LIST_VIEW_H).max(0.0))
        .max(0.0);
    let selected = menu.settings.get(key);
    for (index, option) in options.iter().enumerate() {
        let item_y0 = top + index as f64 * LIST_ITEM_H - list_scroll;
        let item_y1 = item_y0 + LIST_ITEM_H;
        if item_y0 < top - 0.5 || item_y1 > top + visible + 0.5 {
            continue;
        }
        let item_alpha = alpha * menu.selector_item_alpha(index, now);
        if item_alpha <= 0.001 {
            continue;
        }
        let chosen = selected.is_some_and(|value| *value == option.value);
        draw_option_background(menu, ctx, index, item_y0, item_y1, chosen, item_alpha);
        let center_y = (item_y0 + item_y1) / 2.0;
        draw_option_content(menu, ctx, &option.label, chosen, center_y, item_alpha);
    }
}

/// Velo de hover y tinte de la opción elegida sobre el ancho del grupo.
fn draw_option_background(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    index: usize,
    item_y0: f64,
    item_y1: f64,
    chosen: bool,
    item_alpha: f64,
) {
    let bounds = [GROUP_X0, item_y0, GROUP_X1 + menu.dx, item_y1];
    if menu.hover_option == Some(index) {
        let hover = ctx.color("GROUP_HOVER");
        ctx.fill_round_rect(bounds, 0.0, hover, item_alpha);
    }
    if chosen {
        let mint = ctx.color("MINT");
        ctx.fill_round_rect(bounds, 0.0, mint, item_alpha * 0.10);
    }
}

/// Rótulo truncado y check de la opción elegida.
fn draw_option_content(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    label: &str,
    chosen: bool,
    center_y: f64,
    item_alpha: f64,
) {
    let fitted = fit_text(label, FS_BODY, LIST_TEXT_MAX_W + menu.dx, menu.scale);
    let color = if chosen {
        ctx.color("MINT")
    } else {
        ctx.color("TEXT")
    };
    ctx.text_at(
        &fitted,
        FS_BODY,
        color,
        LIST_TEXT_X,
        center_y,
        item_alpha,
        Align::Left,
    );
    if chosen {
        let mint_btn = ctx.color("MINT_BTN");
        ctx.draw_icon(
            "check",
            LIST_CHECK_CX + menu.dx,
            center_y,
            LIST_CHECK_SIZE,
            mint_btn,
            item_alpha,
        );
    }
}
