//! Port of `src/ezagent/voice/hud/menu.py 2339-3100` (F5-10a): huella del
//! cuadro, capas estáticas, composición, cabecera y grupos.
//!
//! Las filas se reparten entre `controls.rs` (paleta, selector, medidor y
//! texto) y los ganchos [`FrameExtras`] que F5-10b implementa en `advanced.rs`.

use ez_gfx::sdf::{self, FillOptions, Layer};
use ez_gfx::text::{Align, FS_MICRO, FS_TITLE};

use crate::anim::{lerp_rgb, Tween};
use crate::model::menu::geometry::Menu;
use crate::model::menu::rects::{GROUP_X0, GROUP_X1, HIDE_CX, HIDE_SIZE, ROW_X0, ROW_X1};
use crate::model::menu::selectors::{
    SELECTOR_ITEM_FADE_MS, SELECTOR_ITEM_STAGGER_MS, SELECTOR_START_DELAY_MS,
};
use crate::tokens::{Color, H_ISLAND};

use super::controls;
use super::{
    FrameExtras, Layout, PaintContext, GROUP_RADIUS, HAIRLINE_PX, HIDE_HOVER_DARK,
    HIDE_HOVER_LIGHT, HIDE_ICON_PRESS_SCALE, HIDE_ICON_SIZE, HIDE_PRESS_DARK, HIDE_PRESS_LIGHT,
    LABEL_X,
};

/// Huella del cuadro visible; dos claves iguales pintan lo mismo.
///
/// Los valores redondeados se guardan como enteros escalados (`round(x, n)`
/// multiplicado por `10^n`) para que la clave sea `Eq + Hash` como la tupla
/// del Python y `-0.0` coincida con `0.0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameKey {
    /// `_layer_signature_value`: lo que pintan las capas.
    pub signature: u64,
    /// `_open`.
    pub open: bool,
    /// `round(height, 3)` × 1000.
    pub height: i64,
    /// `round(content_alpha, 3)` × 1000.
    pub content_alpha: i64,
    /// `round(scroll_offset, 2)` × 100.
    pub scroll: i64,
    /// `round(indicator_alpha, 3)` × 1000.
    pub indicator_alpha: i64,
}

mod fingerprint;

use fingerprint::round_scaled;

/// `_content_tweens`: los tweens que afectan a las capas de contenido.
pub fn content_tweens(menu: &Menu) -> Vec<&Tween> {
    let mut tweens = vec![
        &menu.content_alpha,
        &menu.selector_extra,
        &menu.scroll_tween,
        &menu.testing_t,
    ];
    tweens.extend(menu.blocks.values());
    tweens.extend(menu.chevron.values());
    tweens.extend(menu.row_hover.values());
    tweens.extend(menu.button_hover.values());
    tweens.extend(menu.press.values());
    tweens.extend(menu.thumb.values());
    tweens.extend(menu.slider_color.values());
    tweens.extend(menu.toggle_pos.values());
    tweens.extend(menu.toggle_color.values());
    tweens
}

impl Menu {
    /// `_layers_static`: `true` si el contenido ya no depende de `now`.
    pub fn layers_static(&self, now: f64) -> bool {
        if self.testing_t.value > 0.01 {
            return false;
        }
        if self.editing_key.is_some() {
            return false;
        }
        if let Some(key) = &self.selector_key {
            if !self.selector_closing {
                let options = self.selector_options(key);
                if !options.is_empty() {
                    let end = ((options.len() as f64 - 1.0) * SELECTOR_ITEM_STAGGER_MS
                        + SELECTOR_ITEM_FADE_MS
                        + SELECTOR_START_DELAY_MS)
                        / 1000.0;
                    if now - self.selector_item_start < end {
                        return false;
                    }
                }
            }
        }
        if now
            < self
                .check_until
                .max(self.saved_flash_until)
                .max(self.error_until)
        {
            return false;
        }
        if self.footer_prev.is_some() || self.save_prev_fill.is_some() {
            return false;
        }
        !content_tweens(self).iter().any(|tween| tween.active())
    }

