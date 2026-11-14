//! Port of `src/ezagent/voice/hud/flow.py` (F5-02).
//!
//! «Marea»: el gradiente orgánico del interior de la barra. Un campo de ruido
//! con deformación de dominio se mapea sobre una paleta (verde profundo, agua,
//! menta/lima) con algún filamento coral. No es un orbe de nubes ni un borde
//! luminoso: la identidad es el líquido en movimiento.
//!
//! Reacciona: en reposo se pliega despacio; al hablar tú, la presión acelera el
//! flujo y lo empuja hacia arriba desde abajo; cuando habla EzAgent, los pulsos
//! de luz viajan con su voz; al pensar, todo gira; en pausa va lento y apagado,
//! y el movimiento reducido lo congela.
//!
//! Coordenadas: unidades de media altura de la forma, `y` crece hacia abajo,
//! `x` en `[-aspect, aspect]` e `y` en `[-1, 1]`. Todo el cálculo va en `f32`
//! con el mismo orden de operaciones que NumPy.

use ndarray::{Array2, Array3, ArrayView2};

use ez_core::pyvalue::py_strip;

pub use crate::model::drop::{colors_for, Palette, CUSTOM_POSES};

mod rng;

use rng::{lattice, Pcg64};

/// Temas de la paleta.
pub const THEMES: [&str; 2] = ["light", "dark"];

/// Poses con paleta propia.
pub const POSES: [&str; 7] = ["idle", "listen", "think", "speak", "ask", "error", "mute"];

// DECISION: el `dict` `PRESETS` se porta como tabla ordenada; el orden de
// inserción de Python es el de esta constante.
/// Paletas base del tema personalizado: `(fondo, principal, secundario, acento)`.
pub const PRESETS: [(&str, [&str; 4]); 5] = [
    ("marea", ["#0E6F7C", "#1FAE9F", "#8FE3C0", "#FF8A65"]),
    ("atardecer", ["#5B1A3A", "#E0564F", "#FFB36B", "#FFE08A"]),
    ("bosque", ["#13361F", "#2E7D4F", "#A8D672", "#F2C94C"]),
    ("lavanda", ["#2E2A6B", "#7A6FF0", "#C9B8FF", "#FF9EC4"]),
    ("medianoche", ["#0B1026", "#2D3A8C", "#5FA8FF", "#9BF2FF"]),
];

/// Etiqueta visible de cada preset.
pub const PRESET_LABELS: [(&str, &str); 5] = [
    ("marea", "Marea"),
    ("atardecer", "Atardecer"),
    ("bosque", "Bosque"),
    ("lavanda", "Lavanda"),
    ("medianoche", "Medianoche"),
];

/// `COLORS[theme]`: las paletas de todas las poses, en el orden de [`POSES`].
// DECISION: el `dict` de NumPy se expone como función sobre la tabla ya
// portada en `model::drop::palette`, sin duplicar los 70 colores.
pub fn colors(theme: &str) -> [Palette; POSES.len()] {
    POSES.map(|pose| colors_for(theme, pose))
}

/// `#RRGGBB` (o `RRGGBB`, `#RGB`) -> RGB 0..255; `None` si no vale.
pub fn parse_hex(value: &str) -> Option<[f32; 3]> {
    let text: Vec<char> = py_strip(value).trim_start_matches('#').chars().collect();
    let text: Vec<char> = if text.len() == 3 {
        text.into_iter()
            .flat_map(|character| [character; 2])
            .collect()
    } else {
        text
    };
    if text.len() != 6 {
        return None;
    }
    let mut out = [0.0f32; 3];
    for (index, slot) in out.iter_mut().enumerate() {
        let pair: String = text[index * 2..index * 2 + 2].iter().collect();
        *slot = u8::from_str_radix(py_strip(&pair), 16).ok()? as f32;
    }
    Some(out)
}

/// Los cuatro colores del usuario como paleta de Marea `(5, 3)`, o `None`.
pub fn custom_palette(
    primary: &str,
    secondary: &str,
    accent: &str,
    background: &str,
) -> Option<Palette> {
    let deep = parse_hex(background)?;
    let mid = parse_hex(primary)?;
    let bright = parse_hex(secondary)?;
    let spark = parse_hex(accent)?;
    // La luz: el secundario casi blanco.
    let glow = [
        bright[0] + (255.0 - bright[0]) * 0.75,
        bright[1] + (255.0 - bright[1]) * 0.75,
        bright[2] + (255.0 - bright[2]) * 0.75,
    ];
    Some([deep, mid, bright, spark, glow])
}

