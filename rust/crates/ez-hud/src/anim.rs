//! Port of `src/ezagent/voice/hud/anim.py` (F2-12).
//!
//! La matemática de las curvas, `Tween`, `LevelFollower` y `lerp_rgb` está
//! copiada de la especificación (12.1, 12.2 y 12.7) sin cambiar su
//! comportamiento. Los puntos de control viven en `tokens` para que no haya
//! valores visuales duplicados.
//!
//! Todas las duraciones se expresan en milisegundos y los relojes en segundos.

use crate::tokens::{
    BEZIER_IN, BEZIER_INOUT, BEZIER_OUT, BEZIER_SPRING, VOICE_ATTACK, VOICE_RELEASE,
};

/// Curva de aceleración `ease(x) -> y`, igual que CSS
/// `cubic-bezier(x1, y1, x2, y2)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Easing {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Easing {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// `ease(x)`.
    pub fn apply(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let cx = 3.0 * self.x1;
        let bx = 3.0 * (self.x2 - self.x1) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * self.y1;
        let by = 3.0 * (self.y2 - self.y1) - cy;
        let ay = 1.0 - cy - by;
        let sample_x = |t: f64| ((ax * t + bx) * t + cx) * t;
        let sample_y = |t: f64| ((ay * t + by) * t + cy) * t;
        let mut lo = 0.0;
        let mut hi = 1.0;
        for _ in 0..22 {
            let t = (lo + hi) / 2.0;
            if sample_x(t) < x {
                lo = t;
            } else {
                hi = t;
            }
        }
        sample_y((lo + hi) / 2.0)
    }
}

/// Devuelve `ease(x) -> y`, igual que CSS `cubic-bezier(x1, y1, x2, y2)`.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Easing {
    Easing::new(x1, y1, x2, y2)
}

pub const EASE_OUT: Easing = Easing::new(BEZIER_OUT.0, BEZIER_OUT.1, BEZIER_OUT.2, BEZIER_OUT.3);
pub const EASE_IN: Easing = Easing::new(BEZIER_IN.0, BEZIER_IN.1, BEZIER_IN.2, BEZIER_IN.3);
pub const EASE_INOUT: Easing = Easing::new(
    BEZIER_INOUT.0,
    BEZIER_INOUT.1,
    BEZIER_INOUT.2,
    BEZIER_INOUT.3,
);
pub const EASE_SPRING: Easing = Easing::new(
    BEZIER_SPRING.0,
    BEZIER_SPRING.1,
    BEZIER_SPRING.2,
    BEZIER_SPRING.3,
);

/// Valor animado que siempre parte del valor actual.
///
/// Reglas:
/// - `to()` solo se llama cuando el objetivo cambia, no en cada fotograma.
/// - Se puede invertir a mitad de una animación sin saltos.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub value: f64,
    v0: f64,
    v1: f64,
    t0: f64,
    dur: f64,
    ease: Option<Easing>,
}

impl Tween {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            v0: value,
            v1: value,
            t0: 0.0,
            dur: 0.0,
            ease: None,
        }
    }

    pub fn active(&self) -> bool {
        self.ease.is_some()
    }

    pub fn to(&mut self, target: f64, now_s: f64, dur_ms: f64, ease: Easing) {
        self.v0 = self.value;
        self.v1 = target;
        self.t0 = now_s;
        self.dur = dur_ms / 1000.0;
        self.ease = Some(ease);
        if self.dur <= 0.0 {
            self.value = target;
            self.ease = None;
        }
    }

    pub fn update(&mut self, now_s: f64) -> f64 {
        let Some(ease) = self.ease else {
            return self.value;
        };
        let k = (now_s - self.t0) / self.dur;
        if k >= 1.0 {
            self.value = self.v1;
            self.ease = None;
        } else {
            self.value = self.v0 + (self.v1 - self.v0) * ease.apply(k.max(0.0));
        }
        self.value
    }
}

impl Default for Tween {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Nivel de voz suavizado: sube rápido (ataque), baja suave (caída).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelFollower {
    pub attack: f64,
    pub release: f64,
    pub value: f64,
}

impl LevelFollower {
    pub fn new(attack_ms: f64, release_ms: f64) -> Self {
        Self {
            attack: attack_ms,
            release: release_ms,
            value: 0.0,
        }
    }

