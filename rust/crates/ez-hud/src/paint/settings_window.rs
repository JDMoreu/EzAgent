//! Port of `src/ezagent/voice/hud/settings_window.py` (F5-12).
//!
//! Ventana de ajustes: movible, redimensionable y con cinco secciones. Se abre
//! en el centro de la pantalla (o donde se dejó la última vez) con una barra
//! lateral de secciones (Aspecto, Escucha, Voz, Modelos, Sistema); a la derecha
//! se ve solo la sección elegida, dibujada por el `Menu` de siempre (mismos
//! controles, edición en línea y guardado). Se arrastra desde cualquier zona sin
//! controles, se redimensiona desde los bordes y las esquinas, y un doble clic
//! en la cabecera devuelve el tamaño y la posición de fábrica.
//!
//! Módulo puro de geometría y dibujo (sin Win32): `win/settings_window.rs` lo
//! usa para componer el cuadro y repartir los clics entre la barra lateral, los
//! bordes y el menú.

use ez_gfx::sdf::{fill_circle, Layer};
use ez_gfx::text::{draw_text, FloatSurface, FontStyle, TextOptions};
use ez_gfx::Rgba;

use crate::layout::{self, Group, Row, FOOTER, GROUPS, PAD_BOTTOM, SIDEBAR_ORDER};
use crate::tokens::{self, Color, ColorMap, FLARE};

mod icons;
mod sidebar;

pub use sidebar::Sidebar;

// ------------------------------------------------------------------ medidas
/// px lógicos de la barra lateral.
pub const SIDEBAR_W: f64 = 184.0;
/// Tamaño de fábrica de la ventana (lógico).
pub const DEFAULT_W: f64 = 760.0;
pub const DEFAULT_H: f64 = 620.0;
pub const MIN_W: f64 = 560.0;
pub const MIN_H: f64 = 400.0;
/// Alto de la ventana por encima del alto visible del menú (borde inferior).
pub const WINDOW_EXTRA_H: f64 = 4.0;

/// El cuerpo de la tarjeta se dibuja `FLARE` px dentro de la ventana (margen
/// transparente a izquierda y derecha); los bordes de agarre viven dentro del
/// cuerpo.
pub const BODY_INSET: f64 = FLARE as f64;
/// Grosor de la zona de agarre de un borde.
pub const EDGE: f64 = 7.0;
/// Lado de la zona de agarre de una esquina.
pub const CORNER: f64 = 20.0;
/// Franja superior: doble clic = tamaño de fábrica.
pub const HEADER_H: f64 = 52.0;
/// px de la ventana que siempre quedan en pantalla.
pub const KEEP_VISIBLE: f64 = 96.0;

/// Justo bajo la franja de cabecera.
pub const ITEMS_TOP: f64 = 64.0;
pub const ITEM_H: f64 = 40.0;
pub const ITEM_GAP: f64 = 4.0;
pub const ITEM_X0: f64 = 14.0;
pub const ITEM_X1: f64 = SIDEBAR_W - 10.0;
pub const ITEM_RADIUS: f64 = 12.0;
pub const ICON_SIZE: f64 = 18.0;
/// Un poco mayor que `ICON_SIZE`: las gotas llevan aire alrededor.
pub const MARK_D: f64 = 22.0;
/// Misma columna que los iconos de sección.
pub const MARK_CX: f64 = ITEM_X0 + 22.0;
/// Del centro de la marca al borde inferior de la ventana.
pub const MARK_FROM_BOTTOM: f64 = 30.0;
/// Alto de la fila del pie: como un elemento de la lista.
pub const FOOT_H: f64 = 40.0;
/// Eje de «AJUSTES»: el mismo que el título y el ✕ del contenido.
pub const LABEL_CY: f64 = 26.0;

/// `draw_text` recorta al contorno de la tinta: su `y` es la parte alta de las
/// mayúsculas, no una línea de texto. Para centrar texto con un icono se centra
/// la altura de mayúsculas (Geist y Geist Mono la tienen en ~0,714 em).
pub const CAP_RATIO: f64 = 0.714;