// ------------------------------------------------------------------ simulación
/// Velocidad a la que se pliega el fluido en reposo.
pub const FLOW_SPEED: f64 = 0.55;
/// Frecuencia espacial del fluido (menos = formas más grandes y calmas).
pub const SCALE: f64 = 0.55;
/// El círculo es pequeño: un poco más de detalle dentro.
pub const CIRCLE_SCALE: f64 = 0.9;
/// Cuánto se pliega el fluido sobre sí mismo.
pub const WARP: f64 = 1.7;
/// Unidades por segundo que sube el fluido mientras hablas.
pub const RISE: f64 = 0.9;
/// rad/s del remolino mientras piensa.
pub const SPIN: f64 = 0.55;
/// rad/s de los pulsos de voz de EzAgent.
pub const WAVE_SPEED: f64 = 5.0;
/// Segundos: la presión de la voz sube rápido...
pub const ATTACK: f64 = 0.07;
/// ...y se apaga despacio, para que el fluido siga un momento.
pub const RELEASE: f64 = 0.7;

/// Gradiente vivo que llena la forma y reacciona a las dos voces.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowField {
    pub layout: String,
    pub aspect: f64,
    pub origin: [f32; 2],
    pub scale: f64,
    pub t: f64,
    pub pressure: f64,
    pub wave: f64,
    /// Flujo ascendente acumulado de tu voz.
    pub lift: f64,
    /// Giro mientras piensa.
    pub angle: f64,
    pub spin_rate: f64,
    pub mode: String,
}

impl Default for FlowField {
    fn default() -> Self {
        Self::new("pill", None, 7)
    }
}

impl FlowField {
    /// `FlowField(layout="pill", aspect=None, seed=7)`.
    // DECISION: la semilla es `u64`; el Python admite enteros arbitrarios, pero
    // todo el producto usa 7 y `SeedSequence` con 64 bits cubre cualquier uso real.
    pub fn new(layout: &str, aspect: Option<f64>, seed: u64) -> Self {
        let mut rng = Pcg64::new(seed);
        let origin = [
            (rng.next_double() * 60.0) as f32,
            (rng.next_double() * 60.0) as f32,
        ];
        Self {
            layout: layout.to_string(),
            aspect: aspect.unwrap_or(if layout == "circle" { 1.0 } else { 3.9 }),
            origin,
            scale: if layout == "circle" {
                CIRCLE_SCALE
            } else {
                SCALE
            },
            t: 0.0,
            pressure: 0.0,
            wave: 0.0,
            lift: 0.0,
            angle: 0.0,
            spin_rate: 0.0,
            mode: "idle".to_string(),
        }
    }

    /// Dónde entra tu voz al fluido: el centro de abajo de la forma.
    pub fn inlet(&self, _mode: &str) -> (f64, f64) {
        (0.0, 1.25)
    }

