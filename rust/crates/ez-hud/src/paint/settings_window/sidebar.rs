//! Barra lateral de la ventana de ajustes: marca viva, título y lista de
//! secciones con icono (`Sidebar` de `settings_window.py`).

use ndarray::Array2;

use ez_gfx::sdf::{fill_round_rect, Layer};

use crate::paint::flow::{colors_for, FlowField};
use crate::tokens::{ColorMap, FS_MICRO, FS_ROW};

use super::icons;
use super::{
    cap_top, draw_label, hline, hud_style, palette_color, rgba, vline, ICON_SIZE, ITEM_RADIUS,
    ITEM_X0, ITEM_X1, LABEL_CY, MARK_CX, MARK_D, MARK_FROM_BOTTOM, SECTION_ICONS, SIDEBAR_W,
    WORD_AGENT, WORD_EZ,
};

// Geometría de las gotas de la marca (`brand.py`).
const DROP_R: f64 = 0.125;
const DROP_SPREAD: f64 = 0.28;
const DROP_Y: f64 = 0.60;
const DROP_TAIL: (f64, f64) = (0.075, -0.30);
const DROP_BEND: (f64, f64) = (-0.115, -0.20);
const DROP_TAPER: f64 = 0.80;
const DROP_STEPS: usize = 220;
/// DECISION: 4 % de aire alrededor de la silueta para que el borde no se recorte.
const DROP_AIR: f64 = 1.04;

/// Barra lateral: marca viva, título y lista de secciones con icono.
pub struct Sidebar {
    flow: FlowField,
    last: f64,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    /// `Sidebar()`: fluido en círculo con la semilla 7.
    pub fn new() -> Self {
        Self {
            flow: FlowField::new("circle", None, 7),
            last: 0.0,
        }
    }