/// DECISION: el nombre es una palabra-marca de dos pesos de Geist (la misma
/// familia del sistema): «Ez» en semibold con el acento, «Agent» en medium con
/// el texto.
pub const WORD_EZ: FontStyle = FontStyle {
    token: "WORD_EZ",
    family: "Geist",
    size: 15.0,
    weight: 600,
    tracking_em: -0.01,
    uppercase: false,
};

/// «Agent» en medium con el texto.
pub const WORD_AGENT: FontStyle = FontStyle {
    token: "WORD_AGENT",
    family: "Geist",
    size: 15.0,
    weight: 500,
    tracking_em: -0.01,
    uppercase: false,
};

/// Icono de cada sección.
pub const SECTION_ICONS: [(&str, &str); 5] = [
    ("appearance", "sparkle"),
    ("listening", "mic-line"),
    ("voice", "voice-line"),
    ("models", "chip"),
    ("system", "sliders"),
];

/// Cursores de Windows (`IDC_*`) para cada borde.
pub const IDC_SIZEWE: i32 = 32644;
pub const IDC_SIZENS: i32 = 32645;
pub const IDC_SIZENWSE: i32 = 32642;
pub const IDC_SIZENESW: i32 = 32643;

/// `(izquierda, arriba, derecha, abajo)`.
pub type Edges = (bool, bool, bool, bool);
/// Área de trabajo física `(left, top, right, bottom)`.
pub type WorkArea = (i32, i32, i32, i32);
/// Rectángulo físico `(x, y, w, h)`.
pub type Pixels = (i32, i32, i32, i32);

// ---------------------------------------------------------------- secciones
/// `(id, título)` de cada sección, en el orden de la barra lateral.
pub fn sections() -> Vec<(&'static str, String)> {
    SIDEBAR_ORDER
        .iter()
        .map(|name| (*name, section_title(name)))
        .collect()
}

/// Grupos que muestra el menú para la sección `name` (o la primera).
pub fn section_groups(name: &str) -> Vec<(&'static str, &'static [Row])> {
    let wanted = layout::section_groups(name)
        .filter(|groups| !groups.is_empty())
        .or_else(|| layout::section_groups(SIDEBAR_ORDER[0]))
        .unwrap_or(&[]);
    wanted
        .iter()
        .filter_map(|group| find_group(group))
        .collect()
}

fn find_group(name: &str) -> Option<Group> {
    GROUPS
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(id, rows)| (*id, *rows))
}

/// `SECTION_TITLES.get(name, name.title())`.
pub fn section_title(name: &str) -> String {
    layout::section_title(name).map_or_else(|| py_title(name), str::to_string)
}

/// `str.title()`: mayúscula tras un carácter que no es letra, el resto en
/// minúscula; los nombres de sección solo llevan ASCII.
fn py_title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_is_letter = false;
    for character in text.chars() {
        if previous_is_letter {
            out.extend(character.to_lowercase());
        } else {
            out.extend(character.to_uppercase());
        }
        previous_is_letter = character.is_alphabetic();
    }
    out
}

// ----------------------------------------------------------------- geometría
/// `(ancho, alto visible)` lógicos del menú dentro de una ventana `win_w`×`win_h`.
pub fn content_size(win_w: f64, win_h: f64) -> (f64, f64) {
    (
        1.0f64.max(win_w - SIDEBAR_W),
        1.0f64.max(win_h - WINDOW_EXTRA_H),
    )
}

/// Tamaño lógico de la ventana de fábrica para un alto de contenido dado.
pub fn window_size(viewport_h: f64) -> (f64, f64) {
    (DEFAULT_W, viewport_h + WINDOW_EXTRA_H)
}

/// Tamaño lógico acotado entre el mínimo y el área de trabajo (con margen).
pub fn clamp_size(w: f64, h: f64, work: WorkArea, scale: f64) -> (f64, f64) {
    let max_w = MIN_W.max((work.2 - work.0) as f64 / scale - 16.0);
    let max_h = MIN_H.max((work.3 - work.1) as f64 / scale - 16.0);
    (MIN_W.max(w.min(max_w)), MIN_H.max(h.min(max_h)))
}

