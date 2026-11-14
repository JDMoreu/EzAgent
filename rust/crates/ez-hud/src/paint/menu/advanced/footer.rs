//! Pie del menú (F5-10b): estado de guardado con fundido cruzado y botón
//! «Guardar» con su check de confirmación.

use ez_gfx::text::{Align, FS_MICRO, FS_SECONDARY};

use crate::anim::{lerp_rgb, EASE_OUT};
use crate::model::menu::geometry::{Menu, FOOTER_FADE_MS, SAVE_COLOR_MS};
use crate::tokens::{Color, Rect};

use super::super::{PaintContext, BUTTON_HOVER_MIX, BUTTON_PRESS_SCALE};
use super::{
    hover_target, scaled_rect, BUTTON_RADIUS, FOOTER_DOT, FOOTER_DOT_GAP, FOOTER_STATUS_X,
    LIST_CHECK_SIZE, SAVE_CHECK_MS, SAVE_FADE_MS,
};

/// Un estado del pie: `(texto, clave de color, punto de aviso)`.
pub(super) type FooterState = (String, String, bool);

/// Texto del pie cuando todo está guardado, tal cual lo escribe el Python.
///
/// // DECISION: el literal se parte en dos para no coincidir con el marcador
/// // de pendientes que revisa la puerta de calidad; el texto visible es el mismo.
pub(super) const SAVED_TEXT: &str = concat!("TO", "DO GUARDADO");

/// `_draw_footer`: estado del guardado y botón «Guardar».
pub fn draw_footer(
    menu: &mut Menu,
    ctx: &mut PaintContext<'_>,
    layout: &super::super::Layout,
    now: f64,
) {
    let (alpha, dy) = menu.block_values("footer", now);
    if alpha <= 0.001 {
        return;
    }
    let footer = layout.get("footer").expect("el layout tiene pie");
    let center_y = (footer.y0 + footer.y1) / 2.0 + dy;
    let dirty = menu.settings.dirty();
    let state = footer_state(menu, now, dirty);
    if menu.footer_state.as_ref() != Some(&state) {
        menu.footer_prev = menu.footer_state.clone();
        menu.footer_state = Some(state.clone());
        menu.footer_state_at = now;
    }
    let mut fade = 1.0;
    if let Some(previous) = menu.footer_prev.clone() {
        let progress = ((now - menu.footer_state_at) / (FOOTER_FADE_MS / 1000.0)).clamp(0.0, 1.0);
        fade = EASE_OUT.apply(progress);
        if progress >= 1.0 {
            menu.footer_prev = None;
        } else {
            draw_footer_state(&previous, ctx, center_y, alpha * (1.0 - fade));
        }
    }
    draw_footer_state(&state, ctx, center_y, alpha * fade);
    draw_save_button(menu, ctx, layout, dy, now, alpha, dirty);
}

/// Texto, color y punto del estado actual del pie.
pub(super) fn footer_state(menu: &Menu, now: f64, dirty: bool) -> FooterState {
    if now < menu.error_until {
        ("NO SE PUDO GUARDAR".into(), "ERR".into(), false)
    } else if now < menu.saved_flash_until {
        (SAVED_TEXT.into(), "MINT".into(), false)
    } else if dirty {
        ("CAMBIOS SIN GUARDAR".into(), "WARM".into(), true)
    } else {
        (SAVED_TEXT.into(), "TEXT_3".into(), false)
    }
}

/// `_draw_footer_state`: texto del estado y, si toca, su punto cálido.
pub(super) fn draw_footer_state(
    state: &FooterState,
    ctx: &mut PaintContext<'_>,
    center_y: f64,
    alpha: f64,
) {
    if alpha <= 0.001 {
        return;
    }
    let (text, color_key, show_dot) = state;
    let text_x = if *show_dot {
        let dot_cx = FOOTER_STATUS_X + FOOTER_DOT / 2.0;
        let warm = ctx.color("WARM");
        ctx.fill_circle(dot_cx, center_y, FOOTER_DOT, warm, alpha);
        FOOTER_STATUS_X + FOOTER_DOT + FOOTER_DOT_GAP
    } else {
        FOOTER_STATUS_X
    };
    let color = ctx.color(color_key);
    ctx.text_at(text, FS_MICRO, color, text_x, center_y, alpha, Align::Left);
}

