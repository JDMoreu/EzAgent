//! Iconos de la barra lateral de ajustes (recorte de `icons.py`, F2-11).
//!
//! `ez-gfx::icons` aún está vacío (lo porta F2-11), así que este submódulo
//! rasteriza solo las cinco figuras que usa la barra lateral con el mismo
//! procedimiento analítico de `icons.py`: rejilla de 24 unidades, cobertura
//! `radio − distancia + 0,5` y composición premultiplicada. Cuando F2-11 esté,
//! `draw_icon` se sustituye por el del crate y este archivo se borra.

use ez_gfx::sdf::Layer;

use crate::tokens::Color;

const GRID: f32 = 24.0;
const STROKE: f64 = 1.8;
const STROKE_SMALL: f64 = 1.6;
const SMALL_THRESHOLD: f64 = 18.0;

/// Trazos y rellenos de una figura en la rejilla de 24 × 24.
#[derive(Default)]
struct Pen {
    segments: Vec<[f32; 4]>,
    fills: Vec<[f32; 5]>,
}

impl Pen {
    fn segment(&mut self, from: (f64, f64), to: (f64, f64)) {
        self.segments
            .push([from.0 as f32, from.1 as f32, to.0 as f32, to.1 as f32]);
    }

    fn line(&mut self, points: &[(f64, f64)]) {
        for pair in points.windows(2) {
            self.segment(pair[0], pair[1]);
        }
        if let [only] = points {
            self.segment(*only, *only);
        }
    }

    fn closed(&mut self, points: &[(f64, f64)]) {
        let mut all = points.to_vec();
        if let Some(first) = points.first() {
            all.push(*first);
        }
        self.line(&all);
    }

    /// Arco elíptico; ángulos en grados, horario desde las 3 en punto (y abajo).
    fn arc(&mut self, bounds: [f64; 4], start: f64, end: f64) {
        let [x0, y0, x1, y1] = bounds;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let steps = (((end - start).abs() / 6.0) as i32).max(6);
        let points: Vec<(f64, f64)> = (0..=steps)
            .map(|index| {
                let angle = (start + (end - start) * index as f64 / steps as f64).to_radians();
                (cx + rx * angle.cos(), cy + ry * angle.sin())
            })
            .collect();
        self.line(&points);
    }

    fn ellipse(&mut self, bounds: [f64; 4]) {
        self.arc(bounds, 0.0, 360.0);
    }

    fn rounded_rect(&mut self, bounds: [f64; 4], radius: f64) {
        let [x0, y0, x1, y1] = bounds;
        let radius = radius.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
        let mut points = Vec::with_capacity(28);
        for (cx, cy, start) in [
            (x1 - radius, y0 + radius, -90.0),
            (x1 - radius, y1 - radius, 0.0),
            (x0 + radius, y1 - radius, 90.0),
            (x0 + radius, y0 + radius, 180.0),
        ] {
            for index in 0..7 {
                let angle = (start + 90.0 * index as f64 / 6.0).to_radians();
                points.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
            }
        }
        self.closed(&points);
    }
}

/// Figura de `icons.py` que usa la barra lateral (desconocida: vacía).
fn glyph(name: &str) -> Pen {
    let mut pen = Pen::default();
    match name {
        "sparkle" => pen.closed(&[
            (12.0, 3.4),
            (14.1, 9.9),
            (20.6, 12.0),
            (14.1, 14.1),
            (12.0, 20.6),
            (9.9, 14.1),
            (3.4, 12.0),
            (9.9, 9.9),
        ]),
        "mic-line" => {
            pen.rounded_rect([9.0, 3.4, 15.0, 13.4], 3.0);
            pen.arc([5.4, 6.0, 18.6, 18.0], 0.0, 180.0);
            pen.line(&[(12.0, 18.0), (12.0, 20.6)]);
        }
        "voice-line" => {
            for (x, half) in [
                (5.0, 2.6),
                (9.0, 6.0),
                (13.0, 8.6),
                (17.0, 5.0),
                (21.0, 2.2),
            ] {
                pen.line(&[(x - 1.0, 12.0 - half), (x - 1.0, 12.0 + half)]);
            }
        }
        "chip" => {
            pen.rounded_rect([6.4, 6.4, 17.6, 17.6], 2.4);
            for t in [9.6, 14.4] {
                pen.line(&[(t, 3.2), (t, 6.4)]);
                pen.line(&[(t, 17.6), (t, 20.8)]);
                pen.line(&[(3.2, t), (6.4, t)]);
                pen.line(&[(17.6, t), (20.8, t)]);
            }
        }
        "sliders" => {
            for (y, knob) in [(7.0, 15.0), (12.0, 9.0), (17.0, 13.5)] {
                pen.line(&[(4.5, y), (knob - 2.2, y)]);
                pen.line(&[(knob + 2.2, y), (19.5, y)]);
                pen.ellipse([knob - 2.1, y - 2.1, knob + 2.1, y + 2.1]);
            }
        }
        _ => {}
    }
    pen
}