    /// Avanza el fluido `dt` segundos. `level` es la voz (0..1).
    pub fn step(&mut self, dt: f64, level: f64, mode: &str, reduced: bool) {
        if reduced || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.05);
        self.mode = mode.to_string();
        let air = if mode == "listen" || mode == "speak" {
            level
        } else {
            0.0
        };
        let tau = if air > self.pressure { ATTACK } else { RELEASE };
        self.pressure += (air - self.pressure) * (1.0 - (-dt / tau).exp());
        let calm = match mode {
            "mute" => 0.25,
            "sleep" => 0.35,
            "think" => 1.5,
            _ => 1.0,
        };
        self.t += dt * FLOW_SPEED * calm * (1.0 + 2.5 * self.pressure);
        self.wave += dt * WAVE_SPEED * (0.6 + self.pressure);
        if mode == "listen" {
            self.lift += dt * RISE * self.pressure;
        }
        let target = if mode == "think" { SPIN } else { 0.0 };
        self.spin_rate += (target - self.spin_rate) * (1.0 - (-dt / 0.6).exp());
        self.angle += self.spin_rate * dt;
    }

    /// Valor del fluido `f` y máscara de filamentos `r` (ambos 0..1) en la rejilla.
    ///
    /// El Python mezcla precisiones: las coordenadas de la primera octava son
    /// `float32`, y la interpolación del ruido y la deformación del dominio
    /// acaban en `float64` (NumPy promociona `float32 - int32`). Se reproduce
    /// ese orden para que los valores coincidan bit a bit.
    pub fn field(&self, gx: &ArrayView2<f32>, gy: &ArrayView2<f32>) -> (Array2<f64>, Array2<f64>) {
        assert_eq!(gx.shape(), gy.shape(), "las dos rejillas deben coincidir");
        let mut values = Array2::<f64>::zeros(gx.raw_dim());
        let mut filaments = Array2::<f64>::zeros(gx.raw_dim());
        for (index, &x) in gx.indexed_iter() {
            let (value, filament) = self.field_at(x, gy[index]);
            values[index] = value;
            filaments[index] = filament;
        }
        (values, filaments)
    }

    fn field_at(&self, gx: f32, gy: f32) -> (f64, f64) {
        let ca = self.angle.cos() as f32;
        let sa = self.angle.sin() as f32;
        let x = gx * ca - gy * sa;
        let y = gx * sa + gy * ca;
        let scale = self.scale as f32;
        let px = x * scale + self.origin[0];
        let py = (y + self.lift as f32) * scale + self.origin[1];
        let t = self.t;
        let q1 = fbm32(px + (t * 0.12) as f32, py - (t * 0.07) as f32);
        let q2 = fbm32(px + 5.2 - (t * 0.09) as f32, py + 1.3 + (t * 0.05) as f32);
        let k = WARP + 1.0 * self.pressure;
        let base_x = px as f64;
        let base_y = py as f64;
        let r1 = fbm64(base_x + k * q1 + 1.7 + t * 0.15, base_y + k * q2 + 9.2);
        let r2 = fbm64(base_x + k * q1 + 8.3 - t * 0.11, base_y + k * q2 + 2.8);
        let f = fbm64(base_x + 2.2 * r1, base_y + 2.2 * r2);
        (((f - 0.22) / 0.56).clamp(0.0, 1.0), r1)
    }

    /// Imagen RGB flotante (0..255) del interior de la forma en la rejilla.
    ///
    /// Como en NumPy, la paleta se mezcla en `float64` porque la rampa sale
    /// del campo (ya en doble precisión).
    pub fn render(
        &self,
        gx: &ArrayView2<f32>,
        gy: &ArrayView2<f32>,
        _theme: &str,
        colors: &Palette,
    ) -> Array3<f64> {
        let (values, filaments) = self.field(gx, gy);
        let (deep, mid, bright, accent, glow) =
            (colors[0], colors[1], colors[2], colors[3], colors[4]);
        let mut img = Array3::<f64>::zeros((gx.nrows(), gx.ncols(), 3));
        for (index, &x) in gx.indexed_iter() {
            let y = gy[index];
            let value = values[index];
            let a = smooth(0.0, 0.55, value);
            let b = smooth(0.45, 1.0, value);
            let mut pixel = [0.0f64; 3];
            for channel in 0..3 {
                let mut level = deep[channel] as f64 * (1.0 - a) + mid[channel] as f64 * a;
                level = level * (1.0 - b) + bright[channel] as f64 * b;
                pixel[channel] = level;
            }
            // Los filamentos cálidos donde el fluido se pliega más: la firma
            // coral. Un «screen» aclara como la luz, así el coral sobre el
            // verde se vuelve melocotón en vez de barro.
            let spark = smooth(0.64, 0.8, filaments[index]) * (0.45 + 0.3 * self.pressure);
            for channel in 0..3 {
                let screen =
                    255.0 - (255.0 - pixel[channel]) * (255.0 - accent[channel] as f64) / 255.0;
                pixel[channel] += (screen - pixel[channel]) * spark;
            }
            // Luz suave desde arriba: un brillo de vidrio en lo alto.
            let sheen = ((-y).clamp(0.0, 1.0)).powi(2) * 0.14;
            for channel in 0..3 {
                pixel[channel] += (glow[channel] as f64 - pixel[channel]) * sheen as f64;
            }
            if self.pressure > 1e-3 {
                let light = self.voice_light(x, y, value);
                for channel in 0..3 {
                    pixel[channel] += (glow[channel] as f64 - pixel[channel]) * light;
                }
            }
            img[[index.0, index.1, 0]] = pixel[0];
            img[[index.0, index.1, 1]] = pixel[1];
            img[[index.0, index.1, 2]] = pixel[2];
        }
        img
    }

    fn voice_light(&self, gx: f32, gy: f32, fluid: f64) -> f64 {
        if self.mode == "speak" {
            // La voz de EzAgent: bandas de luz que viajan por la forma.
            let phase = (gx * 1.4 - self.wave as f32) as f64 + fluid * 2.5;
            let band = 0.5 + 0.5 * phase.sin();
            band.powf(3.0) * self.pressure * 0.45
        } else {
            // Tu voz: una entrada luminosa que sube desde el centro de abajo.
            let (inlet_x, inlet_y) = self.inlet(&self.mode);
            let dx = (gx - inlet_x as f32) / (self.aspect * 0.9) as f32;
            let dy = (gy - inlet_y as f32) / 0.85;
            let distance = dx * dx + dy * dy;
            ((-distance).exp() * self.pressure as f32 * 0.6) as f64
        }
    }
}

