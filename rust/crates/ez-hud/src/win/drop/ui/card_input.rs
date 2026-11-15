//! Tarjeta de ajustes: geometría, arrastre, redimensionado y puntero
//! (`drop.py 1741-2235`).

use std::sync::Arc;

use ez_core::metrics::monotonic;

use crate::paint::settings_window as sw;
use crate::win::monitor;
use crate::win::settings_window::SettingsWindow;

use super::{CardSlot, CardText, ConfigValue, DropUI, PointerPhase};

impl DropUI {
    /// `clamp_card`: mantiene una franja de la tarjeta a la vista.
    pub fn clamp_card(&self, x: i32, y: i32) -> (i32, i32) {
        let size = self.card_window().map_or((0, 0), |window| window.size());
        sw::clamp_position(x, y, size.0, size.1, monitor::work_area(), self.scale())
    }

    /// `_save_card_geometry`: guarda posición y tamaño al soltar.
    pub(super) fn save_card_geometry(&self) {
        let Some(window) = self.card_window() else {
            return;
        };
        let (x, y) = window.position();
        let (width, height) = window.size();
        let scale = self.scale();
        let values = [
            ("settings_x", ConfigValue::Int(i64::from(x))),
            ("settings_y", ConfigValue::Int(i64::from(y))),
            (
                "settings_w",
                ConfigValue::Int((width as f64 / scale).round_ties_even() as i64),
            ),
            (
                "settings_h",
                ConfigValue::Int((height as f64 / scale).round_ties_even() as i64),
            ),
        ];
        if let Some(config) = &self.config {
            for (key, value) in values {
                config.remember(key, value.clone());
                config.save(key, value);
            }
        }
    }

    /// `card_moved`: un arrastre no cuenta como primer clic de la cabecera.
    pub fn card_moved(&self) {
        *self.header_click.lock() = None;
        {
            let mut card = self.card.lock();
            if let Some(slot) = card.as_mut() {
                let (x, y) = slot.window.position();
                let (width, height) = slot.window.size();
                slot.geometry = SettingsWindow::new(
                    monitor::work_area(),
                    self.scale(),
                    Some((width as f64 / self.scale(), height as f64 / self.scale())),
                    Some((x, y)),
                );
            }
        }
        self.save_card_geometry();
    }

    /// `_place_card`: abre o recoloca la tarjeta donde se dejó.
    pub(super) fn place_card(&self, create: bool) {
        if self.menu_panel().is_none() || self.window().is_none() {
            return;
        }
        let geometry = self.card_geometry();
        let rect = geometry.rect();
        let mut slot = self.card.lock();
        if slot.is_none() {
            if !create {
                return;
            }
            let window = self
                .factory
                .card(self.self_weak.clone(), rect.x, rect.y, rect.w, rect.h);
            let _ = window.start();
            *slot = Some(CardSlot {
                window: Arc::clone(&window),
                geometry,
            });
            drop(slot);
            window.show();
            return;
        }
        let slot = slot.as_mut().expect("la tarjeta existe");
        slot.geometry = geometry;
        slot.window.resize(rect.w, rect.h);
        slot.window.set_pos(rect.x, rect.y);
        slot.window.show();
    }

