//! Colocación y fundido de la tarjeta de la vista rápida (`peek.py`).

use crate::paint::peek::{PeekBuffer, GAP, MARGIN, RISE};

/// Esquina de la ventana de la tarjeta y si va encima del hub.
///
/// Se centra en todo el hub (forma y botones). Si así se saliera de la pantalla,
/// alinea su borde con el borde del hub de ese lado, no con la esquina de la
/// pantalla: sigue «dentro» del hub. Va encima del hub en la mitad inferior de la
/// pantalla y debajo en la superior, y cambia de lado si no cabe.
pub fn place_card(
    hub: (f64, f64, f64, f64),
    size: (i32, i32),
    work: (i32, i32, i32, i32),
    scale: f64,
) -> (i32, i32, bool) {
    let (hx0, hy0, hx1, hy1) = hub;
    let (w, h) = size;
    let (left, top, right, bottom) = (
        f64::from(work.0),
        f64::from(work.1),
        f64::from(work.2),
        f64::from(work.3),
    );
    let m = (MARGIN * scale).round_ties_even();
    let card_w = f64::from(w) - 2.0 * m;
    let card_h = f64::from(h) - 2.0 * m;
    let gap = GAP * scale;
    let mut x = (hx0 + hx1) / 2.0 - card_w / 2.0; // borde izquierdo de la tarjeta (sin sombra)
    if x + card_w > right {
        x = hx1.min(right) - card_w;
    }
    if x < left {
        x = hx0.max(left);
    }
    x = x.min(right - card_w); // una tarjeta más ancha que el hub nunca sale por la derecha
    let mut above = (hy0 + hy1) / 2.0 > (top + bottom) / 2.0;
    if above && hy0 - gap - card_h < top {
        above = false;
    } else if !above && hy1 + gap + card_h > bottom {
        above = true;
    }
    let y = if above { hy0 - gap - card_h } else { hy1 + gap };
    (
        (x.round_ties_even() - m) as i32,
        (y.round_ties_even() - m) as i32,
        above,
    )
}

/// Fundido y leve desplazamiento hacia la forma; nunca se reescala el contenido.
///
/// Devuelve RGBA premultiplicado en `float32`; `win::peek` lo cuantiza para la
/// ventana *layered*.
pub fn compose(buf: &PeekBuffer, p: f64, above: bool, scale: f64, reduced: bool) -> PeekBuffer {
    let shift = if reduced {
        0.0
    } else {
        (RISE * (1.0 - p) * scale).round_ties_even()
    };
    let (width, height) = (buf.width, buf.height);
    let mut out = PeekBuffer::new(width, height);
    let shift = shift as usize;
    if shift != 0 && above {
        if shift < height {
            let copy = (height - shift) * width * 4;
            out.data[shift * width * 4..].copy_from_slice(&buf.data[..copy]);
        }
    } else if shift != 0 {
        if shift < height {
            let copy = (height - shift) * width * 4;
            out.data[..copy].copy_from_slice(&buf.data[shift * width * 4..]);
        }
    } else {
        out.data.copy_from_slice(&buf.data);
    }
    let fade = p as f32;
    for value in &mut out.data {
        *value *= fade;
    }
    out
}
