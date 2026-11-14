//! Port of `src/ezagent/voice/hud/menu.py 1343-1417` (F5-08).
//!
//! Selectores en línea: opciones, apertura y cierre del desplegable y geometría
//! de su lista. Es modelo puro; la pintura es de F5-10.

use std::collections::BTreeMap;

use crate::anim::{EASE_IN, EASE_OUT};
use crate::layout;
use crate::model::settings::SelectOption;

use super::geometry::{Menu, LIST_ITEM_H, LIST_MAX_VISIBLE, LIST_VIEW_H};

/// Apertura de la lista de un selector.
pub const SELECTOR_OPEN_MS: f64 = 200.0;
/// Cierre de la lista de un selector.
pub const SELECTOR_CLOSE_MS: f64 = 140.0;
/// Fundido de cada opción al abrir.
pub const SELECTOR_ITEM_FADE_MS: f64 = 120.0;
/// Retardo entre opciones al abrir.
pub const SELECTOR_ITEM_STAGGER_MS: f64 = 20.0;
/// Retardo antes de que empiecen a entrar las opciones.
pub const SELECTOR_START_DELAY_MS: f64 = 20.0;
/// Giro del chevron del selector.
pub const CHEVRON_MS: f64 = 180.0;

/// `(y_lista, alto_visible, opciones, scroll_interno)` de
/// `Menu._selector_geometry`.
#[derive(Clone, Debug)]
pub struct SelectorGeometry {
    pub top: f64,
    pub visible: f64,
    pub options: Vec<SelectOption>,
    pub list_scroll: f64,
}

impl Menu {
    /// Opciones de un selector; una clave desconocida devuelve una lista vacía
    /// (el Python atrapaba cualquier `Exception`).
    pub fn selector_options(&self, key: &str) -> Vec<SelectOption> {
        self.settings.select_options(key).unwrap_or_default()
    }

    /// Abre el selector `key` o cierra el que ya está abierto.
    pub fn toggle_selector(&mut self, key: &str, now: f64) {
        if !self.settings.selector_enabled(key) {
            return;
        }
        if self.selector_key.as_deref() == Some(key) && !self.selector_closing {
            self.close_selector(now);
            return;
        }
        if let Some(old) = self.selector_key.clone() {
            if old != key {
                if let Some(tween) = self.chevron.get_mut(&old) {
                    tween.to(0.0, now, CHEVRON_MS, EASE_OUT);
                }
            }
        }
        let options = self.selector_options(key);
        if options.is_empty() {
            return;
        }
        self.selector_key = Some(key.to_string());
        self.selector_closing = false;
        self.selector_item_start = now + SELECTOR_START_DELAY_MS / 1000.0;
        let target = options.len().min(LIST_MAX_VISIBLE) as f64 * LIST_ITEM_H;
        self.selector_extra
            .to(target, now, SELECTOR_OPEN_MS, EASE_OUT);
        if let Some(tween) = self.chevron.get_mut(key) {
            tween.to(1.0, now, CHEVRON_MS, EASE_OUT);
        }
        self.list_scroll = 0.0;
        self.list_scroll_target = 0.0;
        if let Some((x, y)) = self.pointer {
            self.sync_hover(x, y, now);
        }
    }

    /// Pliega la lista abierta; `advance` la olvida al terminar el tween.
    pub fn close_selector(&mut self, now: f64) {
        let Some(key) = self.selector_key.clone() else {
            return;
        };
        self.selector_closing = true;
        if let Some(tween) = self.chevron.get_mut(&key) {
            tween.to(0.0, now, CHEVRON_MS, EASE_OUT);
        }
        self.selector_extra.to(0.0, now, SELECTOR_CLOSE_MS, EASE_IN);
        self.list_scroll = 0.0;
        self.list_scroll_target = 0.0;
        self.hover_option = None;
    }

    /// Geometría de la lista abierta (o de `key`, si se indica).
    pub fn selector_geometry(
        &self,
        layout: &BTreeMap<String, layout::Rect>,
        scroll: f64,
        key: Option<&str>,
    ) -> Option<SelectorGeometry> {
        let target_key = key.or(self.selector_key.as_deref())?;
        let options = self.selector_options(target_key);
        if options.is_empty() {
            return None;
        }
        // DECISION: una clave sin fila en el layout devuelve None en vez del
        // `KeyError` del Python; evita un pánico si el selector quedó huérfano.
        let row = *layout.get(target_key)?;
        let top = row.y0 - scroll + f64::from(layout::row_height("select")?);
        let extra = self.selector_extra.value.max(0.0);
        let visible = extra.min(options.len() as f64 * LIST_ITEM_H).max(0.0);
        let list_scroll = self
            .list_scroll
            .min((options.len() as f64 * LIST_ITEM_H - LIST_VIEW_H).max(0.0))
            .max(0.0);
        Some(SelectorGeometry {
            top,
            visible,
            options,
            list_scroll,
        })
    }

