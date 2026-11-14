//! Port of `src/ezagent/voice/hud/menu.py 1624-1885` (F5-08).
//!
//! Puntero del menú: hover, pulsación, arrastre y activación del control
//! soltado. Mismas transiciones y mismos avisos que el Python.

mod activation;

use crate::anim::{Tween, EASE_OUT, EASE_SPRING};
use crate::layout;
use crate::model::settings::{SettingValue, SLIDER_KEYS};
use crate::tokens::{H_ISLAND, T_MICRO};

use super::geometry::{Menu, MenuHit, MenuHitKind, SLIDER_THUMB_REST};
use super::rects::{inside, GROUP_X0, GROUP_X1};

/// Retorno del botón al soltar.
pub const BUTTON_PRESS_MS: f64 = 60.0;
/// Pulgar del slider al pasar por su fila.
pub const SLIDER_THUMB_HOVER: f64 = 16.0;
/// Pulgar del slider mientras se arrastra.
pub const SLIDER_THUMB_DRAG: f64 = 18.0;
/// Crecimiento del pulgar al entrar en la fila.
pub const SLIDER_HOVER_MS: f64 = 120.0;
/// Crecimiento del pulgar al empezar a arrastrar.
pub const SLIDER_DRAG_MS: f64 = 80.0;
/// Retorno del pulgar al soltar.
pub const SLIDER_RELEASE_MS: f64 = 180.0;
/// Tiempo que el icono de copiar muestra el check.
pub const COPIED_SECS: f64 = 1.6;
/// Ventana para confirmar una acción destructiva.
pub const ARM_SECS: f64 = 3.0;
/// Acciones que piden un segundo clic.
pub const CONFIRM_ACTIONS: [&str; 1] = ["new_conversation"];

impl Menu {
    /// `_row_at`: fila bajo el puntero, aunque esté deshabilitada.
    fn row_at(
        &self,
        x: f64,
        y: f64,
        layout: &std::collections::BTreeMap<String, layout::Rect>,
        scroll: f64,
    ) -> Option<(String, String, bool)> {
        let content_top = self.content_top(layout);
        let footer_top = self.footer_top(layout);
        let window_h = f64::from(H_ISLAND) + self.height_tween.value;
        if !(content_top <= y && y <= footer_top.min(window_h)) {
            return None;
        }
        for (_name, rows) in self.shown_groups() {
            for (key, kind) in rows {
                let row = *layout.get(&key).expect("la fila visible tiene layout");
                let base_h = f64::from(layout::row_height(&kind).expect("tipo de fila conocido"));
                let y0 = row.y0 - scroll;
                let y1 = y0 + base_h;
                if GROUP_X0 <= x && x <= GROUP_X1 + self.dx && y0 <= y && y <= y1 {
                    let disabled = kind == "select" && !self.settings.selector_enabled(&key);
                    return Some((key, kind, disabled));
                }
            }
        }
        None
    }

    /// `_sync_hover`: actualiza la fila/botón/opción bajo el puntero.
    pub(crate) fn sync_hover(&mut self, x: f64, y: f64, now: f64) {
        let layout = self.layout();
        let scroll = self.scroll_offset();
        let window_h = f64::from(H_ISLAND) + self.height_tween.value;
        let (new_id, new_option) = self.hover_target(x, y, &layout, scroll, window_h);
        self.apply_hover(&new_id, new_option, now);
        let hit = self.hit(x, y, now);
        self.copy_hover = hit
            .as_ref()
            .filter(|hit| hit.kind == MenuHitKind::Copy)
            .map(|hit| hit.key.clone());
        self.update_thumb_hover(&new_id, now);
    }

