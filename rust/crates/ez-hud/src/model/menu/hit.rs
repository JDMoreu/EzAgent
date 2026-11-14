//! Port of `src/ezagent/voice/hud/menu.py 1481-1622` (F5-08).
//!
//! Prueba de impacto geométrica: qué control hay bajo `(x, y)` en coordenadas
//! lógicas de ventana. Solo modelo; no dibuja ni toca Win32.

use std::collections::BTreeMap;

use crate::layout;
use crate::model::settings::SECRET_KEYS;
use crate::tokens::{Rect, H_ISLAND};

use super::geometry::{Menu, MenuHit, MenuHitKind, LIST_ITEM_H};
use super::rects::{
    inside, GROUP_X0, GROUP_X1, METER_TRACK_H, METER_TRACK_PAD, METER_TRACK_Y, SLIDER_LINE2_CY,
    SLIDER_TRACK_H, SLIDER_TRACK_PAD, SLIDER_TRACK_X0, SLIDER_TRACK_X1,
};

impl Menu {
    /// Control bajo `(x, y)`; los deshabilitados se omiten.
    pub fn hit(&mut self, x: f64, y: f64, now: f64) -> Option<MenuHit> {
        self.hit_including(x, y, now, false)
    }

    /// `hit(..., include_disabled=True)`: también devuelve controles apagados.
    ///
    /// // DECISION: Rust no tiene argumentos con valor por defecto; `hit`
    /// // mantiene la firma del Python y esta variante añade el indicador.
    pub fn hit_including(
        &mut self,
        x: f64,
        y: f64,
        now: f64,
        include_disabled: bool,
    ) -> Option<MenuHit> {
        self.advance(now);
        self.now = now;
        if !self.open {
            return None;
        }
        let window_h = f64::from(H_ISLAND) + self.height_tween.value;
        if y < f64::from(H_ISLAND) - 1.0 || y > window_h + 0.5 {
            return None;
        }
        let layout = self.layout();
        let scroll = self.scroll_offset();

        if let Some(hit) = self.header_hit(x, y, &layout) {
            return Some(hit);
        }
        let content_top = self.content_top(&layout);
        let footer_top = self.footer_top(&layout);
        if content_top <= y && y <= footer_top.min(window_h) {
            if let Some(hit) = self.content_hit(x, y, &layout, scroll, include_disabled) {
                return Some(hit);
            }
        }
        self.footer_hit(x, y, &layout, window_h, include_disabled)
    }

    /// `_same_hit`: mismo tipo, clave e índice (el rectángulo no importa).
    pub fn same_hit(first: Option<&MenuHit>, second: Option<&MenuHit>) -> bool {
        match (first, second) {
            (Some(first), Some(second)) => {
                first.kind == second.kind && first.key == second.key && first.index == second.index
            }
            _ => false,
        }
    }

    fn header_hit(
        &self,
        x: f64,
        y: f64,
        layout: &BTreeMap<String, layout::Rect>,
    ) -> Option<MenuHit> {
        let header = layout
            .get("header")
            .expect("el layout siempre tiene cabecera");
        let hide = self.hide_rect(layout);
        if header.y0 <= y && y <= header.y1 && inside(hide, x, y) {
            let (cx, cy) = hide.center();
            if (x - cx).hypot(y - cy) <= super::rects::HIDE_SIZE / 2.0 {
                return Some(MenuHit::new(MenuHitKind::Hide, "hide", hide));
            }
        }
        None
    }

    fn content_hit(
        &self,
        x: f64,
        y: f64,
        layout: &BTreeMap<String, layout::Rect>,
        scroll: f64,
        include_disabled: bool,
    ) -> Option<MenuHit> {
        if let Some(hit) = self.selector_option_hit(x, y, layout, scroll) {
            return Some(hit);
        }
        for (_name, rows) in self.shown_groups() {
            for (key, kind) in rows {
                if let Some(hit) =
                    self.row_hit((x, y), &key, &kind, layout, scroll, include_disabled)
                {
                    return Some(hit);
                }
            }
        }
        None
    }

