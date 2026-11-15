//! Adaptador vivo del menú: [`MenuPanel`] y [`MenuFactory`] sobre el `Menu`
//! real de `model::menu` y la pintura de `paint::menu` (F5-05 dejó el contrato;
//! `drop.py 1664-2235` lo usa con la clase `Menu` de `menu.py`).
//!
//! // DECISION: en Python la barra construye `Menu` directamente
//! // (`drop.py: _ensure_menu`); aquí el contrato pide una fábrica para que las
//! // pruebas usen un doble. [`LiveMenuFactory`] es la única implementación
//! // real y la barra la usa cuando nadie instala otra.
//!
//! `Menu` no es `Sync`: sus callbacks son cajas de `Fn`. El contrato exige
//! `MenuPanel: Send`, así que el menú vive en un `Mutex` y cada llamada del
//! hilo de la tarjeta lo bloquea el tiempo de una operación corta.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use ez_gfx::sdf::Layer;
use parking_lot::Mutex;

use crate::layout::Group;
use crate::model::menu::geometry::{Menu, MenuCallbacks as ModelCallbacks, MenuOptions};
use crate::model::settings::{MenuSettings, OptionInput, OptionSource, SettingValue};
use crate::paint::menu::advanced::Advanced;
use crate::tokens::ColorMap;
use crate::win::drop::{ConfigValue, MenuCallbacks, MenuFactory, MenuPanel};

/// `Menu` de `menu.py` detrás del contrato [`MenuPanel`] de la barra.
pub struct LiveMenuPanel {
    menu: Mutex<Menu>,
}

impl LiveMenuPanel {
    /// `Menu(settings, scale, theme, screen_h, on_*)` con sus callbacks.
    pub fn new(
        settings: MenuSettings,
        scale: f64,
        theme: &str,
        screen_h: f64,
        callbacks: MenuCallbacks,
    ) -> Option<Self> {
        let options = MenuOptions {
            scale,
            theme: theme.to_string(),
            screen_h,
        };
        // `_ensure_menu` atrapa el `ValueError` de la escala y deja `_menu = None`.
        let menu = Menu::new(settings, options, model_callbacks(callbacks)).ok()?;
        Some(Self {
            menu: Mutex::new(menu),
        })
    }

    /// Acceso directo al modelo (solo para las pruebas de esta tarea).
    #[cfg(test)]
    fn model(&self) -> parking_lot::MutexGuard<'_, Menu> {
        self.menu.lock()
    }
}

/// Los callbacks de la barra traducidos a los del modelo (`menu.py`).
fn model_callbacks(callbacks: MenuCallbacks) -> ModelCallbacks {
    let MenuCallbacks {
        live_change,
        save,
        test,
        hide,
        close,
        text_edit,
        action,
        copy,
        hotkey_capture,
    } = callbacks;
    ModelCallbacks {
        live_change: Some(Box::new(move |key, value| {
            live_change(key, config_from_setting(value));
        })),
        save: Some(Box::new(move |values| save(values))),
        test: Some(Box::new(move |values| test(values))),
        hide: Some(Box::new(move || hide())),
        close: Some(Box::new(move || close())),
        text_edit: Some(Box::new(move |active| text_edit(active))),
        action: Some(Box::new(move |key| action(key))),
        copy: Some(Box::new(move |text| copy(text))),
        hotkey_capture: Some(Box::new(move |active| hotkey_capture(active))),
        ..ModelCallbacks::default()
    }
}

/// Valor del modelo → `ConfigValue` de la barra: `_live_setting` del Python
/// reenvía el mismo valor que el menú acaba de guardar.
fn config_from_setting(value: &SettingValue) -> ConfigValue {
    match value {
        SettingValue::Bool(flag) => ConfigValue::Bool(*flag),
        SettingValue::Int(number) => ConfigValue::Int(*number),
        SettingValue::Float(number) => ConfigValue::Float(*number),
        SettingValue::Text(text) => ConfigValue::Text(text.clone()),
        // `None` no es una clave editable; la barra lo lee como texto vacío.
        SettingValue::None => ConfigValue::Text(String::new()),
    }
}

/// `section_groups` → los grupos del modelo (`tuple[str, tuple[...]]`).
fn model_groups(groups: &[Group]) -> Vec<(String, Vec<(String, String)>)> {
    groups
        .iter()
        .map(|(name, rows)| {
            let rows = rows
                .iter()
                .map(|(key, kind)| ((*key).to_string(), (*kind).to_string()))
                .collect();
            ((*name).to_string(), rows)
        })
        .collect()
}

/// `frame_key` del modelo como huella `u64` del contrato.
fn frame_hash(menu: &Menu, now: f64) -> Option<u64> {
    let key = menu.frame_key(now)?;
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    Some(hasher.finish())
}

impl MenuPanel for LiveMenuPanel {
    fn set_theme(&mut self, theme: &str) {
        self.menu.lock().set_theme(theme);
    }

    fn set_scale(&mut self, scale: f64) {
        // `toggle_menu` atrapa el `ValueError` de una escala no positiva.
        let _ = self.menu.lock().set_scale(scale);
    }

