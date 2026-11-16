//! `DropModel`: animación pura de la barra (sin Win32 ni dibujo).

use std::collections::BTreeMap;

use crate::motion::{emerge, Choreo, Spring, REDUCED_FADE};

use super::fluid::FlowState;
use super::geometry::{
    clean_dock, emerge_order, slots, Point, Rect, BTN, BUTTONS, CIRCLE_D, HOVER_MARGIN, PILL_H,
    PILL_W,
};
use super::palette::{colors_for, Palette, CUSTOM_POSES};

/// Entrada de `DropModel.update`: una columna del Python, ya resuelta por la UI.
///
/// `buttons` es `"hover"` por defecto; `"always"` solo lo usan las pruebas.
#[derive(Clone, Copy, Debug)]
pub struct Update<'a> {
    pub now: f64,
    pub dt: f64,
    pub pose: &'a str,
    pub rms: f64,
    pub theme: &'a str,
    pub hover: bool,
    pub asking: bool,
    pub menu_open: bool,
    pub reduced: bool,
    pub buttons: &'a str,
    pub muted: bool,
    pub hover_item: Option<&'a str>,
    pub pressed: Option<&'a str>,
    pub in_call: bool,
    pub custom: Option<Palette>,
}

impl Default for Update<'_> {
    fn default() -> Self {
        Self {
            now: 0.0,
            dt: 0.0,
            pose: "idle",
            rms: 0.0,
            theme: "light",
            hover: false,
            asking: false,
            menu_open: false,
            reduced: false,
            buttons: "hover",
            muted: false,
            hover_item: None,
            pressed: None,
            in_call: true,
            custom: None,
        }
    }
}

/// Animación pura de la barra: sin Win32, sin Pillow, probable en aislamiento.
///
/// El contorno es fijo. Dentro fluye el gradiente (`FlowState`, el dibujo es de
/// F5-04) que la voz acelera; fuera, el leve crecimiento al acercar el puntero y
/// la coreografía de los botones. Sin llamada, el micrófono se pliega y ✕ se
/// convierte en el botón de llamar.
pub struct DropModel {
    pub layout: &'static str,
    pub dock: &'static str,
    pub field: FlowState,
    pub theme: &'static str,
    pub cols: Palette,
    /// El chat ocupa su lugar: solo se dibujan los botones.
    pub form_hidden: bool,
    pub bright: f64,
    pub lvl: f64,
    pub peak: f64,
    pub pose: String,
    pub pose_since: f64,
    pub ts: f64,
    pub reduced: bool,
    pub asking: bool,
    pub choreo: Choreo,
    /// 🎤 → ✓ al preguntar.
    pub ask: Spring,
    /// Raya del micro en pausa.
    pub mute_p: Spring,
    /// ✕ → llamar al colgar.
    pub call: Spring,
    /// Hueco del micro en la fila.
    pub mic_space: Spring,
    /// Paleta del tema personalizado (`custom_palette` de `flow.py`).
    pub custom: Option<Palette>,
    colored: bool,
}

impl Default for DropModel {
    fn default() -> Self {
        Self::new("pill", None)
    }
}

impl DropModel {
    pub fn new(layout: &str, dock: Option<&str>) -> Self {
        let layout = if layout == "circle" { "circle" } else { "pill" };
        let dock = clean_dock(layout, dock);
        Self {
            layout,
            dock,
            field: FlowState::new(layout),
            theme: "light",
            cols: colors_for("light", "idle"),
            form_hidden: false,
            bright: 1.0,
            lvl: 0.0,
            peak: 0.008,
            pose: "idle".to_string(),
            pose_since: 0.0,
            ts: 0.0,
            reduced: false,
            asking: false,
            choreo: Choreo::new(&BUTTONS, false),
            ask: Spring::new(0.0, 300.0, 26.0),
            mute_p: Spring::new(0.0, 420.0, 41.0),
            call: Spring::new(0.0, 300.0, 26.0),
            mic_space: Spring::new(1.0, 260.0, 32.0),
            custom: None,
            colored: false,
        }
    }

    /// Tiempo interno del fluido (no avanza con movimiento reducido).
    pub fn flow(&self) -> f64 {
        self.field.t
    }

