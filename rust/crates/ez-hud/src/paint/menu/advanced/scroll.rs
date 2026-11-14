//! Barra de desplazamiento del menú (F5-10b): degradados en los bordes y el
//! indicador que se ve incluso en reposo.

use ez_gfx::sdf::{self, Layer};

use crate::model::menu::geometry::Menu;
use crate::tokens::{ColorMap, BODY_X0, BODY_X1};

use super::super::{to_rgba, Layout};
use super::{
    SCROLL_GRADIENT_H, SCROLL_INDICATOR_ALPHA, SCROLL_INDICATOR_IDLE_ALPHA, SCROLL_INDICATOR_MIN_H,
    SCROLL_INDICATOR_W, SCROLL_INDICATOR_X,
};

/// `_draw_scroll`: se pinta sobre la ventana, no sobre las capas.
pub fn draw_scroll(
    menu: &Menu,
    buf: &mut Layer,
    palette: &ColorMap,
    layout: &Layout,
    now: f64,
    scroll: f64,
) {
    let max_scroll = menu.scroll_max();
    if max_scroll <= 0.5 || menu.height_tween.value <= 4.0 {
        return;
    }
    let content_top = menu.content_top(layout);
    let footer_top = menu.footer_top(layout);
    draw_edge_gradients(menu, buf, palette, scroll, content_top, footer_top);
    draw_indicator(buf, menu, palette, now, scroll, content_top, footer_top);
}

/// `_fill_vertical_gradient` en los bordes visibles del contenido.
fn draw_edge_gradients(
    menu: &Menu,
    buf: &mut Layer,
    palette: &ColorMap,
    scroll: f64,
    content_top: f64,
    footer_top: f64,
) {
    // 1 px hacia dentro: el degradado no debe tapar la línea del borde.
    let x0 = BODY_X0 + 1.0;
    let x1 = (BODY_X1 + menu.dx) - 1.0;
    let body = *palette.get("BODY").expect("la paleta del menú tiene BODY");
    let max_scroll = menu.scroll_max();
    if scroll > 0.5 {
        sdf::fill_vertical_gradient(
            buf,
            [x0, x1, content_top, content_top + SCROLL_GRADIENT_H],
            to_rgba(body),
            menu.scale,
            1.0,
            0.0,
        );
    }
    if scroll < max_scroll - 0.5 {
        sdf::fill_vertical_gradient(
            buf,
            [x0, x1, footer_top - SCROLL_GRADIENT_H, footer_top],
            to_rgba(body),
            menu.scale,
            0.0,
            1.0,
        );
    }
}

/// Pulgar del scroll, visible también en reposo.
#[allow(clippy::too_many_arguments)]
fn draw_indicator(
    buf: &mut Layer,
    menu: &Menu,
    palette: &ColorMap,
    now: f64,
    scroll: f64,
    content_top: f64,
    footer_top: f64,
) {
    let max_scroll = menu.scroll_max();
    let indicator = SCROLL_INDICATOR_IDLE_ALPHA.max(menu.indicator_alpha(now));
    if indicator <= 0.01 {
        return;
    }
    let clip_h = (footer_top - content_top).max(0.0);
    if clip_h <= 0.0 {
        return;
    }
    let total = clip_h + max_scroll;
    let indicator_h = SCROLL_INDICATOR_MIN_H.max(clip_h * (clip_h / total));
    let indicator_h = indicator_h.min(clip_h);
    let travel = (clip_h - indicator_h).max(0.0);
    let y0 = content_top + travel * (scroll / max_scroll);
    let color = *palette
        .get("TEXT_3")
        .expect("la paleta del menú tiene TEXT_3");
    sdf::fill_round_rect(
        buf,
        [
            SCROLL_INDICATOR_X + menu.dx,
            y0,
            SCROLL_INDICATOR_X + menu.dx + SCROLL_INDICATOR_W,
            y0 + indicator_h,
        ],
        SCROLL_INDICATOR_W / 2.0,
        to_rgba(color),
        menu.scale,
        SCROLL_INDICATOR_ALPHA * indicator,
    );
}
