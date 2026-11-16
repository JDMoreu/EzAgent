//! Port of `src/ezagent/voice/hud/bar_motion.py` (F2-12).
//!
//! Coreografía de la barra: cómo nacen los botones de la forma y vuelven a ella.
//! Módulo puro (sin Win32 ni Pillow) para poder probar los tiempos aislados.
//!
//! * Entrada: la forma crece un 4 % y los botones salen de *detrás* de ella,
//!   escalados a 0.6 y transparentes, con un muelle de ~8 % de rebote y 40 ms
//!   de escalonado desde la ranura más cercana a la más lejana.
//! * Salida: orden inverso, 25 ms de escalonado, críticamente amortiguada.
//! * Puntero: el botón bajo el puntero crece a 1.08; pulsarlo, a 0.94.
//! * Movimiento reducido: sin deslizamiento, sin escala ni rebote; fundidos
//!   lineales de 120 ms.

use std::collections::BTreeMap;

/// Segundos entre botones cuando salen.
pub const STAGGER_IN: f64 = 0.040;
/// Segundos entre botones cuando vuelven.
pub const STAGGER_OUT: f64 = 0.025;
/// k, c -> amortiguación 0.64, ~8 % de rebote.
pub const SPRING_IN: (f64, f64) = (300.0, 22.0);
/// k, c -> críticamente amortiguado, sin rebote.
pub const SPRING_OUT: (f64, f64) = (420.0, 41.0);
pub const SPRING_POINTER: (f64, f64) = (520.0, 34.0);
pub const SPRING_GROW: (f64, f64) = (260.0, 26.0);
/// Escala de la forma mientras el puntero está cerca.
pub const FORM_GROW: f64 = 1.04;
pub const HOVER_SCALE: f64 = 1.08;
pub const PRESS_SCALE: f64 = 0.94;
/// Escala de un botón cuando empieza a salir.
pub const BORN_SCALE: f64 = 0.6;
/// Segundos: fundido lineal con movimiento reducido.
pub const REDUCED_FADE: f64 = 0.12;
pub const SETTLE_EPS: f64 = 1e-3;

/// Muelle amortiguado (k, c, masa unitaria) resuelto para el tiempo transcurrido.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
    pub t: f64,
    pub k: f64,
    pub c: f64,
}

impl Spring {
    pub fn new(x: f64, k: f64, c: f64) -> Self {
        Self {
            x,
            v: 0.0,
            t: x,
            k,
            c,
        }
    }

    pub fn step(&mut self, dt: f64, instant: bool) -> f64 {
        if instant {
            self.x = self.t;
            self.v = 0.0;
            return self.x;
        }
        if dt <= 0.0 {
            return self.x;
        }
        // DECISION: solve the spring analytically to keep its trajectory the
        // same at idle/active frame rates without per-frame integration loops.
        let (mut offset, mut velocity) = (self.x - self.t, self.v);
        let damping = self.c / 2.0;
        let discriminant = damping * damping - self.k;
        if discriminant.abs() < 1e-8 {
            let decay = (-damping * dt).exp();
            let b = velocity + damping * offset;
            offset = (offset + b * dt) * decay;
            velocity = (velocity - damping * b * dt) * decay;
        } else if discriminant < 0.0 {
            let frequency = (-discriminant).sqrt();
            let angle = frequency * dt;
            let (cosine, sine) = (angle.cos(), angle.sin());
            let b = (velocity + damping * offset) / frequency;
            let decay = (-damping * dt).exp();
            let next_offset = (offset * cosine + b * sine) * decay;
            let next_velocity =
                (velocity * cosine - (damping * b + frequency * offset) * sine) * decay;
            offset = next_offset;
            velocity = next_velocity;
        } else {
            let root = discriminant.sqrt();
            let (r1, r2) = (-damping + root, -damping - root);
            let a = (velocity - r2 * offset) / (r1 - r2);
            let b = offset - a;
            let (e1, e2) = ((r1 * dt).exp(), (r2 * dt).exp());
            offset = a * e1 + b * e2;
            velocity = r1 * a * e1 + r2 * b * e2;
        }
        self.x = self.t + offset;
        self.v = velocity;
        self.x
    }

    /// Aproximación lineal al objetivo (movimiento reducido).
    pub fn fade(&mut self, dt: f64, seconds: f64) -> f64 {
        let step = dt / seconds.max(1e-6);
        let difference = self.t - self.x;
        self.x = if difference.abs() <= step {
            self.t
        } else {
            self.x + step.copysign(difference)
        };
        self.v = 0.0;
        self.x
    }