    fn selector_option_hit(
        &self,
        x: f64,
        y: f64,
        layout: &BTreeMap<String, layout::Rect>,
        scroll: f64,
    ) -> Option<MenuHit> {
        if self.selector_key.is_none() || self.selector_closing {
            return None;
        }
        let geometry = self.selector_geometry(layout, scroll, None)?;
        if !(GROUP_X0 <= x && x <= GROUP_X1 + self.dx)
            || !(geometry.top <= y && y <= geometry.top + geometry.visible)
        {
            return None;
        }
        for (index, option) in geometry.options.iter().enumerate() {
            let item_y0 = geometry.top + index as f64 * LIST_ITEM_H - geometry.list_scroll;
            let item_y1 = item_y0 + LIST_ITEM_H;
            if item_y0 < geometry.top - 0.5 || item_y1 > geometry.top + geometry.visible + 0.5 {
                continue;
            }
            if item_y0 <= y && y <= item_y1 {
                let key = self.selector_key.clone().expect("selector abierto");
                let rect = Rect::new(GROUP_X0, item_y0, GROUP_X1 + self.dx, item_y1);
                return Some(
                    MenuHit::new(MenuHitKind::Option, key, rect)
                        .with_value(option.value.clone())
                        .with_index(index),
                );
            }
        }
        None
    }

    fn row_hit(
        &self,
        point: (f64, f64),
        key: &str,
        kind: &str,
        layout: &BTreeMap<String, layout::Rect>,
        scroll: f64,
        include_disabled: bool,
    ) -> Option<MenuHit> {
        let row = *layout.get(key).expect("la fila visible tiene layout");
        let base_h = f64::from(layout::row_height(kind).expect("tipo de fila conocido"));
        let y0 = row.y0 - scroll;
        let y1 = y0 + base_h;
        let (x, y) = point;
        let disabled = kind == "select" && !self.settings.selector_enabled(key);
        if disabled && !include_disabled {
            return None;
        }
        let rect = Rect::new(GROUP_X0, y0, GROUP_X1 + self.dx, y1);
        match kind {
            "slider" => self.slider_hit(point, key, row.y0, scroll, rect, disabled),
            "select" => inside(rect, x, y)
                .then(|| MenuHit::new(MenuHitKind::Select, key, rect).with_disabled(disabled)),
            "toggle" => inside(rect, x, y).then(|| {
                let value = self.settings.get(key).is_some_and(|value| value.truthy());
                MenuHit::new(MenuHitKind::Toggle, key, rect)
                    .with_value(crate::model::settings::SettingValue::Bool(value))
                    .with_disabled(disabled)
            }),
            "text" => self.text_hit(x, y, key, y0, rect),
            "palette" => self.palette_hit(x, y, y0, y1),
            "meter" => self.meter_hit(x, y, key, y0, rect),
            "action" => self.action_hit(x, y, key, layout, scroll, include_disabled),
            _ => None,
        }
    }

    fn slider_hit(
        &self,
        point: (f64, f64),
        key: &str,
        row_y0: f64,
        scroll: f64,
        rect: Rect,
        disabled: bool,
    ) -> Option<MenuHit> {
        let (x, y) = point;
        let cy = row_y0 - scroll + SLIDER_LINE2_CY;
        let half = SLIDER_TRACK_H / 2.0 + SLIDER_TRACK_PAD;
        let on_track = SLIDER_TRACK_X0 <= x
            && x <= SLIDER_TRACK_X1 + self.dx
            && cy - half <= y
            && y <= cy + half;
        on_track.then(|| MenuHit::new(MenuHitKind::Slider, key, rect).with_disabled(disabled))
    }

    fn text_hit(&self, x: f64, y: f64, key: &str, row_y0: f64, rect: Rect) -> Option<MenuHit> {
        if SECRET_KEYS.contains(&key) && !self.secret_text(key).is_empty() {
            let button = self.copy_rect(row_y0);
            if inside(button, x, y) {
                return Some(MenuHit::new(MenuHitKind::Copy, key, button));
            }
        }
        inside(rect, x, y).then(|| MenuHit::new(MenuHitKind::Text, key, rect))
    }