    /// Alfa de entrada de una opción durante el escalonado.
    pub fn selector_item_alpha(&self, index: usize, now: f64) -> f64 {
        if self.selector_closing {
            return 1.0;
        }
        let elapsed =
            (now - self.selector_item_start) * 1000.0 - index as f64 * SELECTOR_ITEM_STAGGER_MS;
        let k = (elapsed / SELECTOR_ITEM_FADE_MS).clamp(0.0, 1.0);
        EASE_OUT.apply(k)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::settings::MenuSettings;

    use super::super::geometry::{MenuCallbacks, MenuGroup, MenuOptions};
    use super::*;

    fn menu() -> Menu {
        let mut menu = Menu::new(
            MenuSettings::default(),
            MenuOptions {
                screen_h: 1080.0,
                ..MenuOptions::default()
            },
            MenuCallbacks::default(),
        )
        .expect("opciones válidas");
        menu.set_max_height(Some(2000.0));
        menu.set_groups(
            vec![
                (
                    "prueba".to_string(),
                    vec![("theme".to_string(), "select".to_string())],
                ),
                (
                    "voz".to_string(),
                    vec![
                        ("voice".to_string(), "select".to_string()),
                        ("speed".to_string(), "slider".to_string()),
                    ],
                ),
            ],
            Some("Prueba"),
            Some(0.0),
        );
        menu.open(0.0);
        menu.height(2.0);
        menu
    }

    fn group(rows: Vec<(&str, &str)>) -> MenuGroup {
        (
            "prueba".to_string(),
            rows.into_iter()
                .map(|(key, kind)| (key.to_string(), kind.to_string()))
                .collect(),
        )
    }

    #[test]
    fn toggle_opens_and_closes_the_same_selector() {
        let mut menu = menu();
        menu.toggle_selector("theme", 2.0);
        assert_eq!(menu.selector_key.as_deref(), Some("theme"));
        assert!(!menu.selector_closing);
        assert!(menu.selector_extra.active());
        assert!(menu.chevron.get("theme").is_some_and(|t| t.active()));
        menu.toggle_selector("theme", 2.1);
        assert!(menu.selector_closing);
        assert!(menu.selector_extra.active());
        menu.update(3.0);
        assert!(menu.selector_key.is_none());
        assert!(!menu.selector_closing);
    }

    #[test]
    fn a_selector_without_options_does_not_open() {
        let mut menu = menu();
        menu.toggle_selector("nope", 2.0);
        assert!(menu.selector_key.is_none());
        assert_eq!(menu.selector_options("nope"), Vec::new());
    }

    #[test]
    fn geometry_clamps_the_visible_height_and_the_inner_scroll() {
        let mut menu = menu();
        menu.toggle_selector("theme", 2.0);
        menu.update(2.5);
        let layout = menu.layout();
        let row = menu.layout().get("theme").copied().expect("fila del tema");
        let geometry = menu
            .selector_geometry(&layout, 0.0, None)
            .expect("selector abierto");
        assert_eq!(geometry.options.len(), 4);
        assert!((geometry.visible - 128.0).abs() < 1e-9);
        assert!((geometry.top - (row.y0 + 44.0)).abs() < 1e-9);
        menu.list_scroll = 400.0;
        let clamped = menu
            .selector_geometry(&layout, 0.0, None)
            .expect("selector abierto");
        assert_eq!(clamped.list_scroll, 0.0);
        let geometry = menu
            .selector_geometry(&layout, 12.0, Some("theme"))
            .expect("selector explícito");
        assert!((geometry.top - (row.y0 + 44.0 - 12.0)).abs() < 1e-9);
    }

    #[test]
    fn option_alpha_is_staggered_and_instant_while_closing() {
        let mut menu = menu();
        menu.toggle_selector("theme", 2.0);
        assert_eq!(menu.selector_item_alpha(0, 2.01), 0.0);
        let second = menu.selector_item_alpha(2, 2.01);
        assert_eq!(second, 0.0);
        let first = menu.selector_item_alpha(0, 2.12);
        assert!(first > 0.0 && first < 1.0);
        assert!(menu.selector_item_alpha(3, 2.15) > menu.selector_item_alpha(3, 2.10));
        menu.close_selector(2.4);
        assert_eq!(menu.selector_item_alpha(3, 2.4), 1.0);
    }

    #[test]
    fn changing_the_selector_turns_the_old_chevron_down() {
        let mut menu = menu();
        menu.set_groups(
            vec![
                group(vec![("theme", "select")]),
                group(vec![("voice", "select")]),
            ],
            Some("Prueba"),
            Some(0.0),
        );
        menu.toggle_selector("theme", 2.0);
        menu.toggle_selector("voice", 2.1);
        assert_eq!(menu.selector_key.as_deref(), Some("voice"));
        let theme = menu.chevron.get("theme").expect("chevron del tema");
        assert!(theme.active());
        assert_eq!(theme.value, 0.0);
    }
}
