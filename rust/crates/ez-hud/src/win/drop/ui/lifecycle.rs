//! Ciclo de vida de la barra y estado que publica la app (`drop.py 1202-1298`).

use std::sync::{Arc, Weak};

use ez_core::metrics::monotonic;

use crate::paint::peek::IconPainter;

use super::traits::{ChatPanel, HudWindow, WindowFactory};
use super::{config_text, DropUI, DropUiOptions};
use super::{DropHost, MenuSettingsProvider};

impl DropUI {
    /// `show`: hace visible la barra y le pone el estado de la app.
    pub fn show(&self, state: &str) {
        self.bar.lock().visible = true;
        self.ui.set_app_state(state, None);
        self.ui.set_visible(true, None);
        if self.ensure_window() {
            if let Some(window) = self.window() {
                window.show();
            }
        }
    }

    /// `hide`: oculta barra, vista rápida, chat y tarjeta.
    pub fn hide(&self) {
        self.bar.lock().visible = false;
        self.peek.hide_now();
        self.chat.read().clone().close_now();
        self.close_card_now();
        if let Some(window) = self.window() {
            window.hide();
        }
    }

    /// `close`: detiene la vista rápida y las ventanas.
    pub fn close(&self) {
        self.peek.stop();
        self.chat.read().clone().close_now();
        self.close_card_now();
        if let Some(window) = self.window() {
            window.stop();
        }
    }

    /// `set_state`: estado de la app (`idle`, `listening`…).
    pub fn set_state(&self, state: &str) {
        self.bar.lock().state = state.to_string();
        self.ui.set_app_state(state, None);
    }

    /// `set_muted`.
    pub fn set_muted(&self, muted: bool) {
        self.bar.lock().muted = muted;
        self.ui.set_muted(muted, None);
        self.chat.read().clone().set_mic_muted(muted);
    }

    /// `set_activity`: foto de la actividad para la órbita y la vista rápida.
    pub fn set_activity(&self, items: Vec<super::ActivityItem>) {
        *self.activity.write() = items.clone();
        self.peek.refresh(items);
    }

    /// `show_activity`: abre la vista rápida un momento.
    pub fn show_activity(&self) {
        if self.activity.read().is_empty() {
            return;
        }
        let blocked = {
            let bar = self.bar.lock();
            bar.menu_open || bar.chat_open || bar.asking
        };
        if !blocked {
            self.peek.reveal(monotonic());
        }
    }

    /// `announce_activity`: muestra un desenlace nuevo un momento.
    pub fn announce_activity(&self, item: &super::ActivityItem) {
        let blocked = {
            let bar = self.bar.lock();
            bar.menu_open || bar.chat_open || bar.asking
        };
        if !blocked {
            self.peek.toast(item, monotonic());
        }
    }

    /// `set_in_call`.
    pub fn set_in_call(&self, in_call: bool) {
        self.bar.lock().in_call = in_call;
        self.ui.set_in_call(in_call, None);
    }

    /// `set_mic_level`: nivel del medidor de entrada.
    pub fn set_mic_level(&self, level: f64) {
        let level = if level.is_finite() {
            level.max(0.0)
        } else {
            0.0
        };
        self.bar.lock().mic_level = level;
    }

    /// `draw`: la app entrega el RMS del micrófono (y, si cambió, el estado).
    ///
    /// // DECISION: `wave_data` del Python no se usa (nunca se leyó); se omite.
    pub fn draw(&self, rms: f64, state: Option<&str>) {
        self.bar.lock().rms = if rms.is_finite() { rms } else { 0.0 };
        if let Some(state) = state {
            self.set_state(state);
        }
    }

    /// `notice`: el aviso luminoso de la barra.
    pub fn notice(&self) {
        self.ui.notice(None);
    }

    /// `set_scale`: cambio de DPI de la pantalla.
    pub fn set_scale(&self, scale: f64) {
        let scale = if scale.is_finite() {
            scale.max(1.0)
        } else {
            1.0
        };
        self.bar.lock().scale = scale;
        if let Some(window) = self.window() {
            let (width, height) = self.size();
            window.resize(width, height);
        }
        if self.menu_panel().is_some() {
            self.menu_call(|menu| menu.set_scale(scale));
            if self.menu_open() {
                self.relayout_card();
            }
        }
    }

    /// `_relayout_card`: tras un DPI nuevo, la tarjeta recupera su tamaño lógico.
    pub(super) fn relayout_card(&self) {
        if self.menu_panel().is_none() {
            return;
        }
        self.place_card(false);
        let (logical_w, logical_h) = self.card_logical_size();
        let (content_w, viewport_h) =
            crate::paint::settings_window::content_size(logical_w, logical_h);
        self.menu_call(|menu| menu.resize(content_w, viewport_h, monotonic()));
    }

    /// `set_chat_panel`: sustituye el panel de chat (F5-14/F5-15).
    pub fn set_chat_panel(&self, panel: Arc<dyn ChatPanel>) {
        *self.chat.write() = panel;
    }

    /// Instala el proveedor de ajustes del menú (`menu_settings_provider`).
    pub fn set_menu_settings_provider(&self, provider: Option<MenuSettingsProvider>) {
        *self.menu_settings_provider.lock() = provider;
    }

    /// Ventana de la tarjeta, si está abierta.
    pub fn card_window(&self) -> Option<Arc<dyn HudWindow>> {
        self.card
            .lock()
            .as_ref()
            .map(|slot| Arc::clone(&slot.window))
    }
}

/// `create_ui`: la barra con la posición guardada, si se guardó con esta
/// misma disposición.
pub fn create_ui(
    host: Arc<dyn DropHost>,
    config: Option<Arc<dyn super::DropConfig>>,
    icons: Option<Arc<dyn IconPainter>>,
) -> Arc<DropUI> {
    let layout = config_text(config.as_ref(), "ui_layout")
        .unwrap_or_default()
        .to_lowercase();
    let layout = if crate::model::drop::LAYOUTS.contains(&layout.as_str()) {
        layout
    } else {
        String::from("pill")
    };
    let same = config_text(config.as_ref(), "drop_layout").as_deref() == Some(layout.as_str());
    let x = super::config_integer(config.as_ref(), "drop_x");
    let y = super::config_integer(config.as_ref(), "drop_y");
    let position = if same {
        x.zip(y).map(|(x, y)| (x as i32, y as i32))
    } else {
        None
    };
    DropUI::new(DropUiOptions {
        position,
        config,
        host: Some(host),
        icons,
        ..DropUiOptions::default()
    })
}

impl Drop for DropUI {
    fn drop(&mut self) {
        // Un `Arc<DropUI>` puede soltarse sin llamar a `close`; las ventanas y la
        // vista rápida deben parar igual (sus `stop` son idempotentes).
        self.close();
    }
}

/// Fábrica de ventanas Win32 (la única implementación real).
pub struct NativeWindowFactory;

impl WindowFactory for NativeWindowFactory {
    fn bar(&self, ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32) -> Arc<dyn HudWindow> {
        crate::win::drop::bar::DropWindow::new(ui, x, y, width, height)
    }

    fn card(
        &self,
        ui: Weak<DropUI>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Arc<dyn HudWindow> {
        crate::win::drop::card::CardWindow::new(ui, x, y, width, height)
    }
}
