//! Port of `src/ezagent/voice/hud/menu.py 699-1342` (F5-07): apertura, cierre
//! y reloj de animaciones del menú.
//!
//! Todos los tiempos van en segundos y deben ser monótonos, como
//! `time.monotonic()` en el Python.

use std::collections::BTreeMap;

use crate::anim::{Tween, EASE_IN, EASE_OUT};
use crate::model::settings::{SLIDER_KEYS, TOGGLE_KEYS};
use crate::tokens::T_CLOSE_RETRACT;

use super::geometry::{
    block_delays, Menu, BLOCK_FADE_MS, CLOSE_HEIGHT_DELAY_MS, CONTENT_CLOSE_MS, FOOTER_FADE_MS,
    SAVE_COLOR_MS, SCROLL_TWEEN_MS, TOGGLE_MS, T_SLOW_MS,
};

impl Menu {
    /// Abre el menú (o invierte un cierre en curso) desde el valor actual.
    pub fn open(&mut self, now: f64) {
        self.advance(now);
        let fresh = !self.open && self.height_tween.value <= 0.5;
        if self.open && self.height_tween.value >= self.viewport_height() - 0.5 {
            return;
        }
        self.open = true;
        self.closed_notified = false;
        if fresh {
            self.cancel_text_edit_quiet();
            self.selector_key = None;
            self.selector_closing = false;
            self.selector_extra.to(0.0, now, 0.0, EASE_OUT);
            self.list_scroll = 0.0;
            self.list_scroll_target = 0.0;
            self.scroll_tween.to(0.0, now, 0.0, EASE_OUT);
            self.scroll_target = 0.0;
            self.reset_interaction(now);
            let fade = if self.reduced_motion { 120.0 } else { 0.0 };
            self.content_alpha.to(1.0, now, fade, EASE_OUT);
            self.blocks = block_delays()
                .into_iter()
                .map(|(name, delay)| {
                    let mut tween = Tween::new(0.0);
                    let (start, duration) = if self.reduced_motion {
                        (now, 120.0)
                    } else {
                        (now + delay / 1000.0, BLOCK_FADE_MS)
                    };
                    tween.to(1.0, start, duration, EASE_OUT);
                    (name, tween)
                })
                .collect();
        } else {
            // Inversión a mitad de cierre: el contenido vuelve sin escalonado.
            self.content_alpha.to(1.0, now, CONTENT_CLOSE_MS, EASE_OUT);
        }
        let duration = if self.reduced_motion { 0.0 } else { T_SLOW_MS };
        self.height_tween
            .to(self.viewport_height(), now, duration, EASE_OUT);
    }

    /// Cierra el menú con el tween de retracción y el fundido de contenido.
    pub fn close(&mut self, now: f64) {
        self.advance(now);
        if !self.open {
            // Repetir close (p. ej. hide_hud después de on_hide) no reinicia A-05.
            return;
        }
        self.open = false;
        self.closed_notified = false;
        self.apply_text_edit(true);
        self.release_meter_drag(now, true);
        self.release_slider_drag(now, true);
        self.pressed_hit = None;
        self.release_press(now);
        self.content_alpha.to(0.0, now, CONTENT_CLOSE_MS, EASE_OUT);
        let duration = if self.reduced_motion {
            0.0
        } else {
            T_CLOSE_RETRACT as f64
        };
        let delay = if self.reduced_motion {
            0.0
        } else {
            CLOSE_HEIGHT_DELAY_MS / 1000.0
        };
        self.height_tween.to(0.0, now + delay, duration, EASE_IN);
    }

    /// Invierte el estado de apertura.
    pub fn toggle(&mut self, now: f64) {
        if self.is_open() {
            self.close(now);
        } else {
            self.open(now);
        }
    }

    /// Alto actual del cuerpo del menú, sin la barra.
    pub fn height(&mut self, now: f64) -> f64 {
        self.advance(now);
        self.height_tween.value.max(0.0).min(self.viewport_height())
    }

    /// Avanza tweens y notifica el cierre completo una sola vez.
    pub fn update(&mut self, now: f64) {
        self.now = now;
        self.advance(now);
        if self.capturing {
            // Las teclas que mantienes se ven en vivo, también al soltarlas.
            self.capture_mods = self.modifiers();
        }
        if !self.open && !self.closed_notified && self.height_tween.value <= 0.01 {
            self.height_tween.value = 0.0;
            self.content_alpha.value = 0.0;
            self.selector_key = None;
            self.selector_closing = false;
            self.selector_extra.value = 0.0;
            self.closed_notified = true;
            self.notify_close();
        }
    }

    /// Avanza todos los tweens y la física del medidor.
    pub(crate) fn advance(&mut self, now: f64) {
        self.height_tween.update(now);
        self.content_alpha.update(now);
        update_all(&mut self.blocks, now);
        self.selector_extra.update(now);
        update_all(&mut self.chevron, now);
        update_all(&mut self.row_hover, now);
        update_all(&mut self.button_hover, now);
        update_all(&mut self.press, now);
        update_all(&mut self.thumb, now);
        update_all(&mut self.slider_color, now);
        self.advance_toggles(now);
        self.scroll_tween.update(now);
        self.testing_t.update(now);
        self.collapse_selector();
        self.advance_scroll(now);
        self.advance_meter(now);
        self.clamp_list_scroll();
    }