    fn hover_target(
        &self,
        x: f64,
        y: f64,
        layout: &std::collections::BTreeMap<String, layout::Rect>,
        scroll: f64,
        window_h: f64,
    ) -> (Option<String>, Option<usize>) {
        let hide = self.hide_rect(layout);
        let save = self.save_rect(layout);
        let test = self.test_rect(layout, scroll);
        let header = layout
            .get("header")
            .expect("el layout siempre tiene cabecera");
        let footer = layout.get("footer").expect("el layout siempre tiene pie");
        if header.y0 <= y
            && y <= header.y1
            && inside(hide, x, y)
            && (x - hide.center().0).hypot(y - hide.center().1) <= super::rects::HIDE_SIZE / 2.0
        {
            return (Some("btn:hide".to_string()), None);
        }
        if footer.y0 <= y
            && y <= footer.y1.min(window_h)
            && inside(save, x, y)
            && self.settings.dirty()
        {
            return (Some("btn:save".to_string()), None);
        }
        if inside(test, x, y) && self.testing_t.value <= 0.01 {
            return (Some("btn:test".to_string()), None);
        }
        if let Some(index) = self.hover_selector_option(x, y, layout, scroll) {
            return (None, Some(index));
        }
        if let Some((key, _kind, disabled)) = self.row_at(x, y, layout, scroll) {
            if !disabled {
                return (Some(format!("row:{key}")), None);
            }
        }
        (None, None)
    }

    fn hover_selector_option(
        &self,
        x: f64,
        y: f64,
        layout: &std::collections::BTreeMap<String, layout::Rect>,
        scroll: f64,
    ) -> Option<usize> {
        if self.selector_key.is_none() || self.selector_closing {
            return None;
        }
        let geometry = self.selector_geometry(layout, scroll, None)?;
        if !(GROUP_X0 <= x && x <= GROUP_X1 + self.dx)
            || !(geometry.top <= y && y <= geometry.top + geometry.visible)
        {
            return None;
        }
        for index in 0..geometry.options.len() {
            let item_y0 =
                geometry.top + index as f64 * super::geometry::LIST_ITEM_H - geometry.list_scroll;
            let item_y1 = item_y0 + super::geometry::LIST_ITEM_H;
            if item_y0 < geometry.top - 0.5 || item_y1 > geometry.top + geometry.visible + 0.5 {
                continue;
            }
            if item_y0 <= y && y <= item_y1 {
                return Some(index);
            }
        }
        None
    }

    fn apply_hover(&mut self, new_id: &Option<String>, new_option: Option<usize>, now: f64) {
        if *new_id != self.hover_id {
            if let Some(old) = self.hover_id.clone() {
                if let Some(target) = self.hover_tween(&old) {
                    target.to(0.0, now, f64::from(T_MICRO), EASE_OUT);
                }
            }
            if let Some(new) = new_id {
                if let Some(target) = self.hover_tween(new) {
                    target.to(1.0, now, f64::from(T_MICRO), EASE_OUT);
                }
            }
            self.hover_id = new_id.clone();
        }
        self.hover_option = new_option;
    }

    fn update_thumb_hover(&mut self, new_id: &Option<String>, now: f64) {
        // Thumb del slider: 16 px al pasar por su fila, 14 al salir.
        for key in SLIDER_KEYS {
            if self.slider_drag.as_ref().is_some_and(|hit| hit.key == key) {
                continue;
            }
            let row_id = format!("row:{key}");
            let hovering = new_id.as_deref() == Some(row_id.as_str());
            let target = if hovering {
                SLIDER_THUMB_HOVER
            } else {
                SLIDER_THUMB_REST
            };
            let current = self
                .thumb_target
                .get(key)
                .copied()
                .unwrap_or(SLIDER_THUMB_REST);
            if current != target {
                self.thumb_target.insert((*key).to_string(), target);
                if let Some(tween) = self.thumb.get_mut(key) {
                    tween.to(target, now, SLIDER_HOVER_MS, EASE_OUT);
                }
            }
        }
    }

    fn hover_tween(&mut self, hover_id: &str) -> Option<&mut Tween> {
        if let Some(key) = hover_id.strip_prefix("row:") {
            return self.row_hover.get_mut(key);
        }
        let key = match hover_id {
            "btn:hide" => "hide",
            "btn:save" => "save",
            "btn:test" => "test",
            _ => return None,
        };
        self.button_hover.get_mut(key)
    }