    fn palette_hit(&self, x: f64, y: f64, y0: f64, y1: f64) -> Option<MenuHit> {
        for (value, _label, pill) in self.palette_pills(y0) {
            // El Python comprueba la fila entera (`y0..y1`), no la muestra:
            // cualquier punto de la fila cae en la primera muestra que cruce.
            if pill.x0 <= x && x <= pill.x1 && y0 <= y && y <= y1 {
                return Some(
                    MenuHit::new(MenuHitKind::Palette, "theme", pill)
                        .with_value(crate::model::settings::SettingValue::Text(value.into())),
                );
            }
        }
        None
    }

    fn meter_hit(&self, x: f64, y: f64, key: &str, y0: f64, rect: Rect) -> Option<MenuHit> {
        let track_cy = y0 + METER_TRACK_Y + METER_TRACK_H / 2.0;
        let half = METER_TRACK_H / 2.0 + METER_TRACK_PAD;
        let on_track = SLIDER_TRACK_X0 <= x
            && x <= SLIDER_TRACK_X1 + self.dx
            && track_cy - half <= y
            && y <= track_cy + half;
        on_track.then(|| MenuHit::new(MenuHitKind::Meter, key, rect))
    }

    fn action_hit(
        &self,
        x: f64,
        y: f64,
        key: &str,
        layout: &BTreeMap<String, layout::Rect>,
        scroll: f64,
        include_disabled: bool,
    ) -> Option<MenuHit> {
        let (button, disabled) = if key == "test_voice" {
            (self.test_rect(layout, scroll), self.testing_t.value > 0.01)
        } else {
            (self.action_rect(layout, key, scroll), false)
        };
        if disabled && !include_disabled {
            return None;
        }
        inside(button, x, y)
            .then(|| MenuHit::new(MenuHitKind::Action, key, button).with_disabled(disabled))
    }

