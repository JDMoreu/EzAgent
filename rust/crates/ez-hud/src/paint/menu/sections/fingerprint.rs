//! `_layer_signature_value` de `menu.py 2404-2459` y el redondeo de Python.

use crate::anim::Tween;
use crate::model::menu::geometry::Menu;
use crate::model::settings::SettingValue;
use crate::tokens::palette_version;

use super::content_tweens;

/// FNV-1a con el redondeo de `round(x, digits)` del Python.
///
/// // DECISION: la huella de Python es una tupla de valores; aquí se resume en
/// // un `u64` (el campo que F5-07 dejó para F5-10). El redondeo se hace con
/// // `round_ties_even` sobre la escala decimal, que solo puede diferir de
/// // `round(x, n)` en el último dígito; nunca cambia los píxeles, solo si se
/// // reutiliza una capa.
struct Fingerprint(u64);

impl Fingerprint {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write_u64(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u64(u64::from(value));
    }

    fn write_rounded(&mut self, value: f64, digits: u32) {
        self.write_i64(round_scaled(value, digits));
    }

    fn write_str(&mut self, text: &str) {
        self.write_u64(text.len() as u64);
        for byte in text.bytes() {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_opt_str(&mut self, value: Option<&str>) {
        match value {
            None => self.write_u64(0),
            Some(text) => {
                self.write_u64(1);
                self.write_str(text);
            }
        }
    }

    fn write_opt_index(&mut self, value: Option<usize>) {
        match value {
            None => self.write_u64(0),
            Some(index) => {
                self.write_u64(1);
                self.write_u64(index as u64);
            }
        }
    }

    fn write_setting(&mut self, value: &SettingValue) {
        match value {
            SettingValue::None => self.write_u64(0),
            SettingValue::Bool(flag) => {
                self.write_u64(1);
                self.write_bool(*flag);
            }
            SettingValue::Int(number) => {
                self.write_u64(2);
                self.write_i64(*number);
            }
            SettingValue::Float(number) => {
                self.write_u64(3);
                self.write_rounded(*number, 6);
            }
            SettingValue::Text(text) => {
                self.write_u64(4);
                self.write_str(text);
            }
        }
    }

    fn write_opt_setting(&mut self, value: Option<&SettingValue>) {
        match value {
            None => self.write_u64(0),
            Some(value) => {
                self.write_u64(1);
                self.write_setting(value);
            }
        }
    }

    fn write_tween(&mut self, tween: &Tween, digits: u32) {
        self.write_rounded(tween.value, digits);
    }
}

/// `round(x, digits)` como entero escalado; `-0.0` y `0.0` coinciden.
pub(super) fn round_scaled(value: f64, digits: u32) -> i64 {
    let scale = 10f64.powi(digits as i32);
    let scaled = value * scale;
    if scaled.is_finite() {
        scaled.round_ties_even() as i64
    } else {
        i64::MAX
    }
}

impl Menu {
    /// `_layer_signature_value`: huella de todo lo que pintan las capas.
    pub fn layer_signature_value(&self, now: f64) -> u64 {
        let mut fp = Fingerprint::new();
        self.signature_identity(&mut fp);
        self.signature_interaction(&mut fp);
        self.signature_runtime(&mut fp, now);
        self.signature_tweens(&mut fp);
        self.signature_sources(&mut fp);
        fp.0
    }

    fn signature_identity(&self, fp: &mut Fingerprint) {
        fp.write_str(self.theme);
        fp.write_i64(palette_version());
        fp.write_rounded(self.scale, 4);
        match self.layer_shape {
            None => fp.write_u64(0),
            Some((height, width, channels)) => {
                fp.write_u64(1);
                fp.write_u64(height as u64);
                fp.write_u64(width as u64);
                fp.write_u64(channels as u64);
            }
        }
        fp.write_str(&self.title);
        fp.write_u64(self.groups.len() as u64);
        for (name, rows) in &self.groups {
            fp.write_str(name);
            fp.write_u64(rows.len() as u64);
            for (key, kind) in rows {
                fp.write_str(key);
                fp.write_str(kind);
            }
        }
        fp.write_rounded(self.viewport_height(), 3);
    }

    fn signature_interaction(&self, fp: &mut Fingerprint) {
        fp.write_opt_str(self.selector_key.as_deref());
        fp.write_bool(self.selector_closing);
        fp.write_rounded(self.selector_extra.value, 3);
        fp.write_rounded(self.scroll_tween.value, 3);
        fp.write_rounded(self.list_scroll, 3);
        fp.write_opt_str(self.hover_id.as_deref());
        fp.write_opt_index(self.hover_option);
        fp.write_opt_str(self.editing_key.as_deref());
        fp.write_str(&self.edit_buffer);
        match &self.edit_error {
            None => fp.write_u64(0),
            Some((key, message)) => {
                fp.write_u64(1);
                fp.write_str(key);
                fp.write_str(message);
            }
        }
        fp.write_u64(self.edit_cursor as u64);
        fp.write_bool(self.capturing);
        fp.write_u64(self.capture_mods.len() as u64);
        for name in &self.capture_mods {
            fp.write_str(name);
        }
    }

    fn signature_runtime(&self, fp: &mut Fingerprint, now: f64) {
        let message = self
            .capture_msg
            .as_ref()
            .is_some_and(|(_, until)| now < *until);
        fp.write_bool(message);
        fp.write_i64(if self.capturing {
            (now * 6.0) as i64
        } else {
            0
        });
        for key in ["llm_profile", "theme", "show_advanced"] {
            fp.write_opt_setting(self.settings.get(key));
        }
        fp.write_rounded(self.dx, 2);
        fp.write_rounded(self.meter_level, 3);
        fp.write_rounded(self.meter_target, 3);
        fp.write_bool(self.meter_muted);
        fp.write_bool(self.meter_active);
        fp.write_bool(self.settings.dirty());
        match &self.footer_state {
            None => fp.write_u64(0),
            Some((text, color, dot)) => {
                fp.write_u64(1);
                fp.write_str(text);
                fp.write_str(color);
                fp.write_bool(*dot);
            }
        }
        fp.write_opt_str(self.save_variant.as_deref());
        fp.write_bool(now < self.check_until);
        fp.write_bool(now < self.saved_flash_until);
        fp.write_bool(now < self.error_until);
        fp.write_opt_str(self.copy_hover.as_deref());
        let copied: Vec<&str> = self
            .copied
            .iter()
            .filter(|(_key, until)| now < **until)
            .map(|(key, _until)| key.as_str())
            .collect();
        fp.write_u64(copied.len() as u64);
        for key in copied {
            fp.write_str(key);
        }
        fp.write_bool(self.footer_prev.is_some());
        fp.write_bool(self.save_prev_fill.is_some());
    }

    fn signature_tweens(&self, fp: &mut Fingerprint) {
        let tweens = content_tweens(self);
        fp.write_u64(tweens.len() as u64);
        for tween in tweens {
            fp.write_tween(tween, 4);
        }
    }

    fn signature_sources(&self, fp: &mut Fingerprint) {
        let snapshot = self.settings.snapshot();
        fp.write_u64(snapshot.len() as u64);
        for (key, value) in snapshot.iter() {
            fp.write_str(key);
            fp.write_setting(value);
        }
        let devices = self.settings.devices();
        fp.write_u64(devices.len() as u64);
        for device in devices {
            fp.write_i64(device.index);
            fp.write_str(&device.name);
        }
        let voices = self.settings.voices();
        fp.write_u64(voices.len() as u64);
        for option in voices {
            fp.write_str(&option.value.python_str());
            fp.write_str(&option.label);
        }
    }
}