fn segment_distance(px: f32, py: f32, segments: &[[f32; 4]]) -> f32 {
    let mut best = f32::INFINITY;
    for segment in segments {
        let (ax, ay) = (segment[0], segment[1]);
        let (dx, dy) = (segment[2] - ax, segment[3] - ay);
        let length = (dx * dx + dy * dy).max(1e-9);
        let (rx, ry) = (px - ax, py - ay);
        let t = ((rx * dx + ry * dy) / length).clamp(0.0, 1.0);
        let (qx, qy) = (rx - t * dx, ry - t * dy);
        best = best.min((qx * qx + qy * qy).sqrt());
    }
    best
}

fn rounded_rect_distance(px: f32, py: f32, bounds: &[f32; 5]) -> f32 {
    let [x0, y0, x1, y1, radius] = *bounds;
    let (half_w, half_h) = ((x1 - x0) / 2.0 - radius, (y1 - y0) / 2.0 - radius);
    let qx = (px - (x0 + x1) / 2.0).abs() - half_w;
    let qy = (py - (y0 + y1) / 2.0).abs() - half_h;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

/// Cobertura (0..1) de la figura en un lienzo de `side` px físicos.
fn mask(name: &str, side: usize, stroke: f64) -> Vec<f32> {
    let pen = glyph(name);
    let k = side as f32 / GRID;
    let radius = (stroke / 2.0) as f32;
    let mut out = vec![0.0f32; side * side];
    for row in 0..side {
        let py = (row as f32 + 0.5) / k;
        for column in 0..side {
            let px = (column as f32 + 0.5) / k;
            let mut cover = 0.0f32;
            if !pen.segments.is_empty() {
                let distance = segment_distance(px, py, &pen.segments);
                cover = (radius - distance * k + 0.5).clamp(0.0, 1.0);
            }
            for fill in &pen.fills {
                let distance = rounded_rect_distance(px, py, fill);
                cover = cover.max((0.5 - distance * k).clamp(0.0, 1.0));
            }
            out[row * side + column] = cover;
        }
    }
    out
}

/// Dibuja `name` centrado en `(cx, cy)` sobre un búfer float32 premultiplicado.
///
/// `size` es el lado lógico de la caja de 24 × 24; `cx`/`cy` también son
/// lógicos. `color` admite RGB o RGBA 0..255 y `alpha` permite fundidos.
#[allow(clippy::too_many_arguments)]
// DECISION: el Python lanza `ValueError` con argumentos fuera de rango y con
// nombres desconocidos; aquí el llamador sustituye el nombre y los valores
// inválidos no dibujan nada (la barra lateral nunca los produce).
pub(super) fn draw_icon(
    buf: &mut Layer,
    name: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: Color,
    scale: f64,
    alpha: f64,
) {
    if !(0.0..=1.0).contains(&alpha) || alpha <= 0.0 || size <= 0.0 || scale <= 0.0 {
        return;
    }
    let side = ((size * scale).round_ties_even() as i64).max(1) as usize;
    let stroke = if size < SMALL_THRESHOLD {
        STROKE_SMALL
    } else {
        STROKE
    };
    let stroke = (stroke * scale).max(1.0);
    let coverage = mask(name, side, stroke);
    let rgba = super::rgba(color);
    let (red, green, blue) = (
        rgba.r as f32 / 255.0,
        rgba.g as f32 / 255.0,
        rgba.b as f32 / 255.0,
    );
    let color_alpha = rgba.a as f32 / 255.0;
    let x0 = (cx * scale - side as f64 / 2.0).round_ties_even() as i64;
    let y0 = (cy * scale - side as f64 / 2.0).round_ties_even() as i64;
    let (dst_w, dst_h) = (buf.width() as i64, buf.height() as i64);
    let (src_x, src_y) = ((-x0).max(0), (-y0).max(0));
    let (dst_x, dst_y) = (x0.max(0), y0.max(0));
    let copy_w = (side as i64 - src_x).min(dst_w - dst_x);
    let copy_h = (side as i64 - src_y).min(dst_h - dst_y);
    if copy_w <= 0 || copy_h <= 0 {
        return;
    }
    let width = buf.width() as usize;
    let raw = buf.as_raw_mut();
    for row in 0..copy_h as usize {
        for column in 0..copy_w as usize {
            let cover = coverage[(src_y as usize + row) * side + src_x as usize + column];
            let alpha_source = color_alpha * cover * alpha as f32;
            let source = [
                red * color_alpha * cover,
                green * color_alpha * cover,
                blue * color_alpha * cover,
            ];
            let index = ((dst_y as usize + row) * width + dst_x as usize + column) * 4;
            let pixel = &mut raw[index..index + 4];
            for channel in 0..3 {
                pixel[channel] =
                    source[channel] * alpha as f32 + pixel[channel] * (1.0 - alpha_source);
            }
            pixel[3] = alpha_source + pixel[3] * (1.0 - alpha_source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::Color;

    fn pixel(layer: &Layer, x: usize, y: usize) -> [f32; 4] {
        let index = (y * layer.width() as usize + x) * 4;
        layer.as_raw()[index..index + 4]
            .try_into()
            .expect("4 canales")
    }

    #[test]
    fn every_sidebar_glyph_has_ink_and_keeps_its_border_clear() {
        for name in ["sparkle", "mic-line", "voice-line", "chip", "sliders"] {
            let cover = mask(name, 18, 1.8);
            let total: f32 = cover.iter().sum();
            assert!(total > 5.0, "{name} no dibuja: {total}");
            assert!(total < 250.0, "{name} llena el lienzo: {total}");
            assert_eq!(cover[0], 0.0, "{name} toca la esquina");
            assert_eq!(cover[17 * 18 + 17], 0.0, "{name} toca la esquina");
        }
        assert!(mask("no-existe", 18, 1.8).iter().all(|value| *value == 0.0));
    }

    #[test]
    fn the_glyph_masks_match_the_python_rasterizer() {
        // icons.icon_mask(...) ejecutado con el Python del proyecto.
        let sparkle = mask("sparkle", 18, 1.8);
        assert!((sparkle[9 * 18 + 9] - 0.0).abs() < 1e-6);
        assert!((sparkle[2 * 18 + 9] - 0.908_843).abs() < 1e-5);
        let sliders = mask("sliders", 18, 1.8);
        assert!((sliders[9 * 18 + 9] - 0.9).abs() < 1e-5);
        assert!((sliders[7 * 18 + 9] - 0.122_953).abs() < 1e-5);
    }

    #[test]
    fn drawing_composites_centred_and_clipped() {
        let mut buf = Layer::new(40, 40);
        draw_icon(
            &mut buf,
            "sliders",
            20.0,
            20.0,
            18.0,
            Color::Rgb([228, 242, 240]),
            1.0,
            1.0,
        );
        let centre = pixel(&buf, 20, 20);
        assert!(centre[3] > 0.0);
        assert_eq!(pixel(&buf, 1, 1), [0.0; 4]);
        assert!(pixel(&buf, 20, 20)[3] > 0.85);
        assert_eq!(pixel(&buf, 2, 2)[3], 0.0);

        // Un icono pegado al borde recorta sin salirse del búfer.
        let mut small = Layer::new(6, 6);
        draw_icon(
            &mut small,
            "sparkle",
            0.0,
            0.0,
            18.0,
            Color::Rgba([255, 255, 255, 128]),
            1.0,
            1.0,
        );
        assert!(small.as_raw().iter().any(|value| *value > 0.0));

        let before = small.as_raw().to_vec();
        draw_icon(
            &mut small,
            "sparkle",
            3.0,
            3.0,
            18.0,
            Color::Rgb([1, 2, 3]),
            1.0,
            0.0,
        );
        assert_eq!(small.as_raw(), before.as_slice());
    }
}