    pub fn step(&mut self, target: f64, dt_ms: f64) -> f64 {
        let tau = if target > self.value {
            self.attack
        } else {
            self.release
        };
        self.value += (target - self.value) * (1.0 - (-dt_ms / tau).exp());
        self.value
    }
}

impl Default for LevelFollower {
    fn default() -> Self {
        Self::new(VOICE_ATTACK, VOICE_RELEASE)
    }
}

/// Canales sin recortar, como los devuelve el Python (`int` sin límites).
///
/// El dibujo usa [`lerp_rgb`], que cabe en `u8`; el oráculo comprueba también
/// `k` fuera de 0..1, donde el Python da valores fuera de rango.
pub fn lerp_rgb_raw(a: [u8; 3], b: [u8; 3], k: f64) -> [i64; 3] {
    let channel = |index: usize| {
        let (from, to) = (f64::from(a[index]), f64::from(b[index]));
        (from + (to - from) * k).round_ties_even() as i64
    };
    [channel(0), channel(1), channel(2)]
}

/// Interpola dos colores RGB y redondea cada canal.
pub fn lerp_rgb(a: [u8; 3], b: [u8; 3], k: f64) -> [u8; 3] {
    // DECISION: la firma devuelve `u8`; con `k` fuera de 0..1 el Python daría
    // enteros fuera de rango que el dibujo no usa, así que se recortan. El
    // oráculo usa `lerp_rgb_raw`, que reproduce esos enteros.
    let raw = lerp_rgb_raw(a, b, k);
    [
        raw[0].clamp(0, 255) as u8,
        raw[1].clamp(0, 255) as u8,
        raw[2].clamp(0, 255) as u8,
    ]
}

/// Movimiento del sistema permitido (`SPI_GETCLIENTAREAANIMATION`).
///
/// Si la consulta falla o no estamos en Windows se asume `True`.
///
/// DECISION: la consulta Win32 exige código nativo no seguro y
/// `AGENTS.rust.md` §5 solo lo admite dentro de `ez-hud::win` (de otra tarea),
/// así que aquí se devuelve `true`, que es lo mismo que hace el Python cuando
/// la llamada falla; el ajuste de movimiento reducido del usuario llega por
/// configuración.
pub fn animations_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn curves_match_css_cubic_bezier() {
        close(EASE_OUT.apply(0.5), 0.946_079_480_054_970_6);
        close(EASE_IN.apply(0.25), 0.035_027_621_649_290_304);
        close(EASE_INOUT.apply(0.5), 0.775_561_167_277_037_5);
        close(EASE_SPRING.apply(0.5), 1.087_400_701_915_585_4);
        assert_eq!(EASE_OUT.apply(0.0), 0.0);
        assert_eq!(EASE_OUT.apply(1.0), 1.0);
        assert_eq!(cubic_bezier(0.2, 0.8, 0.2, 1.0), EASE_OUT);
    }

    #[test]
    fn tween_follows_its_curve_and_reverses_without_a_jump() {
        let mut tween = Tween::new(0.0);
        tween.to(1.0, 0.0, 240.0, EASE_OUT);
        close(tween.update(0.016), 0.269_042_295_355_649_33);
        close(tween.update(0.100), 0.912_195_042_878_477_2);
        let before = tween.update(0.200);
        tween.to(0.0, 0.2, 240.0, EASE_OUT);
        assert_eq!(tween.update(0.200), before);
        close(tween.update(0.216), 0.728_376_138_295_944_2);
        assert_eq!(tween.update(0.440), 0.0);
        assert!(!tween.active());
        let mut instant = Tween::new(0.5);
        instant.to(2.0, 0.0, 0.0, EASE_IN);
        assert_eq!(instant.value, 2.0);
        assert!(!instant.active());
    }

    #[test]
    fn level_follower_attacks_faster_than_it_releases() {
        let mut follower = LevelFollower::default();
        for _ in 0..4 {
            assert_eq!(follower.step(0.0, 16.0), 0.0);
        }
        close(follower.step(0.8, 16.0), 0.239_372_791_525_973_77);
        close(follower.step(0.8, 16.0), 0.407_121_416_398_275_9);
        for _ in 0..6 {
            follower.step(0.8, 16.0);
        }
        close(follower.step(0.2, 16.0), 0.710_914_196_701_116_3);
    }

    #[test]
    fn lerp_rgb_rounds_half_to_even() {
        assert_eq!(
            lerp_rgb([12, 112, 117], [94, 226, 196], 0.0),
            [12, 112, 117]
        );
        assert_eq!(
            lerp_rgb([12, 112, 117], [94, 226, 196], 0.25),
            [32, 140, 137]
        );
        assert_eq!(
            lerp_rgb([12, 112, 117], [94, 226, 196], 0.5),
            [53, 169, 156]
        );
        assert_eq!(
            lerp_rgb([12, 112, 117], [94, 226, 196], 1.0),
            [94, 226, 196]
        );
        assert_eq!(lerp_rgb([0, 0, 0], [255, 255, 255], 0.5), [128, 128, 128]);
        assert!(animations_enabled());
    }
}