    /// `frame_key`: huella del cuadro, o `None` si algo se anima.
    pub fn frame_key(&self, now: f64) -> Option<FrameKey> {
        if !self.layers_static(now) {
            return None;
        }
        Some(FrameKey {
            signature: self.layer_signature_value(now),
            open: self.open,
            height: round_scaled(self.height_tween.value, 3),
            content_alpha: round_scaled(self.content_alpha.value, 3),
            scroll: round_scaled(self.scroll_offset(), 2),
            indicator_alpha: round_scaled(self.indicator_alpha(now), 3),
        })
    }

    /// `draw`: pinta el menú sobre `buf` con los ganchos de F5-10a.
    pub fn draw(&mut self, buf: &mut Layer, now: f64) {
        let mut extras = super::NoExtras;
        self.draw_with(buf, now, &mut extras);
    }

    /// `draw` con los ganchos de F5-10b (`advanced::Advanced`).
    pub fn draw_with(&mut self, buf: &mut Layer, now: f64, extras: &mut dyn FrameExtras) {
        // DECISION: el Python valida `buf.dtype == float32` y la forma
        // `(H, W, 4)`; `Layer` ya garantiza ambos, así que no hay rama de error.
        self.update(now);
        if !self.open && self.height_tween.value <= 0.5 {
            return;
        }
        let palette = self.palette();
        self.ensure_layers(buf);
        let layout = self.layout();
        let scroll = self.scroll_offset();
        let reuse = self.layers_static(now)
            && self.layer_signature == Some(self.layer_signature_value(now));
        let mut layers = self.layers.take().expect("capas del menú reservadas");
        if !reuse {
            self.paint_layers(&mut layers, &palette, &layout, now, scroll, extras);
            self.layer_signature = Some(self.layer_signature_value(now));
        }
        self.composite_layers(buf, &layers, &layout);
        extras.draw_scroll(self, buf, &palette, &layout, now, scroll);
        clamp_premultiplied(buf);
        self.layers = Some(layers);
    }

    /// Reserva las tres capas si cambió el tamaño del búfer.
    fn ensure_layers(&mut self, buf: &Layer) {
        let target_h = buf.height().max(
            ((H_ISLAND as f64 + self.viewport_height()) * self.scale).round_ties_even() as u32,
        );
        let shape = (target_h as usize, buf.width() as usize, 4);
        if self.layer_shape == Some(shape) && self.layers.is_some() {
            return;
        }
        self.layer_shape = Some(shape);
        self.layers = Some([
            Layer::new(buf.width(), target_h),
            Layer::new(buf.width(), target_h),
            Layer::new(buf.width(), target_h),
        ]);
        self.layer_signature = None;
    }

    /// Pinta cabecera, grupos y pie en sus capas disjuntas.
    fn paint_layers(
        &mut self,
        layers: &mut [Layer; 3],
        palette: &crate::tokens::ColorMap,
        layout: &Layout,
        now: f64,
        scroll: f64,
        extras: &mut dyn FrameExtras,
    ) {
        for layer in layers.iter_mut() {
            layer.fill(0.0);
        }
        {
            let mut ctx =
                PaintContext::new(&mut layers[0], self.scale, self.dx, self.theme, palette);
            draw_header(self, &mut ctx, layout, now);
        }
        let groups = self.shown_groups();
        for (name, rows) in &groups {
            let mut ctx =
                PaintContext::new(&mut layers[1], self.scale, self.dx, self.theme, palette);
            draw_group(self, &mut ctx, name, rows, layout, now, scroll, extras);
        }
        {
            let mut ctx =
                PaintContext::new(&mut layers[2], self.scale, self.dx, self.theme, palette);
            extras.draw_footer(self, &mut ctx, layout, now);
        }
    }

    /// `_composite_band` para cabecera, contenido y pie.
    fn composite_layers(&self, buf: &mut Layer, layers: &[Layer; 3], layout: &Layout) {
        let window_h = H_ISLAND as f64 + self.height_tween.value;
        let content_top = self.content_top(layout);
        let footer_top = self.footer_top(layout);
        let right = GROUP_X1 + self.dx;
        let _ = sdf::composite_band(
            buf,
            &layers[0],
            H_ISLAND as f64,
            content_top.min(window_h),
            self.scale,
            GROUP_X0,
            Some(right),
        );
        if content_top < footer_top {
            let _ = sdf::composite_band(
                buf,
                &layers[1],
                content_top,
                footer_top.min(window_h),
                self.scale,
                GROUP_X0,
                Some(right),
            );
        }
        if footer_top < window_h {
            let _ = sdf::composite_band(
                buf,
                &layers[2],
                footer_top,
                window_h,
                self.scale,
                GROUP_X0,
                Some(right),
            );
        }
    }
}