    fn footer_hit(
        &self,
        x: f64,
        y: f64,
        layout: &BTreeMap<String, layout::Rect>,
        window_h: f64,
        include_disabled: bool,
    ) -> Option<MenuHit> {
        let footer = layout.get("footer").expect("el layout siempre tiene pie");
        if footer.y0 <= y && y <= footer.y1.min(window_h) {
            let save = self.save_rect(layout);
            let disabled = !self.settings.dirty();
            if disabled && !include_disabled {
                return None;
            }
            if inside(save, x, y) {
                return Some(MenuHit::new(MenuHitKind::Save, "save", save).with_disabled(disabled));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::model::settings::{MenuSettings, SettingValue};

    use super::super::geometry::{MenuCallbacks, MenuOptions};
    use super::super::rects::{SLIDER_LINE2_CY, SLIDER_TRACK_X0};
    use super::*;

    fn menu(rows: Vec<(&str, &str)>) -> Menu {
        let mut menu = Menu::new(
            MenuSettings::default(),
            MenuOptions::default(),
            MenuCallbacks::default(),
        )
        .expect("opciones válidas");
        menu.set_max_height(Some(2000.0));
        menu.set_groups(
            vec![(
                "prueba".to_string(),
                rows.into_iter()
                    .map(|(key, kind)| (key.to_string(), kind.to_string()))
                    .collect(),
            )],
            Some("Prueba"),
            Some(0.0),
        );
        menu.open(0.0);
        menu.height(2.0);
        menu
    }

    fn row_center(menu: &Menu, key: &str) -> (f64, f64) {
        let layout = menu.layout();
        let row = layout.get(key).expect("fila de prueba");
        (100.0, (row.y0 + row.y1) / 2.0)
    }

    #[test]
    fn the_header_circle_is_the_only_hide_target() {
        let mut menu = menu(vec![("repeat_last", "action")]);
        let hit = menu.hit(289.0, 62.0, 2.0).expect("botón «–»");
        assert_eq!(hit.kind, MenuHitKind::Hide);
        assert_eq!(hit.key, "hide");
        assert!(menu.hit(280.0, 46.0, 2.0).is_none());
        assert!(menu.hit(100.0, 62.0, 2.0).is_none());
    }

    #[test]
    fn slider_meter_and_toggle_only_answer_their_own_zone() {
        let mut menu = menu(vec![
            ("speed", "slider"),
            ("input_meter", "meter"),
            ("smart_turn", "toggle"),
        ]);
        let layout = menu.layout();
        let speed = layout.get("speed").copied().expect("slider");
        let track_y = speed.y0 + SLIDER_LINE2_CY;
        let slider = menu
            .hit(SLIDER_TRACK_X0 + 20.0, track_y, 2.0)
            .expect("pista del slider");
        assert_eq!(slider.kind, MenuHitKind::Slider);
        assert!(menu
            .hit(SLIDER_TRACK_X0 + 20.0, speed.y0 + 2.0, 2.0)
            .is_none());

        let meter = layout.get("input_meter").copied().expect("medidor");
        let meter_hit = menu
            .hit(SLIDER_TRACK_X0 + 20.0, meter.y0 + 38.0, 2.0)
            .expect("pista del medidor");
        assert_eq!(meter_hit.kind, MenuHitKind::Meter);
        assert_eq!(meter_hit.key, "input_meter");

        let (x, y) = row_center(&menu, "smart_turn");
        let toggle = menu.hit(x, y, 2.0).expect("interruptor");
        assert_eq!(toggle.kind, MenuHitKind::Toggle);
        assert_eq!(toggle.value, Some(SettingValue::Bool(true)));
    }

    #[test]
    fn a_disabled_selector_is_hidden_unless_requested() {
        let mut menu = menu(vec![("input_device_index", "select")]);
        let (x, y) = row_center(&menu, "input_device_index");
        assert!(menu.hit(x, y, 2.0).is_none());
        let hit = menu
            .hit_including(x, y, 2.0, true)
            .expect("selector deshabilitado");
        assert_eq!(hit.kind, MenuHitKind::Select);
        assert!(hit.disabled);
    }

    #[test]
    fn secret_rows_offer_the_copy_button_on_the_value_line() {
        let mut menu = menu(vec![("api_key", "text")]);
        menu.settings
            .set("api_key", &SettingValue::Text("sk-prueba".into()))
            .expect("clave válida");
        let layout = menu.layout();
        let row = layout.get("api_key").copied().expect("fila de texto");
        let copy = menu.copy_rect(row.y0);
        let hit = menu
            .hit(copy.center().0, copy.center().1, 2.0)
            .expect("botón copiar");
        assert_eq!(hit.kind, MenuHitKind::Copy);
        assert_eq!(hit.key, "api_key");
        let text = menu.hit(100.0, row.y0 + 10.0, 2.0).expect("fila");
        assert_eq!(text.kind, MenuHitKind::Text);
    }

    #[test]
    fn palette_hits_use_the_whole_row_like_the_python() {
        let mut menu = menu(vec![("theme_preview", "palette")]);
        let layout = menu.layout();
        let row = layout.get("theme_preview").copied().expect("paleta");
        let hit = menu
            .hit(40.0, row.y0 + 60.0, 2.0)
            .expect("muestra de paleta");
        assert_eq!(hit.kind, MenuHitKind::Palette);
        assert_eq!(hit.key, "theme");
        assert_eq!(hit.value, Some(SettingValue::Text("light".into())));
        assert!((hit.rect.y0 - (row.y0 + 10.0)).abs() < 1e-9);
    }

    #[test]
    fn action_buttons_respect_the_testing_state() {
        let mut menu = menu(vec![("test_voice", "action"), ("repeat_last", "action")]);
        let layout = menu.layout();
        let test = menu.test_rect(&layout, 0.0);
        let hit = menu
            .hit(test.center().0, test.center().1, 2.0)
            .expect("probar");
        assert_eq!(hit.kind, MenuHitKind::Action);
        assert!(!hit.disabled);
        menu.set_testing(true, Some(2.0));
        menu.update(3.0);
        let layout = menu.layout();
        let test = menu.test_rect(&layout, 0.0);
        assert!(menu.hit(test.center().0, test.center().1, 3.0).is_none());
        let disabled = menu
            .hit_including(test.center().0, test.center().1, 3.0, true)
            .expect("probar deshabilitado");
        assert!(disabled.disabled);
        let layout = menu.layout();
        let repeat = menu.action_rect(&layout, "repeat_last", 0.0);
        let hit = menu
            .hit(repeat.center().0, repeat.center().1, 3.0)
            .expect("repetir");
        assert!(!hit.disabled);
    }

    #[test]
    fn the_save_button_needs_changes_unless_requested() {
        let mut menu = menu(vec![("repeat_last", "action")]);
        let (x, y) = menu.save_rect(&menu.layout()).center();
        assert!(menu.hit(x, y, 2.0).is_none());
        let disabled = menu
            .hit_including(x, y, 2.0, true)
            .expect("guardar deshabilitado");
        assert_eq!(disabled.kind, MenuHitKind::Save);
        assert!(disabled.disabled);
        menu.settings
            .set("speed", &SettingValue::Float(1.2))
            .expect("clave válida");
        let enabled = menu.hit(x, y, 2.0).expect("guardar");
        assert_eq!(enabled.kind, MenuHitKind::Save);
        assert!(!enabled.disabled);
    }

    #[test]
    fn an_open_selector_answers_before_its_row() {
        let mut menu = menu(vec![("theme", "select")]);
        menu.toggle_selector("theme", 2.0);
        menu.update(2.5);
        let layout = menu.layout();
        let geometry = menu
            .selector_geometry(&layout, 0.0, None)
            .expect("selector abierto");
        let option = menu
            .hit(GROUP_X0 + 5.0, geometry.top + 10.0, 2.5)
            .expect("opción");
        assert_eq!(option.kind, MenuHitKind::Option);
        assert_eq!(option.key, "theme");
        assert_eq!(option.index, Some(0));
        assert_eq!(option.value, Some(SettingValue::Text("auto".into())));
    }

    #[test]
    fn outside_the_window_there_is_no_hit() {
        let mut menu = menu(vec![("repeat_last", "action")]);
        assert!(menu.hit(100.0, 10.0, 2.0).is_none());
        menu.close(2.0);
        assert!(menu.hit(100.0, 100.0, 2.0).is_none());
    }

    #[test]
    fn row_rects_match_the_python_oracle() {
        let mut menu = menu(vec![
            ("test_voice", "action"),
            ("repeat_last", "action"),
            ("speed", "slider"),
            ("input_meter", "meter"),
            ("smart_turn", "toggle"),
            ("api_key", "text"),
            ("theme_preview", "palette"),
            ("input_device_index", "select"),
        ]);
        menu.settings
            .set("api_key", &SettingValue::Text("sk-prueba".into()))
            .expect("clave válida");
        let layout = menu.layout();
        let hit = |menu: &mut Menu, key: &str, x: f64, y: f64| {
            menu.hit_including(x, y, 2.0, true)
                .unwrap_or_else(|| panic!("sin impacto en {key}"))
                .rect
        };
        assert_eq!(
            hit(&mut menu, "speed", 51.0, 198.0 + 38.0),
            Rect::new(19.0, 198.0, 301.0, 250.0)
        );
        assert_eq!(
            hit(&mut menu, "input_meter", 51.0, 250.0 + 38.0),
            Rect::new(19.0, 250.0, 301.0, 302.0)
        );
        assert_eq!(
            hit(&mut menu, "smart_turn", 100.0, 324.0),
            Rect::new(19.0, 302.0, 301.0, 346.0)
        );
        assert_eq!(
            hit(&mut menu, "api_key", 100.0, 356.0),
            Rect::new(19.0, 346.0, 301.0, 398.0)
        );
        assert_eq!(
            hit(&mut menu, "api_key", 278.0, 383.0),
            Rect::new(265.0, 370.0, 291.0, 396.0)
        );
        assert_eq!(
            hit(&mut menu, "input_device_index", 100.0, 488.0),
            Rect::new(19.0, 466.0, 301.0, 510.0)
        );
        let pills = menu.palette_pills(398.0);
        assert!((pills[0].2.x1 - 110.333_333_333_333_33).abs() < 1e-9);
        let layout_row = layout.get("theme_preview").copied().expect("paleta");
        assert_eq!(layout_row.y0, 398.0);
    }
}