/// Ruido de valor suave en [0, 1].
///
/// La resta `x - xi` y la interpolación van en `f64` (NumPy promociona
/// `float32 - int32`); las diferencias entre esquinas, en `f32`, como los
/// escalares del array.
fn value_noise(x: f64, y: f64) -> f64 {
    let floor_x = x.floor();
    let floor_y = y.floor();
    let fx = x - floor_x;
    let fy = y - floor_y;
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let xi = floor_x as i32;
    let yi = floor_y as i32;
    let x0 = (xi & 255) as usize;
    let x1 = ((xi + 1) & 255) as usize;
    let y0 = (yi & 255) as usize;
    let y1 = ((yi + 1) & 255) as usize;
    let lattice = lattice();
    let a = lattice[y0 * 256 + x0];
    let b = lattice[y0 * 256 + x1];
    let c = lattice[y1 * 256 + x0];
    let d = lattice[y1 * 256 + x1];
    let corner = ((a - b) - c) + d;
    a as f64 + (b - a) as f64 * u + (c - a) as f64 * v + corner as f64 * u * v
}

/// `_fbm` con coordenadas `float32` (primera octava del campo).
fn fbm32(x: f32, y: f32) -> f64 {
    0.6 * value_noise(x as f64, y as f64)
        + 0.3 * value_noise((x * 2.03 + 3.1) as f64, (y * 2.03 + 1.7) as f64)
        + 0.1 * value_noise((x * 4.1 + 7.3) as f64, (y * 4.1 + 2.9) as f64)
}