    /// Dibuja la barra lateral. `with_mark=false` deja la marca para
    /// [`Self::overlay_mark`].
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        buf: &mut Layer,
        scale: f64,
        pal: &ColorMap,
        _theme: &str,
        selected: &str,
        hover: Option<&str>,
        now: f64,
        reduced: bool,
        with_mark: bool,
    ) {
        if with_mark {
            self.tick(now, reduced);
        }
        self.draw_header(buf, scale, pal);
        let cy = buf.height() as f64 / scale - MARK_FROM_BOTTOM;
        self.draw_brand(buf, scale, pal, cy, with_mark);
        self.draw_sections(buf, scale, pal, selected, hover);
        // Separador vertical entre la barra lateral y el contenido.
        vline(
            buf,
            palette_color(pal, "LINE"),
            scale,
            SIDEBAR_W - 1.0 / scale,
            16.0,
            buf.height() as f64 / scale - 16.0,
        );
    }

    /// «Ajustes» arriba, en la micro-tipografía del HUD.
    fn draw_header(&self, buf: &mut Layer, scale: f64, pal: &ColorMap) {
        draw_label(
            buf,
            "Ajustes",
            hud_style(FS_MICRO),
            palette_color(pal, "TEXT_3"),
            ITEM_X0 + 13.0,
            cap_top(LABEL_CY, FS_MICRO.size),
            scale,
        );
    }

    /// Las cinco secciones, siempre a la vista: elegir una no esconde las demás.
    fn draw_sections(
        &self,
        buf: &mut Layer,
        scale: f64,
        pal: &ColorMap,
        selected: &str,
        hover: Option<&str>,
    ) {
        for (index, (name, title)) in super::sections().iter().enumerate() {
            let (x0, y0, x1, y1) = super::item_rect(index as i32);
            let center = (y0 + y1) / 2.0;
            let color = if *name == selected {
                fill_round_rect(
                    buf,
                    [x0, y0, x1, y1],
                    ITEM_RADIUS,
                    rgba(palette_color(pal, "MINT")),
                    scale,
                    0.14,
                );
                palette_color(pal, "MINT")
            } else {
                if Some(*name) == hover {
                    fill_round_rect(
                        buf,
                        [x0, y0, x1, y1],
                        ITEM_RADIUS,
                        rgba(palette_color(pal, "GROUP_HOVER")),
                        scale,
                        1.0,
                    );
                }
                palette_color(pal, "TEXT_2")
            };
            let icon = SECTION_ICONS
                .iter()
                .find(|(section, _)| section == name)
                .map_or("sliders", |(_, icon)| *icon);
            icons::draw_icon(buf, icon, x0 + 22.0, center, ICON_SIZE, color, scale, 1.0);
            draw_label(
                buf,
                title,
                hud_style(FS_ROW),
                color,
                x0 + 42.0,
                cap_top(center, FS_ROW.size),
                scale,
            );
        }
    }

    /// Avanza el fluido hasta `now` (máximo 0,1 s por paso, como el Python).
    pub fn tick(&mut self, now: f64, reduced: bool) {
        let dt = if self.last == 0.0 {
            0.0
        } else {
            0.1f64.min((now - self.last).max(0.0))
        };
        self.last = now;
        self.flow.step(dt, 0.0, "idle", reduced);
    }

    /// Pie de la barra: línea, marca y palabra-marca, centradas en `cy`.
    fn draw_brand(&self, buf: &mut Layer, scale: f64, pal: &ColorMap, cy: f64, with_mark: bool) {
        hline(
            buf,
            palette_color(pal, "LINE"),
            scale,
            cy - super::FOOT_H / 2.0,
            ITEM_X0,
            ITEM_X1,
        );
        if with_mark {
            self.draw_mark(buf, scale, cy);
        }
        let mut x = ITEM_X0 + 42.0;
        let top = cap_top(cy, WORD_EZ.size);
        // El ancho devuelto es el de la tinta (sin márgenes laterales): +0,6 px de aire.
        x += draw_label(
            buf,
            "Ez",
            WORD_EZ,
            palette_color(pal, "MINT"),
            x,
            top,
            scale,
        )
        .0 + 0.6;
        draw_label(
            buf,
            "Agent",
            WORD_AGENT,
            palette_color(pal, "TEXT"),
            x,
            top,
            scale,
        );
    }

    /// Pinta la marca viva sobre un cuadro ya terminado (BGRA uint8, premultiplicado).
    ///
    /// Es lo único de la ventana que se mueve cuando no pasa nada más: así no
    /// hace falta recomponer la tarjeta entera para animarla.
    #[allow(clippy::too_many_arguments)]
    pub fn overlay_mark(
        &mut self,
        out: &mut [u8],
        width: usize,
        height: usize,
        scale: f64,
        now: f64,
        opacity: f64,
        reduced: bool,
    ) {
        self.tick(now, reduced);
        let k = opacity.clamp(0.0, 1.0);
        if k <= 0.0 {
            return;
        }
        let cy = height as f64 / scale - MARK_FROM_BOTTOM;
        let mark = self.mark_pixels(scale);
        let x0 = ((MARK_CX - MARK_D / 2.0) * scale).round_ties_even() as i64;
        let y0 = ((cy - MARK_D / 2.0) * scale).round_ties_even() as i64;
        let copy_h = (mark.height() as i64).min(height as i64 - y0);
        let copy_w = (mark.width() as i64).min(width as i64 - x0);
        if copy_h <= 0 || copy_w <= 0 || x0 < 0 || y0 < 0 {
            return;
        }
        let mark_width = mark.width() as usize;
        for row in 0..copy_h as usize {
            for column in 0..copy_w as usize {
                let source = &mark.as_raw()[(row * mark_width + column) * 4..][..4];
                let alpha = source[3];
                let top = [
                    source[0] * k as f32,
                    source[1] * k as f32,
                    source[2] * k as f32,
                    alpha * k as f32,
                ];
                let index = ((y0 as usize + row) * width + x0 as usize + column) * 4;
                let region = &mut out[index..index + 4];
                let bgra = [
                    top[2] * 255.0,
                    top[1] * 255.0,
                    top[0] * 255.0,
                    top[3] * 255.0,
                ];
                for channel in 0..4 {
                    let blended = bgra[channel] + region[channel] as f32 * (1.0 - top[3]);
                    region[channel] = (blended + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    /// RGBA premultiplicado (float32, 0..1) de la marca con el fotograma actual
    /// del fluido.
    fn mark_pixels(&self, scale: f64) -> Layer {
        let size = 8usize.max((MARK_D * scale).round_ties_even() as usize);
        let axis = linspace(size);
        let mut gx = Array2::<f32>::zeros((size, size));
        let mut gy = Array2::<f32>::zeros((size, size));
        for row in 0..size {
            for column in 0..size {
                gx[[row, column]] = axis[column];
                gy[[row, column]] = axis[row];
            }
        }
        // DECISION: malla de ±0.5 y paleta clara siempre, igual que el icono: así
        // el borde luminoso del círculo queda fuera de las gotas y la marca no
        // cambia con el tema.
        let rgb = self.flow.render(
            &gx.view(),
            &gy.view(),
            "light",
            &colors_for("light", "idle"),
        );
        let coverage = drops_coverage(size);
        let mut mark = Layer::new(size as u32, size as u32);
        for row in 0..size {
            for column in 0..size {
                let cover = coverage[row * size + column];
                let index = (row * size + column) * 4;
                let pixel = &mut mark.as_raw_mut()[index..index + 4];
                for channel in 0..3 {
                    let value = (rgb[[row, column, channel]] as f32).clamp(0.0, 255.0) / 255.0;
                    pixel[channel] = value * cover;
                }
                pixel[3] = cover;
            }
        }
        mark
    }

    /// La marca de EzAgent (las gotas del icono) llena de Marea en movimiento.
    fn draw_mark(&self, buf: &mut Layer, scale: f64, cy: f64) {
        let x0 = ((MARK_CX - MARK_D / 2.0) * scale).round_ties_even() as i64;
        let y0 = ((cy - MARK_D / 2.0) * scale).round_ties_even() as i64;
        if x0 < 0 || y0 < 0 {
            return;
        }
        super::composite_over(buf, &self.mark_pixels(scale), x0 as usize, y0 as usize);
    }
}

/// `np.linspace(-0.5, 0.5, size, dtype=np.float32)`.
fn linspace(size: usize) -> Vec<f32> {
    let divisor = size.saturating_sub(1).max(1) as f64;
    (0..size)
        .map(|index| (-0.5 + index as f64 / divisor) as f32)
        .collect()
}

/// Una gota de la marca: cabeza y cola como una cadena de círculos.
fn drop_points() -> Vec<(f64, f64, f64)> {
    let mut points = Vec::with_capacity(2 * (DROP_STEPS + 1));
    let half = DROP_SPREAD / 2.0 + 0.005;
    for head_x in [0.5 - half, 0.5 + half] {
        let tip = (head_x + DROP_TAIL.0, DROP_Y + DROP_TAIL.1);
        let bend = (head_x + DROP_BEND.0, DROP_Y + DROP_BEND.1);
        for index in 0..=DROP_STEPS {
            let t = index as f64 / DROP_STEPS as f64;
            let one = 1.0 - t;
            let x = one * one * head_x + 2.0 * one * t * bend.0 + t * t * tip.0;
            let y = one * one * DROP_Y + 2.0 * one * t * bend.1 + t * t * tip.1;
            points.push((x, y, DROP_R * (1.0 - DROP_TAPER * t.powf(0.85))));
        }
    }
    points
}

/// Cobertura (0..1, float32) de las gotas recortadas a su caja, en `size`×`size`.
///
/// La caja es cuadrada y está centrada en el centro visual de las gotas, así
/// que se alinea con texto o iconos sin retoques.
// shortcut: cobertura analítica en vez del ráster Pillow + LANCZOS de
// `brand.drops_coverage`; el borde puede diferir en ~1 px, F2-11 lo sustituye.
fn drops_coverage(size: usize) -> Vec<f32> {
    let points = drop_points();
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for (x, y, radius) in &points {
        left = left.min(x - radius);
        top = top.min(y - radius);
        right = right.max(x + radius);
        bottom = bottom.max(y + radius);
    }
    let half = (right - left).max(bottom - top) / 2.0 * DROP_AIR;
    let (center_x, center_y) = ((left + right) / 2.0, (top + bottom) / 2.0);
    let side = 2.0 * half;
    let mut coverage = vec![0.0f32; size * size];
    for row in 0..size {
        for column in 0..size {
            let x = center_x - half + (column as f64 + 0.5) / size as f64 * side;
            let y = center_y - half + (row as f64 + 0.5) / size as f64 * side;
            let mut nearest = f64::INFINITY;
            for (px, py, radius) in &points {
                nearest = nearest.min((x - px).hypot(y - py) - radius);
            }
            coverage[row * size + column] =
                (0.5 - nearest * size as f64 / side).clamp(0.0, 1.0) as f32;
        }
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::card_palette;

    fn pixel(layer: &Layer, x: usize, y: usize) -> [f32; 4] {
        let index = (y * layer.width() as usize + x) * 4;
        layer.as_raw()[index..index + 4]
            .try_into()
            .expect("4 canales")
    }

    #[test]
    fn the_sidebar_draws_title_selection_hover_divider_and_brand() {
        let mut buf = Layer::new(760, 620);
        let mut sidebar = Sidebar::new();
        let pal = card_palette("dark");
        sidebar.draw(
            &mut buf,
            1.0,
            &pal,
            "dark",
            "appearance",
            Some("voice"),
            1.0,
            true,
            true,
        );
        // Valores del Python: fondo MINT al 14 %, GROUP_HOVER opaco y LINE al 24/255.
        let selected = pixel(&buf, 150, 84);
        for (channel, expected) in selected.iter().zip([0.051_61, 0.124_08, 0.107_61, 0.14]) {
            assert!((channel - expected).abs() < 1e-4, "{selected:?}");
        }
        let hovered = pixel(&buf, 150, 172);
        for (channel, expected) in hovered.iter().zip([0.109_8, 0.176_47, 0.2, 1.0]) {
            assert!((channel - expected).abs() < 1e-4, "{hovered:?}");
        }
        let divider = pixel(&buf, 183, 300);
        for (channel, expected) in divider.iter().zip([0.084_15, 0.089_32, 0.088_58, 0.094_12]) {
            assert!((channel - expected).abs() < 1e-4, "{divider:?}");
        }
        assert_eq!(pixel(&buf, 184, 300)[3], 0.0);
        // «Ajustes» y las cinco secciones dejan tinta.
        let ink = (0..40)
            .flat_map(|row| (0..SIDEBAR_W as usize).map(move |column| (column, row)))
            .filter(|(column, row)| pixel(&buf, *column, *row)[3] > 0.0)
            .count();
        assert!(ink > 40, "{ink}");
    }

    #[test]
    fn the_sidebar_skips_the_mark_when_asked() {
        let pal = card_palette("dark");
        let without = {
            let mut buf = Layer::new(300, 400);
            Sidebar::new().draw(
                &mut buf,
                1.0,
                &pal,
                "dark",
                "appearance",
                None,
                1.0,
                true,
                false,
            );
            buf
        };
        let with = {
            let mut buf = Layer::new(300, 400);
            Sidebar::new().draw(
                &mut buf,
                1.0,
                &pal,
                "dark",
                "appearance",
                None,
                1.0,
                true,
                true,
            );
            buf
        };
        let mark_region = |layer: &Layer| {
            (359..=381)
                .flat_map(|row| (24..=48).map(move |column| (column, row)))
                .filter(|(column, row)| pixel(layer, *column, *row)[3] > 0.0)
                .count()
        };
        assert_eq!(mark_region(&without), 0);
        assert!(mark_region(&with) > 0);
        let alpha = (359..=381)
            .flat_map(|row| (25..=47).map(move |column| (column, row)))
            .map(|(column, row)| pixel(&with, column, row)[3])
            .fold(0.0f32, f32::max);
        assert!(alpha > 0.99, "la marca llega a opaca: {alpha}");
        assert_ne!(with.as_raw(), without.as_raw());
    }

    #[test]
    fn the_mark_overlay_blends_into_a_byte_frame() {
        let mut frame = vec![0u8; 200 * 200 * 4];
        let mut sidebar = Sidebar::new();
        sidebar.overlay_mark(&mut frame, 200, 200, 1.0, 1.0, 1.0, true);
        let alpha = |x: usize, y: usize| frame[(y * 200 + x) * 4 + 3];
        assert!(alpha(32, 172) > 10, "la marca se pinta en el pie");
        assert_eq!(alpha(150, 20), 0);
        // Opacidad 0 no toca el cuadro.
        let mut untouched = vec![7u8; 200 * 200 * 4];
        let before = untouched.clone();
        sidebar.overlay_mark(&mut untouched, 200, 200, 1.0, 1.0, 0.0, true);
        assert_eq!(untouched, before);
        // Un cuadro diminuto no entra en pánico.
        let mut tiny = vec![0u8; 4];
        sidebar.overlay_mark(&mut tiny, 1, 1, 1.0, 1.0, 1.0, true);
    }

    #[test]
    fn the_drops_coverage_is_a_soft_silhouette() {
        let coverage = drops_coverage(22);
        assert_eq!(coverage.len(), 22 * 22);
        let total: f32 = coverage.iter().sum();
        assert!(total > 80.0 && total < 22.0 * 22.0);
        assert!(coverage.iter().all(|value| (0.0..=1.0).contains(value)));
        assert_eq!(coverage[0], 0.0);
        assert_eq!(coverage[21 * 22 + 21], 0.0);
        // Dos cabezas: el centro de la caja cae entre ellas.
        assert!(coverage[11 * 22 + 11] < 0.5);
        let axis = linspace(22);
        assert!((axis[0] + 0.5).abs() < 1e-6);
        assert!((axis[21] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_fluid_only_advances_with_a_previous_frame() {
        let mut sidebar = Sidebar::new();
        sidebar.tick(5.0, false);
        assert_eq!(sidebar.last, 5.0);
        assert_eq!(sidebar.flow.t, 0.0);
        sidebar.tick(5.5, false);
        assert!((sidebar.flow.t - 0.05 * 0.55).abs() < 1e-9);
        let frozen = sidebar.flow.clone();
        sidebar.tick(9.0, true);
        assert_eq!(sidebar.flow, frozen);
        assert_eq!(sidebar.last, 9.0);
    }
}
