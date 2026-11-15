//! Textos y medidas de la vista rápida (`peek_paint.py`).

use ez_core::pyvalue::{head_chars, py_rstrip};
use ez_gfx::text::{self as gfx_text, Align, FloatSurface, TextOptions};

use crate::tokens::{Color, FontStyle};

use super::{PeekBuffer, AGENT_NAMES, FS_TASK, LINE_GAP, MORE_H, PAD_Y, ROW_H};

/// «ahora», «N min» o «N h» a partir de un tiempo en segundos.
pub fn age(seconds: f64) -> String {
    let minutes = (seconds.max(0.0) / 60.0).floor() as i64;
    if minutes < 1 {
        return String::from("ahora");
    }
    if minutes < 60 {
        return format!("{minutes} min");
    }
    format!("{} h", minutes / 60)
}

/// «Estado · Agente · tiempo»: lo vivo cuenta desde que empezó; lo demás, «hace».
pub fn meta_line(item: &super::PeekItem, now: f64) -> String {
    let mut parts = vec![String::from(item.label())];
    if !item.agent.is_empty() {
        let agent = ez_core::pyvalue::py_strip(&item.agent);
        let name = AGENT_NAMES
            .iter()
            .find(|(key, _)| *key == agent.to_lowercase())
            .map_or_else(
                || {
                    let mut chars = agent.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_uppercase().collect::<String>()
                    }) + chars.as_str()
                },
                |(_, name)| String::from(*name),
            );
        parts.push(name);
    }
    let elapsed = age(now - item.since);
    parts.push(if item.live || elapsed == "ahora" {
        elapsed
    } else {
        format!("hace {elapsed}")
    });
    parts.join(" · ")
}

/// Recorta con «…» para que quepa en `max_w` px lógicos.
pub fn fit(text: &str, style: FontStyle, max_w: f64, scale: f64) -> String {
    if text_size(text, style, scale) <= max_w {
        return text.to_string();
    }
    let (mut lo, mut hi) = (0usize, text.chars().count());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate = format!("{}…", py_rstrip(head_chars(text, mid)));
        if text_size(&candidate, style, scale) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    format!("{}…", py_rstrip(head_chars(text, lo)))
}

/// Alto de la tarjeta con `rows` filas y, si hay más, la línea «y N más».
/// Fila bajo un punto de la ventana (px físicos), o `None`.
pub fn row_at(rows: usize, x: f64, y: f64, scale: f64) -> Option<usize> {
    let lx = x / scale - super::MARGIN;
    let ly = y / scale - super::MARGIN - super::PAD_Y;
    if !(0.0..=super::PEEK_W).contains(&lx) || ly < 0.0 {
        return None;
    }
    let index = (ly / super::ROW_H).floor() as usize;
    (index < rows).then_some(index)
}

/// Clave de la fila cuya × está bajo el punto, o `None`.
pub fn dismiss_at(items: &[super::PeekItem], x: f64, y: f64, scale: f64) -> Option<String> {
    let index = row_at(items.len(), x, y, scale)?;
    if !super::dismissible(&items[index]) {
        return None;
    }
    let lx = x / scale - super::MARGIN;
    (lx >= super::PEEK_W - super::PAD_X - super::DISMISS_W + 8.0).then(|| items[index].key.clone())
}

pub fn card_height(rows: usize, more: bool) -> f64 {
    PAD_Y * 2.0 + rows as f64 * ROW_H + if more { MORE_H } else { 0.0 }
}

/// Líneas base (título, estado) dentro de una fila: el bloque queda centrado.
///
/// El aire sobre las mayúsculas del título es el mismo que bajo la línea de estado.
pub fn baselines(scale: f64) -> (f64, f64) {
    let cap = text_cap_height(FS_TASK, scale);
    let first = (ROW_H - LINE_GAP + cap) / 2.0;
    (first, first + LINE_GAP)
}

pub(super) fn gfx_style(style: FontStyle) -> gfx_text::FontStyle {
    gfx_text::FontStyle {
        token: style.token,
        family: style.family,
        size: style.size,
        weight: style.weight,
        tracking_em: style.tracking_em,
        uppercase: style.uppercase,
    }
}

pub(super) fn text_size(text: &str, style: FontStyle, scale: f64) -> f64 {
    gfx_text::text_size(text, gfx_style(style), scale, None, "left")
        .expect("estilo, escala y alineación válidos")
        .0
}

pub(super) fn text_cap_height(style: FontStyle, scale: f64) -> f64 {
    gfx_text::cap_height(gfx_style(style), scale).expect("estilo y escala válidos")
}

pub(super) fn text_baseline_top(text: &str, style: FontStyle, baseline: f64, scale: f64) -> f64 {
    gfx_text::baseline_top(text, gfx_style(style), baseline, scale)
        .expect("estilo y escala válidos")
}

pub(super) fn draw_text(
    buf: &mut PeekBuffer,
    text: &str,
    style: FontStyle,
    color: Color,
    x: f64,
    y: f64,
    scale: f64,
) {
    let (width, height) = (buf.width, buf.height);
    let mut surface = FloatSurface::new(&mut buf.data, width, height);
    gfx_text::draw_text(
        &mut surface,
        text,
        gfx_style(style),
        gfx_text::Color::Rgb(color.channels()),
        (x, y),
        TextOptions {
            scale,
            align: Align::Left,
            tracking: None,
            alpha: 1.0,
        },
    )
    .expect("texto y búfer válidos");
}
