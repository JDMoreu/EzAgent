//! Filas, marcas de estado y rellenos de la vista rápida (`peek_paint.py`).

use crate::paint::marks;
use crate::theme::card_palette;
use crate::tokens::{Color, ColorMap};

use super::body::body;
use super::spin::spinner;
use super::text::{baselines, draw_text, text_baseline_top, text_cap_height};
use super::{
    dismissible, IconPainter, PeekBuffer, PeekItem, DISMISS_W, FS_META, FS_TASK, MARGIN, MARK_COL,
    MARK_ICON, MORE_H, PAD_X, PAD_Y, PEEK_W, RADIUS, ROW_H, ROW_INSET,
};

/// Capa estática: cuerpo, filas, separadores, el punto de «sin ver» y la ×.
#[allow(clippy::too_many_arguments)]
pub fn paint_rows(
    items: &[PeekItem],
    more: usize,
    w: usize,
    h: usize,
    scale: f64,
    theme: &str,
    hover: Option<usize>,
    now: f64,
    icons: &dyn IconPainter,
) -> PeekBuffer {
    let pal = card_palette(theme);
    let mut buf = PeekBuffer::new(w, h);
    buf.data.copy_from_slice(&body(w, h, scale, theme));
    let x0 = MARGIN + PAD_X;
    let (first, second) = baselines(scale);
    for (index, item) in items.iter().enumerate() {
        paint_row(
            &mut buf, item, index, hover, scale, theme, &pal, x0, first, second, now, icons,
        );
        paint_rule(&mut buf, index, items.len(), hover, scale, &pal, x0);
    }
    if more > 0 {
        paint_more(&mut buf, more, items.len(), scale, &pal, x0);
    }
    buf
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    buf: &mut PeekBuffer,
    item: &PeekItem,
    index: usize,
    hover: Option<usize>,
    scale: f64,
    theme: &str,
    pal: &ColorMap,
    x0: f64,
    first: f64,
    second: f64,
    now: f64,
    icons: &dyn IconPainter,
) {
    let y = MARGIN + PAD_Y + index as f64 * ROW_H;
    // La × solo ocupa sitio en la fila bajo el puntero.
    let text_w = PEEK_W
        - PAD_X * 2.0
        - MARK_COL
        - if hover == Some(index) {
            DISMISS_W
        } else {
            10.0
        };
    if hover == Some(index) {
        let inset = ROW_INSET;
        fill_round(
            buf,
            MARGIN + inset,
            y + inset,
            PEEK_W - 2.0 * inset,
            ROW_H - 2.0 * inset,
            RADIUS - inset,
            pal["GROUP_HOVER"],
            scale,
        );
    }
    paint_row_text(buf, item, y, text_w, first, second, scale, pal, x0, now);
    paint_row_end(buf, item, index, y, hover, scale, theme, pal, icons);
}

/// Título y línea de estado de una fila.
#[allow(clippy::too_many_arguments)]
fn paint_row_text(
    buf: &mut PeekBuffer,
    item: &PeekItem,
    y: f64,
    text_w: f64,
    first: f64,
    second: f64,
    scale: f64,
    pal: &ColorMap,
    x0: f64,
    now: f64,
) {
    let title = super::fit(&item.title, FS_TASK, text_w, scale);
    let top = text_baseline_top(&title, FS_TASK, y + first, scale);
    draw_text(buf, &title, FS_TASK, pal["TEXT"], x0 + MARK_COL, top, scale);
    let meta = super::fit(&super::meta_line(item, now), FS_META, text_w, scale);
    let top = text_baseline_top(&meta, FS_META, y + second, scale);
    draw_text(
        buf,
        &meta,
        FS_META,
        pal["TEXT_2"],
        x0 + MARK_COL,
        top,
        scale,
    );
}

/// La × de la fila bajo el puntero o el punto de «sin ver».
#[allow(clippy::too_many_arguments)]
fn paint_row_end(
    buf: &mut PeekBuffer,
    item: &PeekItem,
    index: usize,
    y: f64,
    hover: Option<usize>,
    scale: f64,
    theme: &str,
    pal: &ColorMap,
    icons: &dyn IconPainter,
) {
    if hover == Some(index) && dismissible(item) {
        let cx = MARGIN + PEEK_W - PAD_X - 6.0;
        icons.draw_icon(
            buf,
            "close",
            cx,
            y + ROW_H / 2.0,
            13.0,
            Color::Rgb(pal["TEXT_2"].channels()),
            scale,
        );
    } else if item.unseen {
        let cx = MARGIN + PEEK_W - PAD_X - 2.5;
        let cy = y + ROW_H / 2.0;
        fill_round(
            buf,
            cx - 2.5,
            cy - 2.5,
            5.0,
            5.0,
            2.5,
            Color::Rgb(marks::ink(theme).mint),
            scale,
        );
    }
}

fn paint_rule(
    buf: &mut PeekBuffer,
    index: usize,
    count: usize,
    hover: Option<usize>,
    scale: f64,
    pal: &ColorMap,
    x0: f64,
) {
    if index + 1 < count && hover != Some(index) && hover != Some(index + 1) {
        let rule_w = PEEK_W - PAD_X * 2.0 - MARK_COL;
        let y = MARGIN + PAD_Y + index as f64 * ROW_H;
        fill_round(
            buf,
            x0 + MARK_COL,
            y + ROW_H - 0.5,
            rule_w,
            1.0 / scale,
            0.0,
            pal["LINE"],
            scale,
        );
    }
}

