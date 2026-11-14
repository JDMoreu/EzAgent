//! Port of `src/ezagent/voice/hud/menu.py 1962-2032` (F5-09).
//!
//! Rueda del ratón: lista abierta, scroll del menú o paso del slider. Es modelo
//! puro; no toca Win32 ni dibuja.

use crate::anim::EASE_OUT;
use crate::model::settings::{SettingValue, SLIDERS};

use super::geometry::{
    Menu, MenuHitKind, LIST_ITEM_H, LIST_MAX_VISIBLE, LIST_VIEW_H, SCROLL_TWEEN_MS,
};
use super::pointer::{SLIDER_HOVER_MS, SLIDER_THUMB_HOVER};
use super::rects::{GROUP_X0, GROUP_X1};
use super::sliders::as_float;

/// Desplazamiento del menú por muesca de rueda.
pub const SCROLL_STEP: f64 = 40.0;

/// `int(delta / 120)` con al menos una muesca: `trunc` como el Python.
fn notches_of(delta: i32) -> i32 {
    let notches = (f64::from(delta) / 120.0).trunc() as i32;
    if notches == 0 {
        if delta > 0 {
            1
        } else {
            -1
        }
    } else {
        notches
    }
}

impl Menu {
    /// `wheel`: lista interna, scroll del menú o paso del slider.
    ///
    /// `delta` es el de Windows (120 por muesca); devuelve `true` si la rueda
    /// se consumió dentro del menú.
    pub fn wheel(&mut self, x: f64, y: f64, delta: i32, now: f64) -> bool {
        self.advance(now);
        self.now = now;
        if !self.open || delta == 0 {
            return false;
        }
        self.pointer = Some((x, y));
        self.indicator_activity(now);
        let notches = notches_of(delta);
        self.wheel_list(x, y, notches, now)
            || self.wheel_menu(x, y, notches, now)
            || self.wheel_slider(x, y, notches, now)
    }

    /// Prioridad 1: lista abierta con más de cinco elementos.
    fn wheel_list(&mut self, x: f64, y: f64, notches: i32, now: f64) -> bool {
        if self.selector_key.is_none() || self.selector_closing {
            return false;
        }
        let layout = self.layout();
        let scroll = self.scroll_offset();
        let Some(geometry) = self.selector_geometry(&layout, scroll, None) else {
            return false;
        };
        if geometry.options.len() <= LIST_MAX_VISIBLE
            || !(GROUP_X0 <= x && x <= GROUP_X1 + self.dx)
            || !(geometry.top <= y && y <= geometry.top + geometry.visible)
        {
            return false;
        }
        let maximum = geometry.options.len() as f64 * LIST_ITEM_H - LIST_VIEW_H;
        let target = self.list_scroll_target - f64::from(notches) * LIST_ITEM_H;
        self.list_scroll_target = target.min(maximum).max(0.0);
        self.list_scroll = self.list_scroll_target;
        self.sync_hover(x, y, now);
        true
    }

    /// Prioridad 2: si el menú no cabe, la rueda desplaza el menú.
    fn wheel_menu(&mut self, x: f64, y: f64, notches: i32, now: f64) -> bool {
        let max_scroll = self.scroll_max();
        if max_scroll <= 0.5 {
            return false;
        }
        let target = self.scroll_target - f64::from(notches) * SCROLL_STEP;
        self.scroll_target = target.min(max_scroll).max(0.0);
        self.scroll_tween
            .to(self.scroll_target, now, SCROLL_TWEEN_MS, EASE_OUT);
        self.sync_hover(x, y, now);
        true
    }

    /// Prioridad 3: slider bajo el puntero (solo si el menú no scrollea).
    fn wheel_slider(&mut self, x: f64, y: f64, notches: i32, now: f64) -> bool {
        let Some(hit) = self.hit(x, y, now) else {
            return false;
        };
        if hit.kind != MenuHitKind::Slider {
            return false;
        }
        // // DECISION: una clave sin especificación se ignora en vez de lanzar
        // // el `KeyError`; solo un hit de slider llega hasta aquí.
        let Some(spec) = SLIDERS.get(hit.key.as_str()) else {
            return true;
        };
        let old = self
            .settings
            .get(&hit.key)
            .cloned()
            .unwrap_or(SettingValue::None);
        let value = as_float(&old) + f64::from(notches) * spec.step;
        if let Ok(new) = self.settings.set(&hit.key, &SettingValue::Float(value)) {
            if new != old {
                self.notify_live_change(&hit.key, &new);
            }
        }
        self.thumb_target
            .insert(hit.key.clone(), SLIDER_THUMB_HOVER);
        if let Some(tween) = self.thumb.get_mut(&hit.key) {
            tween.to(SLIDER_THUMB_HOVER, now, SLIDER_HOVER_MS, EASE_OUT);
        }
        true
    }
}

#[cfg(test)]
mod tests;