    fn set_reduced_motion(&mut self, reduced: bool) {
        self.menu.lock().set_reduced_motion(reduced);
    }

    fn set_available_height(&mut self, height: f64) {
        self.menu.lock().set_available_height(Some(height));
    }

    fn set_width(&mut self, width: f64) {
        self.menu.lock().set_width(Some(width));
    }

    fn set_max_height(&mut self, height: f64) {
        self.menu.lock().set_max_height(Some(height));
    }

    fn set_groups(&mut self, groups: &[Group], title: &str, now: f64) {
        self.menu
            .lock()
            .set_groups(model_groups(groups), Some(title), Some(now));
    }

    fn set_palette_overrides(&mut self, overrides: fn(&str) -> ColorMap) {
        self.menu
            .lock()
            .set_palette_overrides(Some(Box::new(overrides)));
    }

    fn open(&mut self, now: f64) {
        self.menu.lock().open(now);
    }

    fn close(&mut self, now: f64) {
        self.menu.lock().close(now);
    }

    fn resize(&mut self, width: f64, height: f64, now: f64) {
        self.menu.lock().resize(width, height, now);
    }

    fn set_testing(&mut self, testing: bool) {
        self.menu.lock().set_testing(testing, None);
    }

    fn set_settings(&mut self, settings: MenuSettings) {
        self.menu.lock().set_settings(settings, None);
    }

    fn set_llm_models(&mut self, provider: &str, models: &[String]) {
        let source = OptionSource::Items(
            models
                .iter()
                .cloned()
                .map(OptionInput::Text)
                .collect::<Vec<_>>(),
        );
        self.menu
            .lock()
            .settings
            .set_llm_models(provider, Some(&source));
    }

    fn set_live_scale(&mut self, scale: f64) {
        // `bar_wheel`: `menu.settings.set("ui_scale", target)`.
        let _ = self
            .menu
            .lock()
            .settings
            .set("ui_scale", &SettingValue::Float(scale));
    }

    fn on_applied(&mut self, ok: bool) {
        self.menu.lock().on_applied(ok);
    }

    fn set_input_level(&mut self, level: f64, muted: bool, active: bool) {
        self.menu.lock().set_input_level(level, muted, active);
    }

    fn update(&mut self, now: f64) {
        self.menu.lock().update(now);
    }

    fn height(&mut self, now: f64) -> f64 {
        self.menu.lock().height(now)
    }

    fn viewport_height(&mut self) -> f64 {
        self.menu.lock().viewport_height()
    }

    fn width(&mut self) -> f64 {
        self.menu.lock().width()
    }

    fn frame_key(&mut self, now: f64) -> Option<u64> {
        frame_hash(&self.menu.lock(), now)
    }

    fn is_animating(&mut self, now: f64) -> bool {
        self.menu.lock().is_animating(now)
    }

    fn draw(&mut self, buf: &mut Layer, now: f64) {
        // `draw_with` y no `draw`: las filas avanzadas (F5-10b) van en
        // `paint::menu::advanced::Advanced`.
        self.menu.lock().draw_with(buf, now, &mut Advanced);
    }

    fn hit(&self, x: f64, y: f64, now: f64) -> bool {
        self.menu.lock().hit(x, y, now).is_some()
    }

    fn pointer_move(&mut self, x: f64, y: f64, now: f64) {
        self.menu.lock().pointer_move(x, y, now);
    }

    fn pointer_down(&mut self, x: f64, y: f64, now: f64) {
        self.menu.lock().pointer_down(x, y, now);
    }

    fn pointer_up(&mut self, x: f64, y: f64, now: f64) {
        self.menu.lock().pointer_up(x, y, now);
    }

    fn editing_key(&self) -> bool {
        self.menu.lock().editing_key().is_some()
    }

    fn wheel(&mut self, x: f64, y: f64, delta: i32, now: f64) -> bool {
        self.menu.lock().wheel(x, y, delta, now)
    }

    fn text_key(&mut self, vk: u32, ctrl: bool) -> bool {
        self.menu.lock().text_key(vk as i32, ctrl)
    }

    fn text_input(&mut self, character: char) -> bool {
        self.menu.lock().text_input(&character.to_string())
    }

    fn paste_text(&mut self, text: &str) -> bool {
        self.menu.lock().paste_text(text)
    }

    fn finish_text_edit(&mut self, commit: bool) {
        self.menu.lock().finish_text_edit(commit);
    }
}

/// Fábrica real del menú de la barra (la única que crea el `Menu` de verdad).
#[derive(Default)]
pub struct LiveMenuFactory;

impl MenuFactory for LiveMenuFactory {
    fn create(
        &self,
        settings: MenuSettings,
        scale: f64,
        theme: &str,
        screen_h: f64,
        callbacks: MenuCallbacks,
    ) -> Option<Arc<Mutex<dyn MenuPanel>>> {
        let panel = LiveMenuPanel::new(settings, scale, theme, screen_h, callbacks)?;
        Some(Arc::new(Mutex::new(panel)))
    }
}

#[cfg(test)]
mod tests;