/// Posición física que centra una ventana `w`×`h` en el área de trabajo.
pub fn centered(work: WorkArea, w: i32, h: i32) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    (
        left + (right - left - w).div_euclid(2).max(0),
        top + (bottom - top - h).div_euclid(2).max(0),
    )
}

/// Mantiene a la vista una franja de la ventana (la cabecera siempre alcanzable).
pub fn clamp_position(x: i32, y: i32, w: i32, _h: i32, work: WorkArea, scale: f64) -> (i32, i32) {
    let keep = (KEEP_VISIBLE * scale).round_ties_even() as i32;
    let (left, top, right, bottom) = work;
    (
        (left - w + keep).max(x.min(right - keep)),
        top.max(y.min(bottom - keep)),
    )
}

/// Bordes que agarra el punto lógico `(x, y)` de una ventana `w`×`h`.
///
/// `None` si el punto no está en una zona de agarre. Las esquinas agarran dos
/// bordes a la vez. El cuerpo empieza `BODY_INSET` px dentro de la ventana.
pub fn resize_edges(x: f64, y: f64, w: f64, h: f64) -> Option<Edges> {
    let (left, right) = (BODY_INSET, w - BODY_INSET);
    if !(left - 1.0 <= x && x <= right + 1.0 && -1.0 <= y && y <= h + 1.0) {
        return None;
    }
    let mut edges = [
        x <= left + EDGE,
        y <= EDGE,
        x >= right - EDGE,
        y >= h - EDGE,
    ];
    let corner = [
        x <= left + CORNER,
        y <= CORNER,
        x >= right - CORNER,
        y >= h - CORNER,
    ];
    if (corner[0] || corner[2]) && (corner[1] || corner[3]) {
        for (edge, near) in edges.iter_mut().zip(corner) {
            *edge = *edge || near;
        }
    }
    edges
        .iter()
        .any(|edge| *edge)
        .then_some((edges[0], edges[1], edges[2], edges[3]))
}

/// Cursor (`IDC_*`) para los bordes agarrados; 0 si no hay ninguno.
pub fn cursor_for(edges: Option<Edges>) -> i32 {
    let Some((left, top, right, bottom)) = edges else {
        return 0;
    };
    let horizontal = left || right;
    let vertical = top || bottom;
    if horizontal && vertical {
        return if left == top {
            IDC_SIZENWSE
        } else {
            IDC_SIZENESW
        };
    }
    if horizontal {
        return IDC_SIZEWE;
    }
    if vertical {
        IDC_SIZENS
    } else {
        0
    }
}

/// Nuevo `(x, y, w, h)` físico al arrastrar `edges` `delta` píxeles.
///
/// `start` es el rectángulo al empezar; `limits` es `(min_w, min_h, max_w,
/// max_h)`. Los bordes izquierdo y superior mueven también el origen para que
/// el borde opuesto no se desplace.
pub fn resized_rect(edges: Edges, start: Pixels, delta: (i32, i32), limits: Pixels) -> Pixels {
    let (x0, y0, w0, h0) = start;
    let (dx, dy) = delta;
    let (min_w, min_h, max_w, max_h) = limits;
    let (left, top, right, bottom) = edges;
    let (mut x, mut y, mut w, mut h) = (x0, y0, w0, h0);
    if right {
        w = min_w.max((w0 + dx).min(max_w));
    } else if left {
        w = min_w.max((w0 - dx).min(max_w));
        x = x0 + (w0 - w);
    }
    if bottom {
        h = min_h.max((h0 + dy).min(max_h));
    } else if top {
        h = min_h.max((h0 - dy).min(max_h));
        y = y0 + (h0 - h);
    }
    (x, y, w, h)
}

/// ¿Está el punto lógico en la franja superior de la ventana?
pub fn in_header(_x: f64, y: f64) -> bool {
    (0.0..=HEADER_H).contains(&y)
}