    fn advance_toggles(&mut self, now: f64) {
        for key in TOGGLE_KEYS {
            let desired = if self.settings.get(key).is_some_and(|value| value.truthy()) {
                1.0
            } else {
                0.0
            };
            let position = self.toggle_pos.get_mut(key).expect("clave de interruptor");
            let color = self
                .toggle_color
                .get_mut(key)
                .expect("clave de interruptor");
            if !position.active() && (position.value - desired).abs() > 0.01 {
                position.to(desired, now, TOGGLE_MS, crate::anim::EASE_SPRING);
            }
            if !color.active() && (color.value - desired).abs() > 0.01 {
                color.to(desired, now, TOGGLE_MS, EASE_OUT);
            }
            position.update(now);
            color.update(now);
        }
    }

    fn collapse_selector(&mut self) {
        // El selector terminó de plegarse: se puede olvidar.
        if self.selector_closing
            && !self.selector_extra.active()
            && self.selector_extra.value <= 0.05
        {
            self.selector_key = None;
            self.selector_closing = false;
            self.selector_extra.value = 0.0;
            self.list_scroll = 0.0;
            self.list_scroll_target = 0.0;
            self.hover_option = None;
        }
    }

    fn advance_scroll(&mut self, now: f64) {
        let max_scroll = self.scroll_max();
        if self.scroll_target > max_scroll {
            self.scroll_target = max_scroll;
            if !self.scroll_tween.active() {
                self.scroll_tween
                    .to(max_scroll, now, SCROLL_TWEEN_MS, EASE_OUT);
            }
        }
        if self.scroll_tween.value > max_scroll && !self.scroll_tween.active() {
            self.scroll_tween.value = max_scroll;
        }
    }

    fn advance_meter(&mut self, now: f64) {
        // Balística del medidor: ataque rápido, caída suave.
        let dt = if self.meter_t == 0.0 {
            0.0
        } else {
            (now - self.meter_t).clamp(0.0, 0.05)
        };
        self.meter_t = now;
        let target = if self.meter_muted {
            0.0
        } else {
            self.meter_target
        };
        if dt > 0.0 {
            let tau = if target > self.meter_level {
                0.035
            } else {
                0.12
            };
            let k = 1.0 - (-dt / tau).exp();
            self.meter_level += (target - self.meter_level) * k;
        } else {
            self.meter_level = target;
        }
        if self.meter_level < 1e-7 {
            self.meter_level = 0.0;
        }
    }

    /// `True` si queda alguna animación que exija repintar.
    pub fn is_animating(&mut self, now: f64) -> bool {
        self.advance(now);
        if self.tween_animating() {
            return true;
        }
        // La onda de «Hablando…» es función del tiempo, no de un tween.
        if self.testing_t.value > 0.01 {
            return true;
        }
        // El cursor de texto parpadea; mientras se edita hay que repintar.
        if self.editing_key.is_some() {
            return true;
        }
        if self.open && (self.meter_active || (self.meter_target - self.meter_level).abs() > 1e-6) {
            return true;
        }
        // Timers sin tween: guardado, error e indicador de scroll.
        if now
            < self
                .check_until
                .max(self.saved_flash_until)
                .max(self.error_until)
        {
            return true;
        }
        if self.copied.values().any(|until| now < *until) {
            return true;
        }
        if self.footer_prev.is_some() && now - self.footer_state_at < FOOTER_FADE_MS / 1000.0 {
            return true;
        }
        if self.save_prev_fill.is_some() && now - self.save_variant_at < SAVE_COLOR_MS / 1000.0 {
            return true;
        }
        self.indicator_alpha(now) > 0.01
    }

    fn tween_animating(&self) -> bool {
        let single = [
            &self.height_tween,
            &self.content_alpha,
            &self.selector_extra,
            &self.scroll_tween,
            &self.testing_t,
        ];
        if single.iter().any(|tween| tween.active()) {
            return true;
        }
        [
            &self.blocks,
            &self.chevron,
            &self.row_hover,
            &self.button_hover,
            &self.press,
            &self.thumb,
            &self.slider_color,
            &self.toggle_pos,
            &self.toggle_color,
        ]
        .iter()
        .any(|map| map.values().any(Tween::active))
    }