/// `_fbm` con coordenadas `float64` (deformación del dominio y valor final).
fn fbm64(x: f64, y: f64) -> f64 {
    0.6 * value_noise(x, y)
        + 0.3 * value_noise(x * 2.03 + 3.1, y * 2.03 + 1.7)
        + 0.1 * value_noise(x * 4.1 + 7.3, y * 4.1 + 2.9)
}

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let k = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: usize, columns: usize) -> (Array2<f32>, Array2<f32>) {
        let xs = [-1.5f32, -0.5, 0.5, 1.5];
        let ys = [-1.0f32, 0.0, 1.0];
        let mut gx = Array2::<f32>::zeros((rows, columns));
        let mut gy = Array2::<f32>::zeros((rows, columns));
        for row in 0..rows {
            for column in 0..columns {
                gx[[row, column]] = xs[column % xs.len()];
                gy[[row, column]] = ys[row % ys.len()];
            }
        }
        (gx, gy)
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn the_field_keeps_the_numpy_seed_and_origin() {
        let field = FlowField::new("circle", None, 7);
        assert_eq!(field.aspect, 1.0);
        assert_eq!(field.scale, CIRCLE_SCALE);
        assert_eq!(field.origin, [37.505_73, 53.832_83]);
        let pill = FlowField::new("pill", Some(3.9), 7);
        assert_eq!(pill.aspect, 3.9);
        assert_eq!(pill.scale, SCALE);
        assert_eq!(pill.origin, field.origin);
        assert_eq!(FlowField::default().layout, "pill");
    }

    #[test]
    fn the_field_values_match_the_python_oracle() {
        // flow.FlowField("circle", seed=7).field(gx, gy) en la rejilla de 3×4.
        let expected_f = [
            0.250_461_42,
            0.278_300_83,
            0.647_452_6,
            0.718_210_55,
            0.630_404_7,
            0.681_941_66,
            0.917_688_8,
            0.753_142_2,
            0.243_136_21,
            0.430_651_73,
            0.103_601_085,
            1.0,
        ];
        let expected_r = [
            0.161_424_5,
            0.713_554_5,
            0.657_004_2,
            0.439_872_8,
            0.517_111_05,
            0.679_075_85,
            0.844_127_1,
            0.663_132_34,
            0.561_373_64,
            0.664_123_5,
            0.514_998_37,
            0.518_063_57,
        ];
        let (gx, gy) = grid(3, 4);
        let (values, filaments) = FlowField::new("circle", None, 7).field(&gx.view(), &gy.view());
        for (index, expected) in expected_f.iter().enumerate() {
            close(values[[index / 4, index % 4]], *expected, 1e-6);
            close(filaments[[index / 4, index % 4]], expected_r[index], 1e-6);
        }
    }

    #[test]
    fn the_render_matches_the_python_oracle() {
        // flow.FlowField("circle", seed=7).render(...) en la rejilla de 3×4.
        let expected = [
            [26.508_19, 90.826_31, 94.671_51],
            [68.899_27, 110.009_88, 109.388_49],
            [40.111_965, 145.270_69, 141.794_23],
        ];
        let (gx, gy) = grid(3, 4);
        let field = FlowField::new("circle", None, 7);
        let image = field.render(&gx.view(), &gy.view(), "dark", &colors_for("dark", "idle"));
        for (index, expected) in expected.iter().enumerate() {
            for channel in 0..3 {
                close(image[[0, index, channel]], expected[channel], 1e-4);
            }
        }
    }

    #[test]
    fn the_voice_pressure_and_the_motion_match_python() {
        let mut field = FlowField::new("circle", None, 7);
        for _ in 0..10 {
            field.step(1.0 / 60.0, 0.5, "listen", false);
        }
        close(field.t, 0.167_568_058_335_430_6, 1e-9);
        close(field.pressure, 0.453_768_761_968_54, 1e-9);
        close(field.lift, 0.049_680_910_910_463_644, 1e-9);
        assert_eq!(field.mode, "listen");
        let frozen = field.clone();
        field.step(1.0, 1.0, "speak", true);
        assert_eq!(field, frozen);
        field.step(0.0, 1.0, "speak", false);
        assert_eq!(field, frozen);
        field.step(1.0, 0.0, "think", false);
        assert!(field.spin_rate > 0.0 && field.angle > 0.0);
    }

    #[test]
    fn the_speaking_bands_match_python() {
        let mut field = FlowField::new("pill", Some(3.9), 7);
        for _ in 0..20 {
            field.step(1.0 / 60.0, 1.0, "speak", false);
        }
        close(field.pressure, 0.991_450_690_520_314, 1e-9);
        close(field.wave, 2.359_331_613_795_415_4, 1e-9);
        let (gx, gy) = grid(3, 4);
        let image = field.render(
            &gx.view(),
            &gy.view(),
            "light",
            &colors_for("light", "speak"),
        );
        close(image[[0, 0, 0]], 133.196_932_221_179_88, 1e-9);
        close(image[[0, 1, 1]], 193.438_014_878_447_55, 1e-9);
        close(image[[0, 2, 2]], 160.853_428_716_517_44, 1e-9);
    }

    #[test]
    fn hex_colours_and_custom_palettes_follow_python() {
        assert_eq!(parse_hex("#0E6F7C"), Some([14.0, 111.0, 124.0]));
        assert_eq!(parse_hex("abc"), Some([170.0, 187.0, 204.0]));
        assert_eq!(parse_hex(" #ABC "), Some([170.0, 187.0, 204.0]));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#GGGGGG"), None);
        assert_eq!(parse_hex(""), None);
        let custom = custom_palette("#1FAE9F", "#8FE3C0", "#FF8A65", "#0E6F7C")
            .expect("cuatro colores válidos");
        assert_eq!(custom[0], [14.0, 111.0, 124.0]);
        assert_eq!(custom[3], [255.0, 138.0, 101.0]);
        assert_eq!(custom[4], [227.0, 248.0, 239.25]);
        assert_eq!(
            custom_palette("#1FAE9F", "nope", "#FF8A65", "#0E6F7C"),
            None
        );
    }

    #[test]
    fn the_preset_tables_keep_their_order_and_labels() {
        assert_eq!(PRESETS[0].0, "marea");
        assert_eq!(PRESETS[0].1[0], "#0E6F7C");
        assert_eq!(PRESETS[4].0, "medianoche");
        assert_eq!(PRESET_LABELS[4].1, "Medianoche");
        assert_eq!(THEMES, ["light", "dark"]);
        assert_eq!(POSES.len(), 7);
        assert_eq!(CUSTOM_POSES.len(), 7);
        let idle = colors_for("light", "idle");
        assert_eq!(idle[0], [14.0, 111.0, 124.0]);
        let table = colors("dark");
        assert_eq!(table[0], colors_for("dark", "idle"));
        assert_eq!(table[6], colors_for("dark", "mute"));
        assert_eq!(colors("auto")[2], colors_for("dark", "think"));
    }

    #[test]
    fn the_inlet_is_the_bottom_centre() {
        let field = FlowField::default();
        assert_eq!(field.inlet("listen"), (0.0, 1.25));
        assert_eq!(field.inlet("speak"), (0.0, 1.25));
    }
}