    /// `_apply_card_rect`: nuevo rectángulo físico y menú adaptado.
    fn apply_card_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        let Some(window) = self.card_window() else {
            return;
        };
        {
            let mut card = self.card.lock();
            if let Some(slot) = card.as_mut() {
                slot.geometry.apply(x, y, width, height);
            }
        }
        window.resize(width, height);
        window.set_pos(x, y);
        let scale = self.scale();
        let (content_w, viewport_h) = sw::content_size(width as f64 / scale, height as f64 / scale);
        let _ = self.menu_call(|menu| menu.resize(content_w, viewport_h, monotonic()));
    }

    /// `reset_card`: doble clic en la cabecera.
    pub fn reset_card(&self) {
        if self.card_window().is_none() {
            return;
        }
        let (logical_w, logical_h) = sw::clamp_size(
            sw::DEFAULT_W,
            sw::DEFAULT_H,
            monitor::work_area(),
            self.scale(),
        );
        let scale = self.scale();
        let width = (logical_w * scale).round_ties_even() as i32;
        let height = (logical_h * scale).round_ties_even() as i32;
        let (x, y) = sw::centered(monitor::work_area(), width, height);
        self.apply_card_rect(x, y, width, height);
        self.save_card_geometry();
    }

    /// `select_section`: cambia la sección visible de la ventana.
    pub fn select_section(&self, name: &str) {
        if self.menu_panel().is_none() || name == self.section.lock().as_str() {
            return;
        }
        *self.section.lock() = name.to_string();
        let title = sw::section_title(name);
        let groups = sw::section_groups(name);
        let _ = self.menu_call(|menu| menu.set_groups(&groups, &title, monotonic()));
    }

    /// `_close_card_now`: cierra el menú y retira la tarjeta sin animación.
    pub(super) fn close_card_now(&self) {
        if self.menu_open() {
            let _ = self.menu_call(|menu| menu.close(monotonic()));
        }
        self.bar.lock().menu_open = false;
        let card = self.card.lock().take();
        if let Some(card) = card {
            card.window.stop();
        }
    }

    /// `_menu_closed`: el menú terminó de plegarse.
    pub fn menu_closed(&self) {
        self.bar.lock().menu_open = false;
        let card = self.card.lock().take();
        if let Some(card) = card {
            card.window.stop();
        }
    }

    /// `_menu_text_edit`: habilita el teclado de la tarjeta.
    pub fn menu_text_edit(&self, active: bool) {
        if let Some(window) = self.card_window() {
            window.set_keyboard_enabled(active);
        }
    }

    /// `_hotkey_capture` hacia la app.
    pub fn hotkey_capture(&self, active: bool) {
        self.host.hotkey_capture(active);
    }

    /// `_copy_text`: portapapeles desde la tarjeta, sin registrarlo.
    pub fn copy_text(&self, text: &str) -> bool {
        self.card_window()
            .is_some_and(|window| window.copy_text(text))
    }

    /// `_hide_from_menu`: el botón «–» cierra solo el menú; la barra sigue.
    ///
    /// // DECISION: `Menu::activate_hit` ya cerró el menú antes de avisar, así
    /// // que al llegar desde ese callback el lock está tomado por el mismo
    /// // hilo; `try_lock` reproduce el `close` idempotente del Python sin
    /// // bloquearse.
    pub fn hide_from_menu(&self) {
        let Some(panel) = self.menu_panel() else {
            return;
        };
        if let Some(mut menu) = panel.try_lock() {
            menu.close(monotonic());
        };
    }

    /// `_menu_action` hacia la app.
    pub fn menu_action(&self, action: &str) {
        self.host.menu_action(action);
    }

    /// `set_testing`: el menú muestra el modo de prueba de voz.
    pub fn set_testing(&self, testing: bool) {
        let _ = self.menu_call(|menu| menu.set_testing(testing));
    }

    /// `set_save_result`: resultado del guardado para el siguiente cuadro.
    pub fn set_save_result(&self, ok: bool) {
        *self.pending_save.lock() = Some(ok);
    }

    /// `update_llm_models`: lista real de modelos para el menú abierto.
    pub fn update_llm_models(&self, provider: &str, models: &[String]) {
        let _ = self.menu_call(|menu| menu.set_llm_models(provider, models));
    }

    /// `refresh_menu_settings`: recarga los ajustes del menú.
    pub fn refresh_menu_settings(&self) {
        let Some(provider) = self.menu_settings_provider.lock().clone() else {
            return;
        };
        let Some(settings) = provider() else {
            return;
        };
        let _ = self.menu_call(|menu| menu.set_settings(settings));
    }

    /// `_card_point`: punto en coordenadas del menú y si cae dentro.
    fn card_point(&self, x: i32, y: i32) -> (f64, f64, bool) {
        if self.card_window().is_none() {
            return (0.0, 0.0, false);
        }
        let scale = self.scale();
        let lx = x as f64 / scale;
        let ly = y as f64 / scale;
        let mx = lx - sw::SIDEBAR_W;
        let height = self
            .menu_call(|menu| menu.height(monotonic()))
            .unwrap_or(0.0);
        let inside = mx >= 0.0 && 0.0 <= ly && ly <= height;
        (mx, ly + f64::from(crate::tokens::H_ISLAND), inside)
    }

    /// `_sidebar_item`: sección bajo el punto.
    fn sidebar_item(&self, x: i32, y: i32) -> Option<&'static str> {
        sw::item_at(x as f64 / self.scale(), y as f64 / self.scale())
    }

    /// `card_pointer`: bordes, barra lateral, menú o arrastre.
    pub fn card_pointer(&self, phase: PointerPhase, x: i32, y: i32) -> bool {
        if self.card_window().is_none() || self.menu_panel().is_none() {
            return false;
        }
        let resizing = *self.resizing.lock();
        if let Some(edges) = resizing {
            return self.resize_pointer(phase, edges);
        }
        match phase {
            PointerPhase::Move => {
                let section = self.sidebar_item(x, y);
                *self.sidebar_hover.lock() = section.map(str::to_string);
                let (mx, my, _) = self.card_point(x, y);
                let _ = self.menu_call(|menu| menu.pointer_move(mx, my, monotonic()));
                true
            }
            PointerPhase::Up => {
                let (mx, my, _) = self.card_point(x, y);
                let _ = self.menu_call(|menu| menu.pointer_up(mx, my, monotonic()));
                true
            }
            PointerPhase::Down => self.card_pointer_down(x, y),
        }
    }

    /// Rama `down` de `card_pointer`: bordes, barra lateral, menú o cabecera.
    fn card_pointer_down(&self, x: i32, y: i32) -> bool {
        let scale = self.scale();
        let now = monotonic();
        let (lx, ly) = (x as f64 / scale, y as f64 / scale);
        let (width, height) = self.card_window().map_or((0, 0), |window| window.size());
        let section = self.sidebar_item(x, y);
        let (mx, my, inside) = self.card_point(x, y);
        let hit = inside
            && self
                .menu_call(|menu| menu.hit(mx, my, now))
                .unwrap_or(false);
        if !hit {
            if let Some(edges) =
                sw::resize_edges(lx, ly, width as f64 / scale, height as f64 / scale)
            {
                self.begin_resize(edges);
                return true;
            }
        }
        if let Some(section) = section {
            self.select_section(section);
            return true;
        }
        let editing = self.menu_call(|menu| menu.editing_key()).unwrap_or(false);
        if hit || editing {
            let _ = self.menu_call(|menu| menu.pointer_down(mx, my, now));
            return true;
        }
        if !sw::in_header(lx, ly) {
            return false;
        }
        let previous = *self.header_click.lock();
        if previous
            .is_some_and(|previous| now - previous < crate::win::settings_window::DOUBLE_CLICK)
        {
            *self.header_click.lock() = None;
            self.reset_card();
            return true;
        }
        *self.header_click.lock() = Some(now);
        false
    }

    /// `_begin_resize`: guarda bordes y cursor de pantalla.
    fn begin_resize(&self, edges: sw::Edges) {
        if self.card_window().is_none() {
            return;
        }
        let (sx, sy) = crate::win::layered::screen_cursor();
        let mut card = self.card.lock();
        if let Some(slot) = card.as_mut() {
            slot.geometry.begin_resize(edges, sx, sy);
        }
        *self.resizing.lock() = Some(edges);
    }

    /// `_resize_pointer`: mueve el borde agarrado hasta el puntero.
    fn resize_pointer(&self, phase: PointerPhase, _edges: sw::Edges) -> bool {
        match phase {
            PointerPhase::Down => true,
            PointerPhase::Up => {
                *self.resizing.lock() = None;
                {
                    let mut card = self.card.lock();
                    if let Some(slot) = card.as_mut() {
                        slot.geometry.end_resize();
                    }
                }
                self.save_card_geometry();
                true
            }
            PointerPhase::Move => {
                let (sx, sy) = crate::win::layered::screen_cursor();
                let rect = {
                    let mut card = self.card.lock();
                    card.as_mut().and_then(|slot| {
                        slot.geometry
                            .resize_to(sx, sy)
                            .then(|| slot.geometry.rect())
                    })
                };
                if let Some(rect) = rect {
                    self.apply_card_rect(rect.x, rect.y, rect.w, rect.h);
                }
                true
            }
        }
    }

    /// `card_cursor`: cursor de redimensión si el punto está en un borde.
    pub fn card_cursor(&self, x: i32, y: i32) -> i32 {
        if self.card_window().is_none() || self.menu_panel().is_none() {
            return 0;
        }
        let resizing = *self.resizing.lock();
        if let Some(edges) = resizing {
            return sw::cursor_for(Some(edges));
        }
        let (mx, my, inside) = self.card_point(x, y);
        if inside
            && self
                .menu_call(|menu| menu.hit(mx, my, monotonic()))
                .unwrap_or(false)
        {
            return 0;
        }
        let scale = self.scale();
        let (width, height) = self.card_window().map_or((0, 0), |window| window.size());
        sw::cursor_for(sw::resize_edges(
            x as f64 / scale,
            y as f64 / scale,
            width as f64 / scale,
            height as f64 / scale,
        ))
    }

    /// `card_wheel`.
    pub fn card_wheel(&self, x: i32, y: i32, delta: i32) -> bool {
        if self.menu_panel().is_none() {
            return false;
        }
        let (mx, my, inside) = self.card_point(x, y);
        if !inside {
            return false;
        }
        self.menu_call(|menu| menu.wheel(mx, my, delta, monotonic()))
            .unwrap_or(false)
    }

    /// `card_hand`: mano sobre la barra lateral o un control del menú.
    pub fn card_hand(&self, x: i32, y: i32) -> bool {
        if self.menu_panel().is_none() {
            return false;
        }
        if self.sidebar_item(x, y).is_some() {
            return true;
        }
        let (mx, my, inside) = self.card_point(x, y);
        inside
            && self
                .menu_call(|menu| menu.hit(mx, my, monotonic()))
                .unwrap_or(false)
    }

    /// `card_text`: teclado, carácter o pegado para el menú.
    pub fn card_text(&self, text: &CardText) -> bool {
        if !self.menu_open() {
            return false;
        }
        match text {
            CardText::Key(vk, ctrl) => self
                .menu_call(|menu| menu.text_key(*vk, *ctrl))
                .unwrap_or(false),
            CardText::Char(character) => self
                .menu_call(|menu| menu.text_input(*character))
                .unwrap_or(false),
            CardText::Paste(value) => self
                .menu_call(|menu| menu.paste_text(value))
                .unwrap_or(false),
        }
    }

    /// `card_focus_lost`: termina la edición en línea.
    pub fn card_focus_lost(&self) {
        let _ = self.menu_call(|menu| menu.finish_text_edit(true));
    }
}