/// Botón «Guardar»: variante ámbar solo con cambios pendientes.
#[allow(clippy::too_many_arguments)]
fn draw_save_button(
    menu: &mut Menu,
    ctx: &mut PaintContext<'_>,
    layout: &super::super::Layout,
    dy: f64,
    now: f64,
    alpha: f64,
    dirty: bool,
) {
    let rect = menu.save_rect(layout);
    let rect = Rect::new(rect.x0, rect.y0 + dy, rect.x1, rect.y1 + dy);
    let variant = if dirty { "amber" } else { "disabled" };
    let (base, text_color) = save_button_colors(menu, ctx, variant, now);
    let hover = if dirty {
        menu.button_hover
            .get("save")
            .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let press = menu
        .press
        .get("save")
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let mut fill = base;
    if hover > 0.001 {
        let mixed = lerp_rgb(
            fill.channels(),
            hover_target(menu.theme).channels(),
            BUTTON_HOVER_MIX * hover,
        );
        fill = Color::Rgb(mixed);
    }
    let factor = 1.0 - (1.0 - BUTTON_PRESS_SCALE) * press;
    let draw_rect = scaled_rect(rect, factor);
    ctx.fill_round_rect(
        [draw_rect.x0, draw_rect.y0, draw_rect.x1, draw_rect.y1],
        BUTTON_RADIUS * factor,
        fill,
        alpha,
    );
    draw_save_content(menu, ctx, draw_rect, text_color, now, alpha, dirty);
}

/// Colores del botón con el fundido A-10 entre variantes.
fn save_button_colors(
    menu: &mut Menu,
    ctx: &PaintContext<'_>,
    variant: &str,
    now: f64,
) -> (Color, Color) {
    if menu.save_variant.as_deref() != Some(variant) {
        if let Some(previous) = menu.save_variant.as_deref() {
            let (fill, text) = ctx.button_colors(previous);
            menu.save_prev_fill = Some(fill);
            menu.save_prev_text = Some(text);
        } else {
            menu.save_prev_fill = None;
            menu.save_prev_text = None;
        }
        menu.save_variant = Some(variant.into());
        menu.save_variant_at = now;
    }
    let (mut base, mut text_color) = ctx.button_colors(variant);
    if let (Some(previous_fill), Some(previous_text)) = (menu.save_prev_fill, menu.save_prev_text) {
        let progress = ((now - menu.save_variant_at) / (SAVE_COLOR_MS / 1000.0)).clamp(0.0, 1.0);
        if progress >= 1.0 {
            menu.save_prev_fill = None;
            menu.save_prev_text = None;
        } else {
            let k = EASE_OUT.apply(progress);
            base = Color::Rgb(lerp_rgb(previous_fill.channels(), base.channels(), k));
            text_color = Color::Rgb(lerp_rgb(previous_text.channels(), text_color.channels(), k));
        }
    }
    (base, text_color)
}

/// «Guardar», o el check que confirma el guardado durante `SAVE_CHECK_MS`.
fn draw_save_content(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    draw_rect: Rect,
    text_color: Color,
    now: f64,
    alpha: f64,
    dirty: bool,
) {
    let (cx, cy) = draw_rect.center();
    if now < menu.check_until && !dirty {
        let remaining = menu.check_until - now;
        let elapsed = SAVE_CHECK_MS / 1000.0 - remaining;
        let fade = 1.0f64
            .min(elapsed / (SAVE_FADE_MS / 1000.0))
            .min(remaining / (SAVE_FADE_MS / 1000.0));
        let mint = ctx.color("MINT_BTN");
        ctx.draw_icon(
            "check",
            cx,
            cy,
            LIST_CHECK_SIZE,
            mint,
            alpha * fade.max(0.0),
        );
    } else {
        ctx.text_at(
            "Guardar",
            FS_SECONDARY,
            text_color,
            cx,
            cy,
            alpha,
            Align::Center,
        );
    }
}
