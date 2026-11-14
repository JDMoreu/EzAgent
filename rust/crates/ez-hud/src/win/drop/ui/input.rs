//! Puntero y rueda de la barra (`drop.py 1570-1661`).

use ez_core::metrics::monotonic;

use crate::model::drop::{choose_dock, form_center, DOUBLE_CLICK};

use super::traits::ConfigValue;
use super::{clean_bar_scale, DropUI};
use crate::model::settings::UI_SCALE_STEP;

impl DropUI {
    /// `drag_target`: esquina permitida para un arrastre; los botones cambian
    /// de lado cerca de un borde.
    pub fn drag_target(&self, x: i32, y: i32) -> (i32, i32) {
        let Some(window) = self.window() else {
            return self.clamp_position(x, y);
        };
        let (layout, dock, scale, bar_scale) = {
            let bar = self.bar.lock();
            (bar.layout, bar.dock, bar.scale, bar.bar_scale)
        };
        let bs = scale * bar_scale;
        let (form_x, form_y) = form_center(layout, Some(dock));
        let anchor = (x as f64 + form_x * bs, y as f64 + form_y * bs);
        let chosen = choose_dock(
            layout,
            Some(dock),
            anchor,
            crate::win::monitor::work_area(),
            bs,
        );
        let mut target = (x, y);
        if chosen != dock {
            let (next_x, next_y) = form_center(layout, Some(chosen));
            {
                let mut bar = self.bar.lock();
                bar.dock = chosen;
                bar.model.dock = chosen;
            }
            window.shift_drag(
                ((next_x - form_x) * bs).round_ties_even() as i32,
                ((next_y - form_y) * bs).round_ties_even() as i32,
            );
            let (width, height) = self.size();
            window.resize(width, height);
            target = self.origin_for(anchor, chosen);
        }
        self.clamp_position(target.0, target.1)
    }

    /// Botón bajo un punto lógico (`None` fuera de la forma y los botones).
    pub fn item_at(&self, x: f64, y: f64) -> Option<&'static str> {
        self.bar.lock().model.item_at(x, y)
    }

    /// `pointer_down`: pulsa un botón o deja arrastrar la forma.
    pub fn pointer_down(&self, x: f64, y: f64) -> bool {
        if self.bar.lock().model.form_hidden {
            return true;
        }
        let Some(name) = self.item_at(x, y) else {
            return false;
        };
        if name == "pill" {
            let now = monotonic();
            let previous = self.bar.lock().form_click;
            if previous.is_some_and(|previous| now - previous < DOUBLE_CLICK) {
                self.bar.lock().form_click = None;
                self.toggle_layout();
                return true;
            }
            self.bar.lock().form_click = Some(now);
            return false;
        }
        self.bar.lock().pressed = Some(name.to_string());
        true
    }

    /// `pointer_up`: activa el botón si el puntero sigue sobre él.
    pub fn pointer_up(&self, x: f64, y: f64) -> bool {
        let pressed = self.bar.lock().pressed.take();
        let Some(name) = pressed else {
            return true;
        };
        if self.item_at(x, y) != Some(name.as_str()) {
            return true;
        }
        self.activate(&name);
        true
    }

    /// `_activate`: la acción de cada botón.
    fn activate(&self, name: &str) {
        let (asking, in_call, on_yes, on_no) = {
            let bar = self.bar.lock();
            (
                bar.asking,
                bar.in_call,
                self.on_yes.lock().clone(),
                self.on_no.lock().clone(),
            )
        };
        match name {
            "gear" => {
                if !asking {
                    self.host.action("panel");
                }
            }
            "chat" => {
                if !asking {
                    self.host.action("chat");
                }
            }
            "mic" => {
                if asking {
                    if let Some(on_yes) = on_yes {
                        on_yes();
                    }
                } else {
                    self.host.action(if in_call { "mute" } else { "listen" });
                }
            }
            "hang" => {
                if asking {
                    if let Some(on_no) = on_no {
                        on_no();
                    }
                } else if in_call {
                    self.host.action("hangup");
                } else {
                    self.host.action("listen");
                }
            }
            _ => {}
        }
    }

    /// `bar_wheel`: Ctrl + rueda sobre la barra cambia el tamaño de 5 en 5 %.
    pub fn bar_wheel(&self, x: f64, y: f64, delta: i32) -> bool {
        if !crate::win::keyboard::ctrl_down() || delta == 0 || self.item_at(x, y).is_none() {
            return false;
        }
        let step = if delta > 0 { 1.0 } else { -1.0 };
        let target = clean_bar_scale(self.bar_scale() + step * UI_SCALE_STEP);
        if target != self.bar_scale() {
            self.set_bar_scale(target);
            let _ = self.menu_call(|menu| menu.set_live_scale(target));
            self.live_setting("ui_scale", ConfigValue::Float(target));
        }
        true
    }

    /// `cursor_hand`: mano sobre un botón, no sobre la forma.
    pub fn cursor_hand(&self, x: i32, y: i32) -> bool {
        let scale = self.bs();
        self.item_at(x as f64 / scale, y as f64 / scale)
            .is_some_and(|item| item != "pill")
    }
}
