//! Reloj del fluido de la barra: lo único que `DropModel` necesita de `flow.py`.
//!
//! El dibujo del fluido (ruido, paleta y `render`) es de F5-04. Aquí solo vive
//! la simulación que avanza `DropModel.update`: tiempo, presión de la voz,
//! oleada, subida y giro.

use super::geometry::{PILL_H, PILL_W};

/// Velocidad a la que se pliega el fluido en reposo.
pub const FLOW_SPEED: f64 = 0.55;
/// Segundos: la presión de la voz sube rápido...
pub const ATTACK: f64 = 0.07;
/// ...y se apaga despacio, para que el fluido siga un momento.
pub const RELEASE: f64 = 0.7;
/// Unidades por segundo que sube el fluido mientras hablas.
pub const RISE: f64 = 0.9;
/// rad/s del remolino mientras piensa.
pub const SPIN: f64 = 0.55;
/// rad/s de los pulsos de voz de EzAgent.
pub const WAVE_SPEED: f64 = 5.0;

/// Estado de `flow.FlowField` sin la parte de pintura.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowState {
    pub layout: &'static str,
    pub aspect: f64,
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

impl FlowState {
    /// `FlowField(layout, aspect=1.0 if circle else PILL_W / PILL_H)`.
    pub fn new(layout: &'static str) -> Self {
        Self {
            layout,
            aspect: if layout == "circle" {
                1.0
            } else {
                PILL_W / PILL_H
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn the_pill_keeps_its_real_proportion() {
        close(FlowState::new("pill").aspect, PILL_W / PILL_H);
        close(FlowState::new("circle").aspect, 1.0);
    }

    #[test]
    fn idle_advances_the_clock_and_reduced_motion_freezes_it() {
        let mut flow = FlowState::new("circle");
        for _ in 0..60 {
            flow.step(1.0 / 60.0, 0.0, "idle", false);
        }
        close(flow.t, 0.55);
        let frozen = flow.clone();
        flow.step(1.0, 1.0, "listen", true);
        assert_eq!(flow, frozen);
        flow.step(0.0, 1.0, "listen", false);
        assert_eq!(flow, frozen);
    }

    #[test]
    fn the_voice_raises_the_pressure_and_the_clock() {
        let mut flow = FlowState::new("pill");
        for _ in 0..30 {
            flow.step(1.0 / 60.0, 1.0, "listen", false);
        }
        assert!(flow.pressure > 0.99);
        assert!(flow.lift > 0.3);
        assert!(flow.t > 0.6);
        assert_eq!(flow.mode, "listen");
    }

    #[test]
    fn thinking_spins_and_the_frame_step_is_capped() {
        let mut flow = FlowState::new("circle");
        flow.step(1.0, 0.0, "think", false);
        close(flow.t, 0.05 * FLOW_SPEED * 1.5);
        assert!(flow.spin_rate > 0.0 && flow.angle > 0.0);
        assert_eq!(flow.mode, "think");
    }
}
