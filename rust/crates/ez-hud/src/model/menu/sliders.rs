//! Port of `src/ezagent/voice/hud/menu.py 1888-1960` (F5-09).
//!
//! Arrastre y emisión de los sliders y del medidor de entrada. Es modelo puro:
//! la posición del puntero se convierte en valor y se avisa a la aplicación.

use crate::model::settings::{db_to_rms, SettingValue, METER_DB_CEIL, METER_DB_FLOOR, SLIDERS};

use super::geometry::Menu;
use super::rects::{SLIDER_TRACK_X0, SLIDER_TRACK_X1};

/// Cadencia mínima de emisión mientras se arrastra un slider.
pub const SLIDER_EMIT_MS: f64 = 50.0;

/// `float(value)` de Python para un valor de slider.
///
/// // DECISION: un valor que `float` no aceptaría (None o texto) vale 0 en vez
/// // de lanzar; las claves de slider siempre guardan `Float` normalizado.
pub(super) fn as_float(value: &SettingValue) -> f64 {
    match value {
        SettingValue::Float(number) => *number,
        SettingValue::Int(number) => *number as f64,
        SettingValue::Bool(flag) => f64::from(u8::from(*flag)),
        SettingValue::None | SettingValue::Text(_) => 0.0,
    }
}

impl Menu {
    /// `_set_slider_from_x`: posición del puntero en el rango del slider.
    pub fn set_slider_from_x(&mut self, key: &str, x: f64, now: f64) {
        // // DECISION: una clave sin especificación no mueve nada en vez de
        // // lanzar el `KeyError`; solo se llega desde un hit de slider.
        let Some(spec) = SLIDERS.get(key) else {
            return;
        };
        let width = (SLIDER_TRACK_X1 + self.dx) - SLIDER_TRACK_X0;
        let fraction = if width <= 0.0 {
            0.0
        } else {
            (x - SLIDER_TRACK_X0) / width
        };
        let fraction = fraction.clamp(0.0, 1.0);
        let value = spec.minimum + fraction * (spec.maximum - spec.minimum);
        let old = self.settings.get(key).cloned();
        let Ok(new) = self.settings.set(key, &SettingValue::Float(value)) else {
            return;
        };
        if old.as_ref() == Some(&new) {
            return;
        }
        self.slider_changed = true;
        self.slider_pending = Some((key.to_string(), new));
        self.emit_slider(now, false);
    }

    /// `_set_meter_from_x`: posición del puntero en el umbral de voz.
    pub fn set_meter_from_x(&mut self, x: f64, now: f64) {
        let width = (SLIDER_TRACK_X1 + self.dx) - SLIDER_TRACK_X0;
        let fraction = if width <= 0.0 {
            0.0
        } else {
            (x - SLIDER_TRACK_X0) / width
        };
        let fraction = fraction.clamp(0.0, 1.0);
        // La posición se convierte en dB, la misma escala con la que se dibuja
        // el medidor, así el pulgar acaba justo bajo el puntero.
        let value = db_to_rms(METER_DB_FLOOR + fraction * (METER_DB_CEIL - METER_DB_FLOOR));
        let old = self.settings.get("min_rms").cloned();
        let Ok(new) = self.settings.set("min_rms", &SettingValue::Float(value)) else {
            return;
        };
        if old.as_ref() == Some(&new) {
            return;
        }
        self.meter_changed = true;
        self.meter_pending = Some(("min_rms".to_string(), new));
        if now - self.meter_last_emit >= SLIDER_EMIT_MS / 1000.0 {
            self.emit_meter(now);
        }
    }

    /// `_emit_meter`: avisa del último umbral pendiente.
    pub fn emit_meter(&mut self, now: f64) {
        let Some((key, value)) = self.meter_pending.take() else {
            return;
        };
        self.meter_last_emit = now;
        self.notify_live_change(&key, &value);
    }

    /// `_emit_slider`: avisa del último valor pendiente, con límite de cadencia.
    pub fn emit_slider(&mut self, now: f64, force: bool) {
        if self.slider_pending.is_none() {
            return;
        }
        if !force && now - self.slider_last_emit < SLIDER_EMIT_MS / 1000.0 {
            return;
        }
        let Some((key, value)) = self.slider_pending.take() else {
            return;
        };
        self.slider_last_emit = now;
        self.notify_live_change(&key, &value);
    }
}

#[cfg(test)]
mod tests;