// ---------------------------------------------------------------- barra lateral
/// `(x0, y0, x1, y1)` lógicos de la fila `index` de la lista de secciones.
pub fn item_rect(index: i32) -> (f64, f64, f64, f64) {
    let y0 = ITEMS_TOP + index as f64 * (ITEM_H + ITEM_GAP);
    (ITEM_X0, y0, ITEM_X1, y0 + ITEM_H)
}

/// Sección bajo el punto lógico `(x, y)` de la ventana, si la hay.
pub fn item_at(x: f64, y: f64) -> Option<&'static str> {
    if !(ITEM_X0..=ITEM_X1).contains(&x) {
        return None;
    }
    for (index, name) in SIDEBAR_ORDER.iter().enumerate() {
        let (_, y0, _, y1) = item_rect(index as i32);
        if y0 <= y && y <= y1 {
            return Some(*name);
        }
    }
    None
}

/// `src` sobre `dst` (float32 RGBA premultiplicado) en `(x0, y0)`.
pub fn composite_over(dst: &mut Layer, src: &Layer, x0: usize, y0: usize) {
    let height = (dst.height() as usize)
        .saturating_sub(y0)
        .min(src.height() as usize);
    let width = (dst.width() as usize)
        .saturating_sub(x0)
        .min(src.width() as usize);
    if height == 0 || width == 0 {
        return;
    }
    let (dst_w, src_w) = (dst.width() as usize, src.width() as usize);
    for row in 0..height {
        for column in 0..width {
            let src_index = (row * src_w + column) * 4;
            let dst_index = ((y0 + row) * dst_w + x0 + column) * 4;
            let top = &src.as_raw()[src_index..src_index + 4];
            let alpha = top[3];
            let region = &mut dst.as_raw_mut()[dst_index..dst_index + 4];
            for channel in 0..3 {
                region[channel] = top[channel] + region[channel] * (1.0 - alpha);
            }
            region[3] = alpha + region[3] * (1.0 - alpha);
        }
    }
}

/// `y` de `draw_text` para que las mayúsculas de `size` px queden centradas en
/// `center_y`.
pub fn cap_top(center_y: f64, size: f64) -> f64 {
    center_y - CAP_RATIO * size / 2.0
}

/// Mezcla el color de línea (premultiplicado) sobre un tramo de 1 px.
fn blend(span: &mut [f32], line: Color) {
    let alpha = line.alpha().map_or(0.1, |value| value as f64 / 255.0);
    let channels = line.channels();
    let color = [
        channels[0] as f64 / 255.0 * alpha,
        channels[1] as f64 / 255.0 * alpha,
        channels[2] as f64 / 255.0 * alpha,
        alpha,
    ];
    for pixel in span.as_chunks_mut::<4>().0 {
        for channel in 0..4 {
            pixel[channel] = color[channel] as f32 + pixel[channel] * (1.0 - alpha as f32);
        }
    }
}

fn row_span(buf: &mut Layer, y: i32, x0: i32, x1: i32) -> Option<&mut [f32]> {
    let height = buf.height() as i32;
    let width = buf.width() as i32;
    if y < 0 || y >= height || x0 < 0 || x1 <= x0 {
        return None;
    }
    let (x0, x1) = (x0.min(width), x1.min(width));
    if x1 <= x0 {
        return None;
    }
    let start = (y as usize * width as usize + x0 as usize) * 4;
    let end = (y as usize * width as usize + x1 as usize) * 4;
    Some(&mut buf.as_raw_mut()[start..end])
}

/// Línea horizontal de 1 px físicas en `y`, de `x0` a `x1` (px lógicos).
pub(crate) fn hline(buf: &mut Layer, line: Color, scale: f64, y: f64, x0: f64, x1: f64) {
    let row = (y * scale).round_ties_even() as i32;
    let start = (x0 * scale).round_ties_even() as i32;
    let end = (x1 * scale).round_ties_even() as i32;
    if let Some(span) = row_span(buf, row, start, end) {
        blend(span, line);
    }
}