    pub fn settled(&self) -> bool {
        (self.x - self.t).abs() < SETTLE_EPS && self.v.abs() < SETTLE_EPS * 10.0
    }
}

/// Muelles de aparición por botón, con inicio escalonado.
#[derive(Clone, Debug)]
pub struct Choreo {
    pub names: Vec<String>,
    pub appear: BTreeMap<String, Spring>,
    pub pointer: BTreeMap<String, Spring>,
    pub grow: Spring,
    want: BTreeMap<String, bool>,
    due: BTreeMap<String, f64>,
}

impl Choreo {
    pub fn new(names: &[&str], shown: bool) -> Self {
        let value = if shown { 1.0 } else { 0.0 };
        let (spring_k, spring_c) = SPRING_IN;
        let mut appear = BTreeMap::new();
        let mut pointer = BTreeMap::new();
        let mut want = BTreeMap::new();
        let mut due = BTreeMap::new();
        for name in names {
            appear.insert((*name).to_string(), Spring::new(value, spring_k, spring_c));
            pointer.insert(
                (*name).to_string(),
                Spring::new(1.0, SPRING_POINTER.0, SPRING_POINTER.1),
            );
            want.insert((*name).to_string(), shown);
            due.insert((*name).to_string(), 0.0);
        }
        Self {
            names: names.iter().map(|name| (*name).to_string()).collect(),
            appear,
            pointer,
            grow: Spring::new(1.0, SPRING_GROW.0, SPRING_GROW.1),
            want,
            due,
        }
    }

    fn wanted(&self, name: &str) -> bool {
        self.want.get(name).copied().unwrap_or(false)
    }

    /// Programa cada cambio.
    ///
    /// `order` es una secuencia de grupos de la ranura más cercana a la más
    /// lejana; los botones del mismo grupo se mueven juntos. Movimiento
    /// reducido: sin escalonado.
    pub fn set_targets(
        &mut self,
        want: &BTreeMap<String, bool>,
        order: &[&[&str]],
        now: f64,
        reduced: bool,
    ) {
        let groups: Vec<Vec<&str>> = order
            .iter()
            .map(|group| {
                group
                    .iter()
                    .copied()
                    .filter(|name| self.appear.contains_key(*name))
                    .collect()
            })
            .collect();
        let coming: Vec<&Vec<&str>> = groups
            .iter()
            .filter(|group| {
                group
                    .iter()
                    .any(|name| want.get(*name).copied().unwrap_or(false) && !self.wanted(name))
            })
            .collect();
        let going: Vec<&Vec<&str>> = groups
            .iter()
            .rev()
            .filter(|group| {
                group
                    .iter()
                    .any(|name| !want.get(*name).copied().unwrap_or(false) && self.wanted(name))
            })
            .collect();
        for (index, group) in coming.iter().enumerate() {
            for name in group.iter() {
                let target = want.get(*name).copied().unwrap_or(false);
                if target && !self.wanted(name) {
                    self.want.insert((*name).to_string(), true);
                    let delay = if reduced {
                        0.0
                    } else {
                        index as f64 * STAGGER_IN
                    };
                    self.due.insert((*name).to_string(), now + delay);
                }
            }
        }
        for (index, group) in going.iter().enumerate() {
            for name in group.iter() {
                let target = want.get(*name).copied().unwrap_or(false);
                if !target && self.wanted(name) {
                    self.want.insert((*name).to_string(), false);
                    let delay = if reduced {
                        0.0
                    } else {
                        index as f64 * STAGGER_OUT
                    };
                    self.due.insert((*name).to_string(), now + delay);
                }
            }
        }
    }

    pub fn step(
        &mut self,
        now: f64,
        dt: f64,
        reduced: bool,
        grow: bool,
        hover_item: Option<&str>,
        pressed: Option<&str>,
    ) {
        for (name, spring) in self.appear.iter_mut() {
            if now >= self.due[name.as_str()] {
                let target = if self.want[name.as_str()] { 1.0 } else { 0.0 };
                if target != spring.t {
                    spring.t = target;
                    let (k, c) = if target > spring.x {
                        SPRING_IN
                    } else {
                        SPRING_OUT
                    };
                    spring.k = k;
                    spring.c = c;
                }
            }
            if reduced {
                spring.fade(dt, REDUCED_FADE);
            } else {
                spring.step(dt, false);
            }
        }
        for (name, spring) in self.pointer.iter_mut() {
            spring.t = if reduced {
                1.0
            } else if pressed == Some(name.as_str()) {
                PRESS_SCALE
            } else if hover_item == Some(name.as_str()) {
                HOVER_SCALE
            } else {
                1.0
            };
            spring.step(dt, reduced);
        }
        self.grow.t = if grow && !reduced { FORM_GROW } else { 1.0 };
        self.grow.step(dt, reduced);
    }