    /// Limpia hover/pulsación/thumbs al abrir desde cerrado.
    pub(crate) fn reset_interaction(&mut self, now: f64) {
        self.hover_id = None;
        self.hover_option = None;
        self.pressed_hit = None;
        self.slider_drag = None;
        self.slider_pending = None;
        self.slider_changed = false;
        self.meter_drag = false;
        self.meter_pending = None;
        self.meter_changed = false;
        self.cancel_text_edit_quiet();
        for tween in self.row_hover.values_mut() {
            tween.to(0.0, now, 0.0, EASE_OUT);
        }
        for tween in self.button_hover.values_mut() {
            tween.to(0.0, now, 0.0, EASE_OUT);
        }
        for tween in self.press.values_mut() {
            tween.to(0.0, now, 0.0, EASE_OUT);
        }
        for key in crate::model::settings::SELECT_KEYS {
            self.chevron
                .get_mut(key)
                .expect("clave de selector")
                .to(0.0, now, 0.0, EASE_OUT);
        }
        for key in SLIDER_KEYS {
            self.thumb.get_mut(key).expect("clave de slider").to(
                super::geometry::SLIDER_THUMB_REST,
                now,
                0.0,
                EASE_OUT,
            );
            self.thumb_target
                .insert(key.to_string(), super::geometry::SLIDER_THUMB_REST);
            self.slider_color
                .get_mut(key)
                .expect("clave de slider")
                .to(0.0, now, 0.0, EASE_OUT);
        }
    }

    /// Recorta el scroll interno de la lista abierta a su máximo.
    pub(crate) fn clamp_list_scroll(&mut self) {
        let Some(key) = self.selector_key.clone() else {
            self.list_scroll = 0.0;
            self.list_scroll_target = 0.0;
            return;
        };
        let options = self.settings.select_options(&key).unwrap_or_default();
        let maximum = (options.len() as f64 * super::geometry::LIST_ITEM_H
            - super::geometry::LIST_VIEW_H)
            .max(0.0);
        self.list_scroll_target = self.list_scroll_target.clamp(0.0, maximum);
        self.list_scroll = self.list_scroll.clamp(0.0, maximum);
    }

    /// Modificadores pulsados según el callback de la aplicación.
    pub fn modifiers(&self) -> std::collections::BTreeSet<String> {
        self.callbacks
            .modifier_source
            .as_ref()
            .map_or_else(Default::default, |source| source())
    }
}

fn update_all(map: &mut BTreeMap<String, Tween>, now: f64) {
    for tween in map.values_mut() {
        tween.update(now);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::model::settings::{MenuSettings, SettingValue};

    use super::super::geometry::{MenuCallbacks, MenuOptions};
    use super::*;

    fn menu() -> Menu {
        Menu::new(
            MenuSettings::default(),
            MenuOptions::default(),
            MenuCallbacks::default(),
        )
        .expect("opciones válidas")
    }

    #[test]
    fn open_animates_height_and_blocks() {
        let mut menu = menu();
        let target = menu.viewport_height();
        assert_eq!(menu.height(0.0), 0.0);
        menu.open(1.0);
        assert!(menu.is_open());
        assert!(menu.is_animating(1.0));
        assert_eq!(menu.height(1.0), 0.0);
        assert_eq!(menu.height(1.4), target);
        assert!((menu.content_alpha.value - 1.0).abs() < 1e-9);
        assert!(menu.is_animating(1.4));
        assert!(!menu.is_animating(3.0));
    }

    #[test]
    fn close_notifies_once_and_leaves_zero_height() {
        let count = Rc::new(Cell::new(0));
        let counter = Rc::clone(&count);
        let callbacks = MenuCallbacks {
            close: Some(Box::new(move || counter.set(counter.get() + 1))),
            ..MenuCallbacks::default()
        };
        let mut menu = Menu::new(MenuSettings::default(), MenuOptions::default(), callbacks)
            .expect("opciones válidas");
        menu.open(0.0);
        menu.update(2.0);
        menu.close(2.0);
        assert!(!menu.is_open());
        assert!(menu.is_animating(2.0));
        menu.update(3.0);
        assert!(!menu.is_animating(3.0));
        assert_eq!(count.get(), 1);
        assert_eq!(menu.height(3.0), 0.0);
        menu.close(3.5);
        menu.update(4.0);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn reduced_motion_opens_and_closes_without_tweens() {
        let mut menu = menu();
        menu.set_reduced_motion(true);
        menu.open(1.0);
        assert_eq!(menu.height(1.0), menu.viewport_height());
        menu.close(2.0);
        menu.update(2.0);
        assert_eq!(menu.height(2.0), 0.0);
    }

    #[test]
    fn toggle_flips_the_logical_state() {
        let mut menu = menu();
        menu.toggle(0.0);
        assert!(menu.is_open());
        menu.toggle(0.1);
        assert!(!menu.is_open());
    }

    #[test]
    fn update_advances_the_meter_ballistics() {
        let mut menu = menu();
        menu.open(0.0);
        menu.set_input_level(0.5, false, true);
        menu.update(0.016);
        assert_eq!(menu.meter_level, 0.5);
        menu.set_input_level(0.0, false, false);
        menu.update(0.1);
        menu.update(0.2);
        menu.update(0.3);
        assert!(menu.meter_level < 0.2);
    }

    #[test]
    fn toggles_follow_the_settings_values() {
        let mut menu = menu();
        let mut settings = MenuSettings::default();
        settings
            .set("smart_turn", &SettingValue::Bool(true))
            .expect("interruptor válido");
        menu.set_settings(settings, Some(0.0));
        menu.update(0.2);
        let tween = menu.toggle_pos.get("smart_turn").expect("interruptor");
        assert!((tween.value - 1.0).abs() < 1e-9);
    }
}