/// Línea vertical de 1 px físicas en `x`, de `y0` a `y1` (px lógicos).
pub(crate) fn vline(buf: &mut Layer, line: Color, scale: f64, x: f64, y0: f64, y1: f64) {
    let column = (x * scale).round_ties_even() as i32;
    if column < 0 || column >= buf.width() as i32 {
        return;
    }
    let start = (y0 * scale).round_ties_even() as i32;
    let end = (y1 * scale).round_ties_even() as i32;
    let width = buf.width() as usize;
    let height = buf.height() as i32;
    let start = start.clamp(0, height);
    let end = end.clamp(0, height);
    let raw = buf.as_raw_mut();
    for row in start..end {
        let index = (row as usize * width + column as usize) * 4;
        blend(&mut raw[index..index + 4], line);
    }
}

/// `(color RGB o RGBA) -> Rgba` con alfa 255 cuando no lleva.
pub(crate) fn rgba(color: Color) -> Rgba {
    let channels = color.channels();
    Rgba::new(
        channels[0],
        channels[1],
        channels[2],
        color.alpha().unwrap_or(255),
    )
}

fn text_color(color: Color) -> ez_gfx::text::Color {
    match color {
        Color::Rgb(rgb) => ez_gfx::text::Color::Rgb(rgb),
        Color::Rgba(rgba) => ez_gfx::text::Color::Rgba(rgba),
    }
}

/// `draw_text` sobre un [`Layer`]; devuelve el tamaño lógico de la tinta.
pub(crate) fn draw_label(
    buf: &mut Layer,
    text: &str,
    style: FontStyle,
    color: Color,
    x: f64,
    y: f64,
    scale: f64,
) -> (f64, f64) {
    let (width, height) = (buf.width() as usize, buf.height() as usize);
    let mut surface = FloatSurface::new(buf.as_raw_mut(), width, height);
    draw_text(
        &mut surface,
        text,
        style,
        text_color(color),
        (x, y),
        TextOptions {
            scale,
            ..TextOptions::default()
        },
    )
    .expect("estilo y búfer del HUD")
}

/// Estilo del HUD resuelto por su token (las tablas coinciden con `tokens`).
pub(crate) fn hud_style(token_style: tokens::FontStyle) -> FontStyle {
    ez_gfx::text::font_style(token_style.token).expect("estilo del HUD registrado")
}

/// `pal[key]`: los llamadores pasan una paleta completa.
pub(crate) fn palette_color(pal: &ColorMap, key: &str) -> Color {
    match pal.get(key) {
        Some(color) => *color,
        None => panic!("la paleta no tiene la clave {key}"),
    }
}

/// Seis puntos en el pie, a la derecha de «Guardar»: «aquí se estira».
pub fn draw_grip(buf: &mut Layer, scale: f64, pal: &ColorMap) {
    let right = buf.width() as f64 / scale - BODY_INSET - 13.0;
    let cy = buf.height() as f64 / scale - WINDOW_EXTRA_H - PAD_BOTTOM as f64 - FOOTER as f64 / 2.0;
    let color = palette_color(pal, "TEXT_3");
    for row in 0..3 {
        for column in 0..3 - row {
            fill_circle(
                buf,
                right - column as f64 * 5.0,
                cy + 5.0 - row as f64 * 5.0,
                2.2,
                rgba(color),
                scale,
                0.55,
            );
        }
    }
}

/// Factor de opacidad (curva suave) para un fundido de `amount` entre 0 y 1.
pub fn fade_factor(amount: f64) -> f64 {
    let k = amount.clamp(0.0, 1.0);
    if k >= 1.0 {
        return 1.0;
    }
    // `math.isclose(k, 0.0)` con la tolerancia por defecto solo es cierto para
    // el cero exacto: `|k| <= 1e-9 · |k|`. Se reproduce tal cual.
    if k == 0.0 {
        0.0
    } else {
        k * k * (3.0 - 2.0 * k)
    }
}

/// Multiplica el cuadro entero (premultiplicado) por `amount` (fundido).
pub fn fade(buf: &mut Layer, amount: f64) {
    let factor = fade_factor(amount);
    if factor < 1.0 {
        for value in buf.as_raw_mut() {
            *value *= factor as f32;
        }
    }
}

#[cfg(test)]
mod tests;