    /// Una columna de la animación: estado, voz, tema y puntero.
    pub fn update(&mut self, input: &Update<'_>) {
        self.reduced = input.reduced;
        self.asking = input.asking;
        if input.pose != self.pose {
            self.pose = input.pose.to_string();
            self.pose_since = input.now;
        }
        self.ts = input.now - self.pose_since;
        if self.field.layout != self.layout {
            self.field = FlowState::new(self.layout);
        }
        self.custom = input.custom;
        self.update_air(input);

        let show = (input.hover || input.asking || input.menu_open || input.buttons == "always")
            && !self.form_hidden;
        let live = input.in_call || input.asking;
        let want = BTreeMap::from([
            ("gear".to_string(), show && !input.asking),
            ("chat".to_string(), show && !input.asking),
            ("mic".to_string(), show && live),
            ("hang".to_string(), show),
        ]);
        let order = emerge_order(self.layout, Some(self.dock));
        let groups: Vec<&[&str]> = order.iter().map(Vec::as_slice).collect();
        self.choreo
            .set_targets(&want, &groups, input.now, input.reduced);
        self.choreo.step(
            input.now,
            input.dt,
            input.reduced,
            input.hover && !input.asking,
            input.hover_item,
            input.pressed,
        );
        self.ask.t = if input.asking { 1.0 } else { 0.0 };
        self.mute_p.t = if input.muted { 1.0 } else { 0.0 };
        self.call.t = if live { 0.0 } else { 1.0 };
        self.mic_space.t = if live { 1.0 } else { 0.0 };
        for spring in [
            &mut self.ask,
            &mut self.mute_p,
            &mut self.call,
            &mut self.mic_space,
        ] {
            if input.reduced {
                spring.fade(input.dt, REDUCED_FADE);
            } else {
                spring.step(input.dt, false);
            }
        }
    }

    /// Nivel de voz con ganancia automática, brillo y paleta.
    fn update_air(&mut self, input: &Update<'_>) {
        self.update_level(input.dt, input.pose, input.rms, input.reduced);
        self.update_brightness(input.dt, input.pose, input.reduced);
        self.update_colors(input.dt, input.pose, input.theme, input.reduced);
    }

    fn update_level(&mut self, dt: f64, pose: &str, rms: f64, reduced: bool) {
        // Un micrófono flojo usa todo el rango.
        let r = rms.max(0.0);
        let target = if pose == "listen" || pose == "speak" {
            self.peak = 0.008_f64.max(self.peak * 0.9985).max(r);
            (r / self.peak).powf(0.8).min(1.0)
        } else {
            self.peak = 0.008_f64.max(self.peak * 0.9995);
            0.0
        };
        let tau = if target > self.lvl { 0.05 } else { 0.25 };
        self.lvl += (target - self.lvl) * (1.0 - (-dt / tau).exp());
        self.field.step(dt, self.lvl, pose, reduced);
    }

    fn update_brightness(&mut self, dt: f64, pose: &str, reduced: bool) {
        let mut target = match pose {
            "mute" => 0.9,
            "sleep" => 0.97,
            _ => 1.0,
        };
        if pose == "done" {
            target += (1.0 - self.ts * 2.2).max(0.0) * 0.2;
        }
        let k = if reduced {
            1.0
        } else {
            1.0 - (-dt / 0.15).exp()
        };
        self.bright += (target - self.bright) * k;
    }

    fn update_colors(&mut self, dt: f64, pose: &str, theme: &str, reduced: bool) {
        self.theme = if theme == "light" { "light" } else { "dark" };
        // El primer fotograma ya sale con el color correcto (sin fundido desde el claro).
        let k = if self.colored {
            1.0 - (-dt / if reduced { 0.12 } else { 0.25 }).exp()
        } else {
            1.0
        };
        self.colored = true;
        let target = match self.custom {
            Some(custom) if CUSTOM_POSES.contains(&pose) => custom,
            _ => colors_for(self.theme, pose),
        };
        let k = k as f32;
        for (row, target_row) in target.iter().enumerate() {
            for (channel, target_value) in target_row.iter().enumerate() {
                self.cols[row][channel] += (target_value - self.cols[row][channel]) * k;
            }
        }
    }

    /// ¿Hay botones, crecimiento o iconos a medio camino? (pide 60 fps).
    pub fn animating(&self, now: f64) -> bool {
        let springs = [&self.ask, &self.mute_p, &self.call, &self.mic_space];
        self.choreo.animating(now) || !springs.iter().all(|spring| spring.settled())
    }