    /// Texto de una fila secreta: lo que se escribe o lo guardado.
    pub(crate) fn secret_text(&self, key: &str) -> String {
        if self.editing_key.as_deref() == Some(key) {
            return self.edit_buffer.clone();
        }
        self.settings
            .get(key)
            .map(|value| {
                if value.truthy() {
                    value.python_str()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default()
    }

    fn copy_secret(&mut self, key: &str, now: f64) {
        let text = self.secret_text(key);
        if !text.is_empty() && self.notify_copy(&text) {
            self.copied.insert(key.to_string(), now + COPIED_SECS);
        }
    }

    /// Un clic fuera de la fila en edición la confirma; si falla, sigue activa.
    fn finish_foreign_text_edit(&mut self, hit: Option<&MenuHit>) -> bool {
        let Some(editing) = self.editing_key.clone() else {
            return false;
        };
        let same_text = hit.is_some_and(|hit| hit.kind == MenuHitKind::Text && hit.key == editing);
        if same_text {
            return false;
        }
        self.apply_text_edit(true);
        self.editing_key.is_some()
    }

    /// Pulsa un control. Devuelve `true` dentro del menú (evita arrastrar).
    pub fn pointer_down(&mut self, x: f64, y: f64, now: f64) -> bool {
        self.advance(now);
        self.now = now;
        if !self.open {
            return false;
        }
        self.pointer = Some((x, y));
        self.indicator_activity(now);
        self.sync_hover(x, y, now);
        let hit = self.hit(x, y, now);
        if self.finish_foreign_text_edit(hit.as_ref()) {
            return true;
        }
        if hit
            .as_ref()
            .is_some_and(|hit| hit.kind == MenuHitKind::Meter)
        {
            self.meter_drag = true;
            self.meter_pending = None;
            self.meter_changed = false;
            activation::set_meter_from_x(self, x, now);
            return true;
        }
        if let Some(hit) = hit.as_ref().filter(|hit| hit.kind == MenuHitKind::Slider) {
            self.slider_drag = Some(hit.clone());
            self.pressed_hit = None;
            self.slider_changed = false;
            self.slider_pending = None;
            self.thumb_target.insert(hit.key.clone(), SLIDER_THUMB_DRAG);
            if let Some(tween) = self.thumb.get_mut(&hit.key) {
                tween.to(SLIDER_THUMB_DRAG, now, SLIDER_DRAG_MS, EASE_OUT);
            }
            if let Some(tween) = self.slider_color.get_mut(&hit.key) {
                tween.to(1.0, now, f64::from(T_MICRO), EASE_OUT);
            }
            activation::set_slider_from_x(self, &hit.key, x, now);
            return true;
        }
        self.pressed_hit = hit.clone();
        if let Some(hit) = hit {
            let press_key = match hit.kind {
                MenuHitKind::Hide => Some("hide"),
                MenuHitKind::Save => Some("save"),
                MenuHitKind::Action => Some("test"),
                _ => None,
            };
            if let Some(key) = press_key {
                if let Some(tween) = self.press.get_mut(key) {
                    tween.to(1.0, now, BUTTON_PRESS_MS, EASE_OUT);
                }
            }
        }
        true
    }

    /// Actualiza hover y arrastre. Devuelve `true` mientras el menú siga abierto.
    pub fn pointer_move(&mut self, x: f64, y: f64, now: f64) -> bool {
        self.advance(now);
        self.now = now;
        if !self.open {
            return false;
        }
        self.pointer = Some((x, y));
        self.indicator_activity(now);
        self.sync_hover(x, y, now);
        if self.meter_drag {
            activation::set_meter_from_x(self, x, now);
        }
        if let Some(hit) = self.slider_drag.clone() {
            activation::set_slider_from_x(self, &hit.key, x, now);
        }
        true
    }

    /// Confirma el clic al soltar sobre el mismo control.
    pub fn pointer_up(&mut self, x: f64, y: f64, now: f64) -> bool {
        self.advance(now);
        self.now = now;
        if !self.open {
            return false;
        }
        self.pointer = Some((x, y));
        self.indicator_activity(now);
        if self.meter_drag {
            activation::set_meter_from_x(self, x, now);
            self.release_meter_drag(now, true);
            self.sync_hover(x, y, now);
            return true;
        }
        if let Some(hit) = self.slider_drag.clone() {
            let key = hit.key.clone();
            activation::set_slider_from_x(self, &key, x, now);
            self.release_slider_drag(now, true);
            let hit = self.hit(x, y, now);
            let hovering = hit
                .as_ref()
                .is_some_and(|hit| hit.kind == MenuHitKind::Slider && hit.key == key);
            let target = if hovering {
                SLIDER_THUMB_HOVER
            } else {
                SLIDER_THUMB_REST
            };
            self.thumb_target.insert(key.clone(), target);
            if let Some(tween) = self.thumb.get_mut(&key) {
                tween.to(target, now, SLIDER_RELEASE_MS, EASE_SPRING);
            }
            self.sync_hover(x, y, now);
            return true;
        }
        let pressed = self.pressed_hit.take();
        self.release_press(now);
        let hit = self.hit(x, y, now);
        if Menu::same_hit(pressed.as_ref(), hit.as_ref()) {
            if let Some(hit) = hit.as_ref() {
                self.activate_hit(hit, now);
            }
        }
        self.sync_hover(x, y, now);
        true
    }

    /// `_activate_hit`: efecto del clic confirmado.
    pub fn activate_hit(&mut self, hit: &MenuHit, now: f64) {
        match hit.kind {
            MenuHitKind::Hide => {
                self.close(now);
                self.notify_hide();
            }
            MenuHitKind::Save => activation::save(self, now),
            MenuHitKind::Action => {
                if hit.key == "test_voice" {
                    activation::start_test(self, now);
                } else if CONFIRM_ACTIONS.contains(&hit.key.as_str())
                    && !self.is_armed(&hit.key, now)
                {
                    self.armed.insert(hit.key.clone(), now + ARM_SECS);
                } else {
                    self.armed.remove(&hit.key);
                    self.notify_action(&hit.key);
                }
            }
            MenuHitKind::Copy => self.copy_secret(&hit.key, now),
            MenuHitKind::Select => self.toggle_selector(&hit.key, now),
            MenuHitKind::Option => {
                let value = hit.value.clone().expect("una opción lleva valor");
                self.settings
                    .set(&hit.key, &value)
                    .expect("el valor de una opción es válido");
                self.close_selector(now);
                self.notify_live_change(&hit.key, &value);
            }
            MenuHitKind::Palette => {
                let value = hit.value.clone().expect("una paleta lleva valor");
                self.settings
                    .set("theme", &value)
                    .expect("el valor de una paleta es válido");
                self.notify_live_change("theme", &value);
            }
            MenuHitKind::Toggle => {
                let value = !self
                    .settings
                    .get(&hit.key)
                    .is_some_and(|value| value.truthy());
                self.settings
                    .set(&hit.key, &SettingValue::Bool(value))
                    .expect("un interruptor acepta bool");
                self.animate_toggle(&hit.key, now);
                self.notify_live_change(&hit.key, &SettingValue::Bool(value));
            }
            MenuHitKind::Text => activation::begin_text_edit(self, &hit.key, Some(now)),
            MenuHitKind::Slider | MenuHitKind::Meter => {}
        }
    }

    /// `_animate_toggle`: el interruptor sigue al valor y el color también.
    pub fn animate_toggle(&mut self, key: &str, now: f64) {
        let target = if self.settings.get(key).is_some_and(|value| value.truthy()) {
            1.0
        } else {
            0.0
        };
        if let Some(tween) = self.toggle_pos.get_mut(key) {
            tween.to(target, now, super::geometry::TOGGLE_MS, EASE_SPRING);
        }
        if let Some(tween) = self.toggle_color.get_mut(key) {
            tween.to(target, now, super::geometry::TOGGLE_MS, EASE_OUT);
        }
    }
}

#[cfg(test)]
mod tests;
