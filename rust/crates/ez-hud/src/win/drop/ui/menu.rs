//! Tarjeta de ajustes: menú, barra lateral, geometría y reparto del puntero
//! (`drop.py 1664-2235`).

use std::sync::Arc;

use ez_core::metrics::monotonic;
use parking_lot::Mutex;

use crate::model::settings::MenuSettings;
use crate::paint::settings_window as sw;
use crate::win::menu_panel::LiveMenuFactory;
use crate::win::monitor;
use crate::win::settings_window::SettingsWindow;

use super::traits::{MenuCallbacks, MenuFactory, MenuPanel};
use super::DropUI;

impl DropUI {
    /// Menú actual, si ya se creó.
    pub fn menu_panel(&self) -> Option<Arc<Mutex<dyn MenuPanel>>> {
        self.menu.lock().clone()
    }

    /// Ejecuta una llamada corta al menú sin retener su lock.
    pub(super) fn menu_call<R>(&self, call: impl FnOnce(&mut dyn MenuPanel) -> R) -> Option<R> {
        let menu = self.menu_panel()?;
        let mut guard = menu.lock();
        let result = call(&mut *guard);
        drop(guard);
        Some(result)
    }

    /// `_menu_open`.
    pub fn menu_open(&self) -> bool {
        self.bar.lock().menu_open
    }

    /// `menu.set_theme(self._theme_name())`, con el menú tolerante a fallos.
    pub(super) fn menu_set_theme(&self) {
        let theme = self.theme_name();
        let _ = self.menu_call(|menu| menu.set_theme(&theme));
    }

    /// `open_menu`.
    pub fn open_menu(&self) {
        let asking = self.bar.lock().asking;
        if !asking && !self.menu_open() {
            self.toggle_menu();
        }
    }

    /// `toggle_menu`: abre o pliega la tarjeta de ajustes.
    pub fn toggle_menu(&self) {
        if self.bar.lock().asking {
            return;
        }
        if self.menu_open() {
            self.close_menu();
            return;
        }
        let provider = self.menu_settings_provider.lock().clone();
        let Some(settings) = provider.and_then(|provider| provider()) else {
            return;
        };
        let Some(menu) = self.ensure_menu(settings) else {
            return;
        };
        if self.window().is_none() {
            return;
        }
        let now = monotonic();
        let theme = self.theme_name();
        let scale = self.scale();
        let reduced = self.bar.lock().reduced;
        let room = self.settings_room();
        let (logical_w, logical_h) = self.card_logical_size();
        let (content_w, viewport_h) = sw::content_size(logical_w, logical_h);
        let title = sw::section_title(&self.section.lock());
        let groups = sw::section_groups(&self.section.lock());
        {
            let mut menu = menu.lock();
            menu.set_theme(&theme);
            menu.set_scale(scale);
            menu.set_reduced_motion(reduced);
            menu.set_available_height(room);
            menu.set_width(content_w);
            menu.set_max_height(viewport_h);
            menu.set_groups(&groups, &title, now);
            menu.open(now);
        }
        // La tarjeta nace con el puntero en la barra, no en la lista de secciones.
        *self.sidebar_hover.lock() = None;
        self.bar.lock().menu_open = true;
        self.place_card(true);
    }

    /// `_close_menu`: pide al menú que se pliegue (la ventana se retira al
    /// terminar la animación).
    pub(super) fn close_menu(&self) {
        let _ = self.menu_call(|menu| menu.close(monotonic()));
    }

    /// `_ensure_menu`: crea el menú con los ajustes y las llamadas de la app.
    fn ensure_menu(&self, settings: MenuSettings) -> Option<Arc<Mutex<dyn MenuPanel>>> {
        if let Some(menu) = self.menu_panel() {
            return Some(menu);
        }
        // DECISION: `drop.py` construye `Menu` directamente; aquí, si nadie
        // instala una fábrica (el caso de la app y de la demo), se usa la real.
        let factory: Arc<dyn MenuFactory> = self
            .menu_factory
            .clone()
            .unwrap_or_else(|| Arc::new(LiveMenuFactory));
        let scale = self.scale();
        let theme = self.theme_name();
        let screen_h = {
            let monitor_rect = monitor::monitor_rect(None, None);
            (monitor_rect.3 - monitor_rect.1) as f64 / scale
        };
        let callbacks = MenuCallbacks::for_ui(&self.self_weak);
        let menu = factory.create(settings, scale, &theme, screen_h, callbacks)?;
        menu.lock()
            .set_palette_overrides(crate::theme::card_overrides);
        *self.menu.lock() = Some(Arc::clone(&menu));
        Some(menu)
    }

    /// `_settings_room`: alto lógico máximo de la ventana de ajustes.
    fn settings_room(&self) -> f64 {
        let work = monitor::work_area();
        (240.0f64).max((work.3 - work.1) as f64 / self.scale())
    }

    /// `_card_logical_size`: tamaño guardado (acotado) o el de fábrica.
    pub(super) fn card_logical_size(&self) -> (f64, f64) {
        let width = super::config_number(self.config.as_ref(), "settings_w")
            .filter(|value| *value > 0.0)
            .unwrap_or(sw::DEFAULT_W);
        let height = super::config_number(self.config.as_ref(), "settings_h")
            .filter(|value| *value > 0.0)
            .unwrap_or(sw::DEFAULT_H);
        sw::clamp_size(width, height, monitor::work_area(), self.scale())
    }

    /// `_card_position` + `_card_logical_size` en una ventana de geometría.
    pub(super) fn card_geometry(&self) -> SettingsWindow {
        let (logical_w, logical_h) = self.card_logical_size();
        let x = super::config_integer(self.config.as_ref(), "settings_x");
        let y = super::config_integer(self.config.as_ref(), "settings_y");
        let position = x.zip(y).map(|(x, y)| (x as i32, y as i32));
        SettingsWindow::new(
            monitor::work_area(),
            self.scale(),
            Some((logical_w, logical_h)),
            position,
        )
    }
}