/// `np.minimum(buf[..., :3], buf[..., 3:4], out=buf[..., :3])`.
fn clamp_premultiplied(buf: &mut Layer) {
    for pixel in buf.as_raw_mut().as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        pixel[0] = pixel[0].min(alpha);
        pixel[1] = pixel[1].min(alpha);
        pixel[2] = pixel[2].min(alpha);
    }
}

// --------------------------------------------------------------------------
// Cabecera, grupos y reparto de filas
// --------------------------------------------------------------------------

/// `_draw_header`.
pub fn draw_header(menu: &Menu, ctx: &mut PaintContext<'_>, layout: &Layout, now: f64) {
    let (alpha, dy) = menu.block_values("header", now);
    if alpha <= 0.001 {
        return;
    }
    let header = layout.get("header").expect("el layout tiene cabecera");
    let cy = (header.y0 + header.y1) / 2.0 + dy;
    let text = ctx.color("TEXT");
    ctx.text_at(&menu.title, FS_TITLE, text, LABEL_X, cy, alpha, Align::Left);
    let hover = menu
        .button_hover
        .get("hide")
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    let press = menu
        .press
        .get("hide")
        .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
    if hover > 0.001 || press > 0.001 {
        let (overlay, overlay_alpha) = if menu.theme == "light" {
            (
                Color::Rgb([0, 0, 0]),
                HIDE_HOVER_LIGHT * hover + HIDE_PRESS_LIGHT * press,
            )
        } else {
            (
                Color::Rgb([255, 255, 255]),
                HIDE_HOVER_DARK * hover + HIDE_PRESS_DARK * press,
            )
        };
        ctx.fill_circle(
            HIDE_CX + menu.dx,
            cy,
            HIDE_SIZE,
            overlay,
            alpha * overlay_alpha,
        );
    }
    let icon_size = HIDE_ICON_SIZE * (1.0 - (1.0 - HIDE_ICON_PRESS_SCALE) * press);
    let icon = ctx.color("TEXT_2");
    ctx.draw_icon("close", HIDE_CX + menu.dx, cy, icon_size, icon, alpha);
}

/// `_draw_group`.
#[allow(clippy::too_many_arguments)]
pub fn draw_group(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    group_name: &str,
    rows: &[(String, String)],
    layout: &Layout,
    now: f64,
    scroll: f64,
    extras: &mut dyn FrameExtras,
) {
    let (alpha, dy) = menu.block_values(group_name, now);
    if alpha <= 0.001 {
        return;
    }
    let y_offset = dy - scroll;
    draw_group_label(ctx, group_name, layout, y_offset, alpha);
    let group = layout
        .get(&format!("group_{group_name}"))
        .expect("el layout tiene el grupo");
    let gx0 = GROUP_X0;
    let gx1 = GROUP_X1 + menu.dx;
    let gy0 = group.y0 + y_offset;
    let gy1 = group.y1 + y_offset;
    let group_fill = ctx.color("GROUP");
    ctx.fill_round_rect_opts(
        [gx0, gy0, gx1, gy1],
        GROUP_RADIUS,
        group_fill,
        alpha,
        FillOptions {
            replace: true,
            ..FillOptions::default()
        },
    );

    let last = rows.len().checked_sub(1);
    for (index, (key, kind)) in rows.iter().enumerate() {
        let row = layout.get(key).expect("el layout tiene la fila");
        let base_h = crate::layout::row_height(kind).expect("tipo de fila conocido") as f64;
        let row_y0 = row.y0 + y_offset;
        let row_y1 = row_y0 + base_h;
        let disabled = kind == "select" && !menu.settings.selector_enabled(key);
        draw_row_background(menu, ctx, key, disabled, row_y0, row_y1, index, last, alpha);
        draw_row(
            menu, ctx, key, kind, row_y0, layout, now, scroll, alpha, extras,
        );
        if menu.selector_key.as_deref() == Some(key.as_str()) {
            extras.draw_selector_list(menu, ctx, key, row_y0, layout, now, alpha);
        }
        if last != Some(index) {
            draw_row_separator(ctx, menu.dx, row.y1 + y_offset, alpha);
        }
    }

    if menu.theme == "light" {
        let line = ctx.color("LINE");
        ctx.stroke_round_rect([gx0, gy0, gx1, gy1], GROUP_RADIUS, line, alpha);
    }
}