    /// Para cada elemento: `(cx, cy, escala, opacidad)`, en coordenadas lógicas.
    ///
    /// Los botones viajan desde el centro de la forma hasta su hueco; la forma
    /// nunca se mueve (su «escala» es el leve crecimiento al acercar).
    pub fn positions(&self) -> BTreeMap<&'static str, (f64, f64, f64, f64)> {
        let sl = slots(self.layout, self.mic_space.x, Some(self.dock));
        let origin = sl["pill"];
        let mut out = BTreeMap::new();
        out.insert("pill", (origin.0, origin.1, self.choreo.grow.x, 1.0));
        for name in BUTTONS {
            let (cx, cy, scale, alpha) =
                emerge(sl[name], origin, self.choreo.appear[name].x, self.reduced);
            out.insert(name, (cx, cy, scale * self.choreo.pointer[name].x, alpha));
        }
        out
    }

    /// Botón visible bajo el punto, o `Some("pill")` si es la forma (para arrastrar).
    pub fn item_at(&self, x: f64, y: f64) -> Option<&'static str> {
        if self.form_hidden {
            // El chat ocupa su lugar: ni botones ni forma bajo el ratón.
            return None;
        }
        let pos = self.positions();
        for name in BUTTONS {
            let (cx, cy, scale, alpha) = pos[name];
            let (dx, dy) = (x - cx, y - cy);
            if scale > 0.6 && alpha > 0.5 && dx * dx + dy * dy <= (BTN / 2.0 * 1.05).powi(2) {
                return Some(name);
            }
        }
        let (cx, cy, _, _) = pos["pill"];
        let (dx, dy) = (x - cx, y - cy);
        if self.layout == "circle" {
            return if dx * dx + dy * dy <= (CIRCLE_D / 2.0).powi(2) {
                Some("pill")
            } else {
                None
            };
        }
        let r = PILL_H / 2.0;
        let dx = (dx.abs() - (PILL_W / 2.0 - r)).max(0.0);
        if dx * dx + dy * dy <= r * r {
            Some("pill")
        } else {
            None
        }
    }

    /// Rectángulo de forma y huecos de botones, con margen (para el «cerca»).
    ///
    /// Usa los huecos completos, no la posición animada: acercarse a donde
    /// estará un botón oculto también lo hace aparecer.
    pub fn bar_rect(&self) -> Rect {
        let (x0, y0, x1, y1) = self.hub_rect(1.0);
        (
            x0 - HOVER_MARGIN,
            y0 - HOVER_MARGIN,
            x1 + HOVER_MARGIN,
            y1 + HOVER_MARGIN,
        )
    }

    /// Rectángulo lógico de la forma y los huecos de sus botones (todo el hub).
    pub fn hub_rect(&self, mic: f64) -> Rect {
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for (name, (cx, cy)) in slots(self.layout, mic, Some(self.dock)) {
            if name == "mic" && mic < 0.01 {
                continue; // plegado: no ocupa sitio
            }
            let (half_w, half_h): Point = if name == "pill" {
                if self.layout == "pill" {
                    (PILL_W / 2.0, PILL_H / 2.0)
                } else {
                    (CIRCLE_D / 2.0, CIRCLE_D / 2.0)
                }
            } else {
                (BTN / 2.0, BTN / 2.0)
            };
            x0 = x0.min(cx - half_w);
            x1 = x1.max(cx + half_w);
            y0 = y0.min(cy - half_h);
            y1 = y1.max(cy + half_h);
        }
        (x0, y0, x1, y1)
    }

    pub fn near(&self, x: f64, y: f64) -> bool {
        if self.form_hidden {
            return false;
        }
        let (x0, y0, x1, y1) = self.bar_rect();
        x0 <= x && x <= x1 && y0 <= y && y <= y1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    fn run(model: &mut DropModel, frames: usize, overrides: Update<'_>) {
        for step in 0..frames {
            model.update(&Update {
                now: step as f64 / 60.0,
                dt: 1.0 / 60.0,
                ..overrides
            });
        }
    }

    #[test]
    fn buttons_emerge_and_are_clickable() {
        let mut model = DropModel::new("circle", None);
        run(
            &mut model,
            60,
            Update {
                theme: "dark",
                hover: true,
                ..Update::default()
            },
        );
        let pos = model.positions();
        close(pos["chat"].0, 74.000_471_995_178_63);
        close(pos["chat"].1, 97.998_777_103_400_81);
        close(pos["chat"].2, 0.999_991_418_269_479_3);
        close(pos["pill"].2, 1.040_000_103_910_948_3);
        assert_eq!(model.item_at(pos["chat"].0, pos["chat"].1), Some("chat"));
        assert_eq!(model.item_at(pos["pill"].0, pos["pill"].1), Some("pill"));
        assert_eq!(model.item_at(0.0, 0.0), None);
        assert!(!model.animating(59.0 / 60.0));
        assert_eq!(model.hub_rect(1.0), (12.0, 12.0, 180.0, 116.0));
        let (x0, _, x1, _) = model.hub_rect(1.0);
        assert!(x1 - x0 > crate::model::drop::CIRCLE_D + 3.0 * crate::model::drop::BTN);
        let folded = model.hub_rect(0.0);
        assert!(folded.2 - folded.0 < x1 - x0);
        assert_eq!(model.bar_rect(), (-10.0, -10.0, 202.0, 138.0));
        assert!(model.near(1.0, 1.0));
        assert!(!model.near(500.0, 500.0));
    }

    #[test]
    fn asking_hides_the_menu_and_chat_buttons() {
        let mut model = DropModel::new("pill", None);
        run(
            &mut model,
            90,
            Update {
                theme: "dark",
                hover: true,
                asking: true,
                ..Update::default()
            },
        );
        let pos = model.positions();
        assert!(pos["gear"].3 < 0.1 && pos["chat"].3 < 0.1);
        assert!(pos["mic"].3 > 0.9 && pos["hang"].3 > 0.9);
        assert!(model.ask.x > 0.99);
        assert_eq!(model.call.x, 0.0);
    }

    #[test]
    fn without_a_call_the_mic_folds_and_the_form_stays_put() {
        let mut model = DropModel::new("circle", None);
        run(
            &mut model,
            60,
            Update {
                pose: "listen",
                rms: 0.4,
                theme: "light",
                buttons: "always",
                in_call: false,
                ..Update::default()
            },
        );
        let pos = model.positions();
        assert_eq!(pos["pill"], (96.0, 41.0, 1.0, 1.0));
        assert_eq!(pos["mic"].3, 0.0);
        close(pos["mic"].0, 96.0);
        close(pos["mic"].2, 0.6);
        assert_eq!(model.hub_rect(0.0), (34.0, 12.0, 158.0, 116.0));
        assert!(model.mic_space.x < 0.01);
        assert!(model.lvl > 0.99);
    }

    #[test]
    fn the_hidden_form_keeps_the_pointer_out() {
        let mut model = DropModel::new("pill", None);
        model.update(&Update {
            hover: true,
            ..Update::default()
        });
        run(
            &mut model,
            30,
            Update {
                hover: true,
                ..Update::default()
            },
        );
        model.form_hidden = true;
        assert_eq!(model.item_at(154.0, 31.0), None);
        assert!(!model.near(154.0, 31.0));
    }

    #[test]
    fn the_flow_clock_and_the_palette_follow_the_python() {
        let mut model = DropModel::new("pill", None);
        for step in 0..60 {
            model.update(&Update {
                now: step as f64 / 60.0,
                dt: 1.0 / 60.0,
                theme: "dark",
                ..Update::default()
            });
        }
        close(model.flow(), 0.549_999_999_999_999_6);
        assert_eq!(
            model.cols[0],
            [5.0, 38.0, 46.0],
            "el primer fotograma ya sale oscuro"
        );
        assert_eq!(model.bright, 1.0);
    }

    #[test]
    fn a_custom_palette_only_applies_to_its_poses() {
        let custom = [[1.0, 2.0, 3.0]; 5];
        let mut model = DropModel::new("pill", None);
        model.update(&Update {
            pose: "idle",
            custom: Some(custom),
            ..Update::default()
        });
        assert_eq!(model.cols, custom);
        // «notice» no está en `CUSTOM_POSES`: vuelve a la paleta del tema.
        model.update(&Update {
            pose: "error",
            custom: Some(custom),
            dt: 100.0,
            now: 1.0,
            ..Update::default()
        });
        assert_eq!(model.cols[0], [185.0, 38.0, 53.0]);
    }

    #[test]
    fn changing_the_layout_rebuilds_the_flow() {
        let mut model = DropModel::new("pill", None);
        model.field.t = 12.0;
        model.layout = "circle";
        model.update(&Update::default());
        assert_eq!(model.field.layout, "circle");
        assert_eq!(model.field.t, 0.0);
    }
}
