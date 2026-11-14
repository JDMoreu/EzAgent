//! Teclas, atajos, valores secretos y botón de copiar (F5-10b).

use ez_gfx::text::{self, Align, FS_BODY, FS_SECONDARY};

use crate::model::menu::geometry::Menu;
use crate::model::menu::rects::{COPY_BTN, ROW_X0, ROW_X1, TEXT_LINE2_CY};
use crate::paint::hotkey_field as hk;

use super::super::{PaintContext, DOT_MAX, DOT_PITCH};
use super::{str_or_empty, COPY_ICON, DOT_R, KEYCAP_GAP, KEYCAP_H, KEYCAP_PAD, KEYCAP_RADIUS};

/// `_draw_keycap`: una tecla dibujada; devuelve la `x` de la siguiente.
pub fn draw_keycap(
    ctx: &mut PaintContext<'_>,
    x: f64,
    cy: f64,
    label: &str,
    alpha: f64,
    accent: bool,
    ghost: bool,
) -> f64 {
    let width = text::text_size(label, FS_SECONDARY, ctx.scale, None, "left")
        .map_or(0.0, |(width, _height)| width)
        + 2.0 * KEYCAP_PAD;
    let width = width.max(KEYCAP_H);
    let bounds = [x, cy - KEYCAP_H / 2.0, x + width, cy + KEYCAP_H / 2.0];
    let color = if ghost {
        let dim = ctx.color("TEXT_3");
        ctx.stroke_round_rect(bounds, KEYCAP_RADIUS, dim, alpha * 0.7);
        dim
    } else if accent {
        let mint = ctx.color("MINT");
        ctx.fill_round_rect(bounds, KEYCAP_RADIUS, mint, alpha * 0.18);
        ctx.stroke_round_rect(bounds, KEYCAP_RADIUS, mint, alpha * 0.8);
        mint
    } else {
        let track = ctx.color("TRACK");
        ctx.fill_round_rect(bounds, KEYCAP_RADIUS, track, alpha * 0.8);
        ctx.color("TEXT")
    };
    ctx.text_at(
        label,
        FS_SECONDARY,
        color,
        x + KEYCAP_PAD,
        cy,
        alpha,
        Align::Left,
    );
    x + width + KEYCAP_GAP
}

/// `_draw_hotkey_value`: atajo como teclas dibujadas y captura en vivo.
pub fn draw_hotkey_value(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    now: f64,
    alpha: f64,
) {
    let cy = row_y0 + TEXT_LINE2_CY;
    let right = ROW_X1 + menu.dx;
    if menu.capturing && menu.editing_key.as_deref() == Some(key) {
        draw_capture_field(menu, ctx, now, cy, right, alpha);
    } else {
        draw_hotkey_keycaps(menu, ctx, key, cy, right, alpha);
    }
}

/// Teclas del atajo guardado y la pista «Cambiar» bajo el puntero.
fn draw_hotkey_keycaps(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    cy: f64,
    right: f64,
    alpha: f64,
) {
    let mut x = ROW_X0;
    for part in hk::parts_for(&str_or_empty(menu.settings.get(key))) {
        x = draw_keycap(ctx, x, cy, &part, alpha, false, false);
    }
    let hover = menu
        .row_hover
        .get(key)
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    if hover > 0.01 {
        let dim = ctx.color("TEXT_3");
        ctx.text_at(
            "Cambiar",
            FS_BODY,
            dim,
            right,
            cy,
            alpha * hover,
            Align::Right,
        );
    }
}

/// Campo de captura: modificadores en vivo, mensaje de error o la invitación.
fn draw_capture_field(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    now: f64,
    cy: f64,
    right: f64,
    alpha: f64,
) {
    let field = [
        ROW_X0 - 8.0,
        cy - KEYCAP_H / 2.0 - 1.5,
        right + 6.0,
        cy + KEYCAP_H / 2.0 + 1.5,
    ];
    let track = ctx.color("TRACK");
    ctx.fill_round_rect(field, 9.0, track, alpha * 0.5);
    let mint = ctx.color("MINT");
    ctx.stroke_round_rect(field, 9.0, mint, alpha);
    if let Some((message, until)) = &menu.capture_msg {
        if now < *until {
            let error = ctx.color("ERR");
            ctx.text_at(message, FS_BODY, error, ROW_X0, cy, alpha, Align::Left);
            return;
        }
    }
    let names: Vec<&str> = menu.capture_mods.iter().map(String::as_str).collect();
    let mods = hk::Mods::from_names(&names);
    let mut x = ROW_X0;
    for part in hk::mod_labels(mods) {
        x = draw_keycap(ctx, x, cy, &part, alpha, true, false);
    }
    if !mods.is_empty() {
        let _ = draw_keycap(ctx, x, cy, "…", alpha, false, true);
        return;
    }
    let pulse = 0.65 + 0.35 * ((now - menu.edit_t0) * 5.0).sin();
    let text_2 = ctx.color("TEXT_2");
    ctx.text_at(
        "Pulsa la combinación",
        FS_BODY,
        text_2,
        ROW_X0,
        cy,
        alpha * pulse,
        Align::Left,
    );
    let text_3 = ctx.color("TEXT_3");
    ctx.text_at(
        "Esc cancela",
        FS_BODY,
        text_3,
        right,
        cy,
        alpha,
        Align::Right,
    );
}

/// `_draw_secret_value`: puntos en lugar de la clave; nunca su contenido.
pub fn draw_secret_value(
    ctx: &mut PaintContext<'_>,
    raw: &str,
    editing: bool,
    value_cy: f64,
    alpha: f64,
) {
    if raw.is_empty() && !editing {
        let dim = ctx.color("TEXT_3");
        ctx.text_at(
            "Sin clave",
            FS_BODY,
            dim,
            ROW_X0,
            value_cy,
            alpha,
            Align::Left,
        );
        return;
    }
    let color = if editing {
        ctx.color("TEXT")
    } else {
        ctx.color("TEXT_2")
    };
    for index in 0..raw.chars().count().min(DOT_MAX) {
        let cx = ROW_X0 + DOT_R + index as f64 * DOT_PITCH;
        ctx.fill_circle(cx, value_cy, DOT_R * 2.0, color, alpha);
    }
}

/// `_draw_copy_button`: icono «copiar» (un check un momento después de copiar).
pub fn draw_copy_button(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    row_y0: f64,
    now: f64,
    alpha: f64,
) {
    if menu.secret_text(key).is_empty() {
        return;
    }
    let rect = menu.copy_rect(row_y0);
    let (cx, cy) = rect.center();
    let copied = now < menu.copied.get(key).copied().unwrap_or(0.0);
    let hovered = menu.copy_hover.as_deref() == Some(key);
    if hovered && !copied {
        let hover = ctx.color("GROUP_HOVER");
        ctx.fill_circle(cx, cy, COPY_BTN, hover, alpha);
    }
    let (icon, color) = if copied {
        ("check", ctx.color("MINT"))
    } else if hovered {
        ("copy", ctx.color("TEXT"))
    } else {
        ("copy", ctx.color("TEXT_2"))
    };
    ctx.draw_icon(icon, cx, cy, COPY_ICON, color, alpha);
}