    /// True mientras un cambio escalonado aún no ha empezado.
    pub fn pending(&self, now: f64) -> bool {
        self.names.iter().any(|name| {
            now < self.due[name.as_str()]
                && (if self.want[name.as_str()] { 1.0 } else { 0.0 })
                    != self.appear[name.as_str()].t
        })
    }

    pub fn animating(&self, now: f64) -> bool {
        self.pending(now)
            || !self
                .appear
                .values()
                .chain(self.pointer.values())
                .chain(std::iter::once(&self.grow))
                .all(Spring::settled)
    }
}

/// `(cx, cy, scale, alpha)` de un botón cuyo valor de aparición es `a`.
///
/// El botón viaja de `origin` (el centro de la forma) a su `slot`. `a` puede
/// pasarse un poco de 1: el botón va un poco más allá de su ranura y vuelve,
/// que es el rebote.
pub fn emerge(slot: (f64, f64), origin: (f64, f64), a: f64, reduced: bool) -> (f64, f64, f64, f64) {
    if reduced {
        return (slot.0, slot.1, 1.0, a.clamp(0.0, 1.0));
    }
    let k = a.max(0.0);
    let cx = origin.0 + (slot.0 - origin.0) * k;
    let cy = origin.1 + (slot.1 - origin.1) * k;
    let scale = BORN_SCALE + (1.0 - BORN_SCALE) * k.min(1.0);
    let alpha = (k * 1.6).clamp(0.0, 1.0);
    (cx, cy, scale, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    fn spring_track(k: f64, c: f64) -> Vec<f64> {
        let mut spring = Spring::new(0.0, k, c);
        spring.t = 1.0;
        let mut previous = 0.0;
        [0.0, 0.016, 0.033, 0.050, 0.100, 0.200, 0.400, 1.200]
            .iter()
            .map(|seconds| {
                let value = spring.step(seconds - previous, false);
                previous = *seconds;
                value
            })
            .collect()
    }

    #[test]
    fn springs_trace_the_reference_trajectories() {
        let inside = spring_track(SPRING_IN.0, SPRING_IN.1);
        close(inside[1], 0.034_051_071_131_189_77);
        close(inside[5], 1.058_085_308_138_592_1);
        let outside = spring_track(SPRING_OUT.0, SPRING_OUT.1);
        close(outside[4], 0.607_087_617_090_771_7);
        close(outside[7], 0.999_999_999_434_631_5);
        let pointer = spring_track(SPRING_POINTER.0, SPRING_POINTER.1);
        close(pointer[3], 0.360_844_160_446_033_2);
    }

    #[test]
    fn spring_settles_and_instant_jumps() {
        let mut spring = Spring::new(0.0, SPRING_IN.0, SPRING_IN.1);
        spring.t = 1.0;
        for _ in 0..240 {
            spring.step(1.0 / 60.0, false);
        }
        assert!(spring.settled());
        assert_eq!(spring.step(1.0, true), 1.0);
        assert_eq!(spring.v, 0.0);
        assert_eq!(spring.step(0.0, false), 1.0);
    }

    #[test]
    fn the_trajectory_does_not_depend_on_the_frame_rate() {
        for damping in [22.0, 40.0, 55.0] {
            let mut reference = Spring::new(0.0, 400.0, damping);
            reference.t = 1.0;
            reference.step(0.5, false);
            for fps in [30, 60, 144] {
                let mut sampled = Spring::new(0.0, 400.0, damping);
                sampled.t = 1.0;
                for _ in 0..fps / 2 {
                    sampled.step(1.0 / f64::from(fps), false);
                }
                assert!(
                    (sampled.x - reference.x).abs() < 1e-10,
                    "posición {fps} fps, c={damping}"
                );
                assert!(
                    (sampled.v - reference.v).abs() < 1e-10,
                    "velocidad {fps} fps, c={damping}"
                );
            }
        }
    }

    #[test]
    fn reversing_keeps_position_and_velocity() {
        let mut spring = Spring::new(0.0, SPRING_IN.0, SPRING_IN.1);
        spring.t = 1.0;
        spring.step(0.08, false);
        let (position, velocity) = (spring.x, spring.v);
        spring.t = 0.0;
        assert_eq!(spring.step(0.0, false), position);
        assert_eq!(spring.v, velocity);
        spring.step(0.001, false);
        assert!((spring.x - position).abs() < 0.01);
        spring.step(10.0, false);
        assert!(spring.settled());
        assert!(spring.x.abs() < 1e-8);
    }

    #[test]
    fn fade_is_linear_and_stops_on_target() {
        let mut spring = Spring::new(0.0, SPRING_IN.0, SPRING_IN.1);
        spring.t = 1.0;
        let values: Vec<f64> = [0.0, 0.016, 0.033, 0.050, 0.100, 0.120]
            .iter()
            .map(|dt| spring.fade(*dt, REDUCED_FADE))
            .collect();
        close(values[1], 0.133_333_333_333_333_33);
        close(values[2], 0.408_333_333_333_333_3);
        assert_eq!(values[4], 1.0);
        assert_eq!(values[5], 1.0);
        assert!(spring.settled());
    }

    #[test]
    fn buttons_emerge_from_the_form_centre() {
        let slot = (220.0, 31.0);
        let origin = (154.0, 31.0);
        let (cx, cy, scale, alpha) = emerge(slot, origin, 0.25, false);
        close(cx, 170.5);
        assert_eq!(cy, 31.0);
        close(scale, 0.7);
        close(alpha, 0.4);
        let (cx, _, scale, alpha) = emerge(slot, origin, 1.04, false);
        close(cx, 222.64);
        assert_eq!(scale, 1.0);
        assert_eq!(alpha, 1.0);
        assert_eq!(emerge(slot, origin, 0.4, true), (220.0, 31.0, 1.0, 0.4));
    }

    #[test]
    fn choreo_staggers_the_appearance_in_order() {
        let names = ["gear", "chat", "mic", "hang"];
        let order: [&[&str]; 2] = [&["chat", "mic"], &["gear", "hang"]];
        let mut choreo = Choreo::new(&names, false);
        let wants: BTreeMap<String, bool> = names
            .iter()
            .map(|name| ((*name).to_string(), true))
            .collect();
        choreo.set_targets(&wants, &order, 0.0, false);
        assert!(choreo.pending(0.0));
        let expected = [
            (0.0, 0.0, 0.0, 1.0),
            (0.016, 0.034_051_071_131, 0.0, 1.001_158_052_231),
            (0.033, 0.126_740_905_356, 0.0, 1.004_243_303_059),
            (
                0.050,
                0.253_220_498_739,
                0.038_145_778_067,
                1.008_384_422_833,
            ),
            (
                0.100,
                0.656_886_090_110,
                0.393_863_820_247,
                1.021_575_903_941,
            ),
        ];
        let mut previous = 0.0;
        for (seconds, chat, gear, grow) in expected {
            choreo.step(seconds, seconds - previous, false, true, None, None);
            previous = seconds;
            close(choreo.appear["chat"].x, chat);
            close(choreo.appear["gear"].x, gear);
            close(choreo.grow.x, grow);
        }
        assert!(!choreo.pending(0.1));
        assert!(choreo.animating(0.1));
        choreo.step(2.0, 2.0 - previous, false, true, None, None);
        assert!(!choreo.animating(2.0));
    }

    #[test]
    fn reduced_motion_only_fades() {
        let names = ["gear", "chat"];
        let order: [&[&str]; 1] = [&["gear", "chat"]];
        let mut choreo = Choreo::new(&names, false);
        let wants: BTreeMap<String, bool> = names
            .iter()
            .map(|name| ((*name).to_string(), true))
            .collect();
        choreo.set_targets(&wants, &order, 0.0, true);
        choreo.step(0.0, 0.0, true, true, Some("gear"), None);
        assert_eq!(choreo.pointer["gear"].x, 1.0);
        assert_eq!(choreo.grow.x, 1.0);
        choreo.step(0.060, 0.060, true, true, Some("gear"), Some("gear"));
        close(choreo.appear["gear"].x, 0.5);
        choreo.set_targets(&BTreeMap::new(), &order, 0.060, true);
        assert!(!choreo.pending(0.060));
    }
}