/// Etiqueta del grupo (`GROUP_LABELS`) sobre el contenedor.
fn draw_group_label(
    ctx: &mut PaintContext<'_>,
    group_name: &str,
    layout: &Layout,
    y_offset: f64,
    alpha: f64,
) {
    let label_rect = layout
        .get(&format!("label_{group_name}"))
        .expect("el layout tiene la etiqueta del grupo");
    let group_label = crate::layout::group_label(group_name).expect("el grupo tiene etiqueta");
    let label_color = ctx.color("TEXT_3");
    ctx.text_at(
        group_label,
        FS_MICRO,
        label_color,
        LABEL_X,
        (label_rect.y0 + label_rect.y1) / 2.0 + y_offset,
        alpha,
        Align::Left,
    );
}

/// Velo de la fila bajo el puntero, con las esquinas del grupo en los extremos.
#[allow(clippy::too_many_arguments)]
fn draw_row_background(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    disabled: bool,
    row_y0: f64,
    row_y1: f64,
    index: usize,
    last: Option<usize>,
    alpha: f64,
) {
    let hover = if disabled {
        0.0
    } else {
        menu.row_hover
            .get(key)
            .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0))
    };
    if hover <= 0.001 {
        return;
    }
    let fill = lerp_rgb(ctx.rgb("GROUP"), ctx.rgb("GROUP_HOVER"), hover);
    ctx.fill_round_rect_opts(
        [GROUP_X0, row_y0, GROUP_X1 + menu.dx, row_y1],
        0.0,
        Color::Rgb(fill),
        alpha,
        FillOptions {
            radius_top: Some(if index == 0 { GROUP_RADIUS } else { 0.0 }),
            radius_bottom: Some(if last == Some(index) {
                GROUP_RADIUS
            } else {
                0.0
            }),
            replace: false,
        },
    );
}

/// Hairline entre dos filas.
fn draw_row_separator(ctx: &mut PaintContext<'_>, dx: f64, boundary: f64, alpha: f64) {
    let line = ctx.color("LINE_2");
    ctx.fill_round_rect(
        [
            ROW_X0,
            boundary - HAIRLINE_PX / 2.0,
            ROW_X1 + dx,
            boundary + HAIRLINE_PX / 2.0,
        ],
        0.0,
        line,
        alpha,
    );
}

/// `_draw_row`: reparte cada tipo de fila entre esta mitad y F5-10b.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    menu: &Menu,
    ctx: &mut PaintContext<'_>,
    key: &str,
    kind: &str,
    row_y0: f64,
    layout: &Layout,
    now: f64,
    scroll: f64,
    alpha: f64,
    extras: &mut dyn FrameExtras,
) {
    let disabled = kind == "select" && !menu.settings.selector_enabled(key);
    let center_y =
        row_y0 + crate::layout::row_height(kind).expect("tipo de fila conocido") as f64 / 2.0;
    match kind {
        "slider" => extras.draw_slider_row(menu, ctx, key, row_y0, alpha),
        "select" => controls::draw_select_row(menu, ctx, key, row_y0, center_y, alpha, disabled),
        "toggle" => extras.draw_toggle_row(menu, ctx, key, center_y, alpha),
        "text" => controls::draw_text_row(menu, ctx, key, row_y0, alpha, now, extras),
        "meter" => controls::draw_meter_row(menu, ctx, key, row_y0, alpha, now),
        "palette" => controls::draw_palette_row(menu, ctx, row_y0, alpha),
        "action" => {
            extras.draw_action_row(menu, ctx, key, row_y0, center_y, layout, now, scroll, alpha)
        }
        // `ROW` solo tiene los siete tipos de la tabla; un tipo nuevo debe
        // añadirse aquí y a `layout::ROW` a la vez.
        _ => {}
    }
}