fn paint_more(
    buf: &mut PeekBuffer,
    more: usize,
    count: usize,
    scale: f64,
    pal: &ColorMap,
    x0: f64,
) {
    let label = if more > 1 {
        format!("y {more} más")
    } else {
        String::from("y 1 más")
    };
    let base =
        MARGIN + PAD_Y + count as f64 * ROW_H + (MORE_H + text_cap_height(FS_META, scale)) / 2.0;
    let top = text_baseline_top(&label, FS_META, base - 3.0, scale);
    draw_text(
        buf,
        &label,
        FS_META,
        pal["TEXT_3"],
        x0 + MARK_COL,
        top,
        scale,
    );
}

/// Rectángulo redondeado relleno, con borde analítico, sobre `buf`.
#[allow(clippy::too_many_arguments)] // firma del Python (`_fill_round`)
fn fill_round(
    buf: &mut PeekBuffer,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
    color: Color,
    scale: f64,
) {
    let x0 = (x * scale).floor().max(0.0) as usize;
    let y0 = (y * scale).floor().max(0.0) as usize;
    let x1 = ((x + w) * scale).ceil().min(buf.width as f64) as usize;
    let y1 = ((y + h) * scale).ceil().min(buf.height as f64) as usize;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let scale32 = scale as f32;
    let center_x = (x + w / 2.0) as f32;
    let center_y = (y + h / 2.0) as f32;
    let half_x = (w / 2.0 - r) as f32;
    let half_y = (h / 2.0 - r) as f32;
    let radius = r as f32;
    let alpha = color.alpha().map_or(1.0, |value| f64::from(value) / 255.0) as f32;
    let channels = color.channels().map(|value| f32::from(value) / 255.0);
    for py in y0..y1 {
        let gy = (py as f32 + 0.5) / scale32;
        for px in x0..x1 {
            let gx = (px as f32 + 0.5) / scale32;
            let qx = (gx - center_x).abs() - half_x;
            let qy = (gy - center_y).abs() - half_y;
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let a = (0.5 - distance * scale32).clamp(0.0, 1.0) * alpha;
            let index = (py * buf.width + px) * 4;
            for (channel, value) in channels.iter().enumerate() {
                let destination = buf.data[index + channel];
                buf.data[index + channel] = *value * a + destination * (1.0 - a);
            }
            let destination = buf.data[index + 3];
            buf.data[index + 3] = a + destination * (1.0 - a);
        }
    }
}

/// Indicadores de estado (los únicos que cambian de un cuadro a otro).
pub fn paint_marks(
    buf: &mut PeekBuffer,
    items: &[PeekItem],
    scale: f64,
    theme: &str,
    now: f64,
    reduced: bool,
    icons: &dyn IconPainter,
) {
    let ink = marks::ink(theme);
    let cx = MARGIN + PAD_X + 8.0;
    for (index, item) in items.iter().enumerate() {
        let cy = MARGIN + PAD_Y + index as f64 * ROW_H + ROW_H / 2.0;
        if item.state == marks::WORKING {
            let step = if reduced {
                0
            } else {
                ((now * super::SPIN_STEPS as f64) as i64).rem_euclid(super::SPIN_STEPS as i64)
                    as usize
            };
            let sprite = spinner(scale, Color::Rgb(ink.mint), step, MARK_ICON - 1.0);
            blit(buf, &sprite, cx * scale, cy * scale);
            continue;
        }
        let (name, tone) = mark_for(&item.state);
        let color = Color::Rgb(tone_color(&ink, tone));
        icons.draw_icon(buf, name, cx, cy, MARK_ICON, color, scale);
    }
}

fn mark_for(state: &str) -> (&'static str, &'static str) {
    match state {
        marks::NEEDS_YOU => ("alert", "amber"),
        marks::FAILED => ("close", "red"),
        marks::WAITING => ("clock", "muted"),
        marks::STALE => ("minus", "muted"),
        marks::DONE => ("check", "mint"),
        _ => ("minus", "muted"),
    }
}

fn tone_color(ink: &marks::Tints, tone: &str) -> [u8; 3] {
    match tone {
        "mint" => ink.mint,
        "amber" => ink.amber,
        "red" => ink.red,
        _ => ink.muted,
    }
}

fn blit(buf: &mut PeekBuffer, sprite: &PeekBuffer, cx: f64, cy: f64) {
    let sh = sprite.height as i64;
    let sw = sprite.width as i64;
    let x0 = (cx - sw as f64 / 2.0).round_ties_even() as i64;
    let y0 = (cy - sh as f64 / 2.0).round_ties_even() as i64;
    if x0 < 0 || y0 < 0 || x0 + sw > buf.width as i64 || y0 + sh > buf.height as i64 {
        return;
    }
    for y in 0..sh as usize {
        for x in 0..sw as usize {
            let source = sprite.pixel(x, y);
            let index = ((y0 as usize + y) * buf.width + (x0 as usize + x)) * 4;
            let inverse = 1.0 - source[3];
            for (channel, value) in source.iter().take(3).enumerate() {
                buf.data[index + channel] = *value + buf.data[index + channel] * inverse;
            }
            buf.data[index + 3] = source[3] + buf.data[index + 3] * inverse;
        }
    }
}
