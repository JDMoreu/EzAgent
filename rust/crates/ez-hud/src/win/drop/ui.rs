//! `DropUI`: controlador de la barra (`drop.py 978-1275`).
//!
//! La app solo habla con esta estructura. Todas las operaciones publican
//! estado bajo locks cortos y vuelven; el hilo de pintura de cada ventana las
//! aplica en el cuadro siguiente. Las llamadas a la app (`DropHost`) se hacen
//! siempre fuera de los locks de la barra.
//!
//! // DECISION: `root` de Tk no existe; la escala inicial llega en
//! // [`DropUiOptions`] y los avisos de DPI por `set_scale`.

mod appearance;
mod card_input;
mod input;
mod lifecycle;
mod menu;
mod render;
mod traits;
mod types;

#[cfg(test)]
mod tests;

pub use lifecycle::{create_ui, NativeWindowFactory};
use types::{BarState, CustomTheme, PeekView};
pub use types::{CardKey, CardSlot, CardText, DropUiOptions, FluidBuffer, PointerPhase};

use std::sync::{Arc, Weak};

use parking_lot::{Mutex, RwLock};

use ez_core::metrics::monotonic;

use crate::model::drop::{
    choose_dock, clean_dock, form_center, layout_size, DropModel, CIRCLE_D, LAYOUTS, MOTION_MODES,
    PAD, PILL_H, PILL_W,
};
use crate::model::settings::{UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP};
use crate::paint::peek::{NoIcons, PeekItem};
use crate::paint::settings_window::{self as sw, Edges};
use crate::paint::state::UiState;
use crate::win::monitor;
use crate::win::peek::Peek;

pub use traits::{
    ActivityItem, BarFrame, BoolCallback, CardFrame, ChatPanel, ConfigValue, CopyCallback,
    DropConfig, DropHost, HudWindow, LiveCallback, MenuCallbacks, MenuFactory, MenuPanel,
    MenuSettingsProvider, NoChat, SaveCallback, TestCallback, TextCallback, VoidCallback,
    WindowFactory,
};

/// `ORBIT_LIMIT` de `activity.py`: marcas como máximo en la órbita.
pub const ORBIT_LIMIT: usize = 5;

/// Controlador de la barra; la app solo habla con esta estructura.
pub struct DropUI {
    self_weak: Weak<DropUI>,
    host: Arc<dyn DropHost>,
    config: Option<Arc<dyn DropConfig>>,
    factory: Arc<dyn WindowFactory>,
    menu_factory: Option<Arc<dyn MenuFactory>>,
    menu_settings_provider: Mutex<Option<MenuSettingsProvider>>,
    peek: Arc<Peek>,
    peek_view: Mutex<PeekView>,
    bar: Mutex<BarState>,
    ui: UiState,
    win: Mutex<Option<Arc<dyn HudWindow>>>,
    theme_setting: Mutex<String>,
    theme_cache: Mutex<(String, f64)>,
    custom: Mutex<CustomTheme>,
    on_yes: Mutex<Option<VoidCallback>>,
    on_no: Mutex<Option<VoidCallback>>,
    activity: RwLock<Vec<PeekItem>>,
    chat: RwLock<Arc<dyn ChatPanel>>,
    chat_size: Mutex<Option<(f64, f64)>>,
    fluid_cache: Mutex<Option<(f64, ndarray::Array3<u8>)>>,
    menu: Mutex<Option<Arc<Mutex<dyn MenuPanel>>>>,
    card: Mutex<Option<CardSlot>>,
    section: Mutex<String>,
    sidebar_hover: Mutex<Option<String>>,
    sidebar: Mutex<Option<sw::Sidebar>>,
    card_cache: Mutex<Option<(CardKey, Vec<u8>)>>,
    pending_save: Mutex<Option<bool>>,
    header_click: Mutex<Option<f64>>,
    resizing: Mutex<Option<Edges>>,
}

fn config_value(config: Option<&Arc<dyn DropConfig>>, key: &str) -> Option<ConfigValue> {
    config?.value(key)
}

pub(super) fn config_text(config: Option<&Arc<dyn DropConfig>>, key: &str) -> Option<String> {
    config_value(config, key).and_then(|value| match value {
        ConfigValue::Text(text) => Some(text),
        _ => None,
    })
}

pub(super) fn config_number(config: Option<&Arc<dyn DropConfig>>, key: &str) -> Option<f64> {
    config_value(config, key).and_then(|value| value.as_f64())
}

pub(super) fn config_integer(config: Option<&Arc<dyn DropConfig>>, key: &str) -> Option<i64> {
    config_value(config, key).and_then(|value| value.as_i64())
}

fn valid_scale(value: f64) -> f64 {
    if value.is_finite() {
        value.max(1.0)
    } else {
        1.0
    }
}

/// `DropUI._clean_bar_scale`: acotado y al paso del slider (5 %).
pub fn clean_bar_scale(value: f64) -> f64 {
    let number = if value.is_finite() { value } else { 1.0 };
    let steps = ((number - UI_SCALE_MIN) / UI_SCALE_STEP).round_ties_even();
    let scaled = (UI_SCALE_MIN + steps * UI_SCALE_STEP).clamp(UI_SCALE_MIN, UI_SCALE_MAX);
    (scaled * 10_000.0).round_ties_even() / 10_000.0
}

/// `_form_dims`: tamaño lógico de la forma de color.
pub(super) fn form_dims(layout: &str) -> (f64, f64) {
    if layout == "circle" {
        (CIRCLE_D, CIRCLE_D)
    } else {
        (PILL_W, PILL_H)
    }
}

/// Tamaño físico de la ventana para un estado de la barra.
fn frame_size(bar: &BarState) -> (i32, i32) {
    frame_size_parts(bar.layout, bar.dock, bar.scale, bar.bar_scale)
}

fn frame_size_parts(layout: &str, dock: &str, scale: f64, bar_scale: f64) -> (i32, i32) {
    let (width, height) = layout_size(layout, Some(dock));
    let factor = scale * bar_scale;
    (
        (width * factor).round_ties_even() as i32,
        (height * factor).round_ties_even() as i32,
    )
}

impl DropUI {
    /// Crea el controlador; todavía no abre ninguna ventana.
    pub fn new(options: DropUiOptions) -> Arc<Self> {
        let configured = config_text(options.config.as_ref(), "ui_layout").unwrap_or_default();
        let configured = configured.to_lowercase();
        let layout = LAYOUTS
            .iter()
            .find(|candidate| **candidate == configured)
            .copied()
            .unwrap_or("pill");
        let bar_scale =
            clean_bar_scale(config_number(options.config.as_ref(), "ui_scale").unwrap_or(1.0));
        let same_layout =
            config_text(options.config.as_ref(), "drop_layout").as_deref() == Some(layout);
        let saved_dock = if same_layout {
            config_text(options.config.as_ref(), "drop_dock")
        } else {
            None
        };
        let dock = clean_dock(layout, saved_dock.as_deref());
        let motion = config_text(options.config.as_ref(), "ui_motion")
            .unwrap_or_default()
            .to_lowercase();
        let motion = MOTION_MODES
            .iter()
            .find(|candidate| **candidate == motion)
            .copied()
            .unwrap_or("system");
        let scale = valid_scale(options.scale);
        let icons = options.icons.unwrap_or_else(|| Arc::new(NoIcons));
        let host = options.host.unwrap_or_else(|| Arc::new(()));
        let factory = options
            .factory
            .unwrap_or_else(|| Arc::new(NativeWindowFactory));
        let reduced = motion == "reduced" || !crate::anim::animations_enabled();
        let last = monotonic();
        Arc::new_cyclic(|weak| {
            let proxy: Arc<dyn crate::win::peek::PeekUi> =
                Arc::new(render::BarPeekUi { ui: weak.clone() });
            let peek = Arc::new(Peek::new(proxy, icons));
            Self {
                self_weak: weak.clone(),
                host,
                config: options.config,
                factory,
                menu_factory: options.menu_factory,
                menu_settings_provider: Mutex::new(None),
                peek,
                peek_view: Mutex::new(PeekView::default()),
                bar: Mutex::new(BarState {
                    scale,
                    bar_scale,
                    layout,
                    dock,
                    buttons: String::from("hover"),
                    motion,
                    position: options.position,
                    model: DropModel::new(layout, Some(dock)),
                    state: String::from("idle"),
                    rms: 0.0,
                    mic_level: 0.0,
                    muted: false,
                    visible: false,
                    in_call: false,
                    menu_open: false,
                    chat_open: false,
                    asking: false,
                    hover_until: 0.0,
                    pressed: None,
                    form_click: None,
                    hover_item: None,
                    last,
                    reduced,
                    reduced_at: 0.0,
                    persist_pos_at: 0.0,
                }),
                ui: UiState::new(),
                win: Mutex::new(None),
                theme_setting: Mutex::new(String::from("auto")),
                theme_cache: Mutex::new((String::from("dark"), f64::NEG_INFINITY)),
                custom: Mutex::new(CustomTheme {
                    palette: None,
                    base: String::from("dark"),
                    hex: None,
                }),
                on_yes: Mutex::new(None),
                on_no: Mutex::new(None),
                activity: RwLock::new(Vec::new()),
                chat: RwLock::new(Arc::new(NoChat)),
                chat_size: Mutex::new(None),
                fluid_cache: Mutex::new(None),
                menu: Mutex::new(None),
                card: Mutex::new(None),
                section: Mutex::new(String::from("appearance")),
                sidebar_hover: Mutex::new(None),
                sidebar: Mutex::new(None),
                card_cache: Mutex::new(None),
                pending_save: Mutex::new(None),
                header_click: Mutex::new(None),
                resizing: Mutex::new(None),
            }
        })
    }

    /// `DropUI.bs`: escala de dibujo de la barra (pantalla × tamaño elegido).
    pub fn bs(&self) -> f64 {
        let bar = self.bar.lock();
        bar.scale * bar.bar_scale
    }

    /// `DropUI.s`: escala de pantalla.
    pub fn scale(&self) -> f64 {
        self.bar.lock().scale
    }

    /// `DropUI.bar_scale`: tamaño elegido en Ajustes.
    pub fn bar_scale(&self) -> f64 {
        self.bar.lock().bar_scale
    }

    /// Disposición actual (`pill` o `circle`).
    pub fn layout(&self) -> &'static str {
        self.bar.lock().layout
    }

    /// Lado actual de los botones.
    pub fn dock(&self) -> &'static str {
        self.bar.lock().dock
    }

    /// Modo de movimiento (`system`, `full`, `reduced`).
    pub fn motion(&self) -> &'static str {
        self.bar.lock().motion
    }

    /// `DropUI.buttons`: `hover` (por defecto) o `always` (solo pruebas).
    pub fn buttons(&self) -> String {
        self.bar.lock().buttons.clone()
    }

    /// Cambia la visibilidad de los botones (`hover` o `always`).
    pub fn set_buttons(&self, buttons: &str) {
        self.bar.lock().buttons = buttons.to_string();
    }

    /// Configuración de la app (para que F6 lea y escriba las mismas claves).
    pub fn config(&self) -> Option<Arc<dyn DropConfig>> {
        self.config.clone()
    }

    /// `DropUI.size()`: tamaño físico de la barra.
    pub fn size(&self) -> (i32, i32) {
        frame_size(&self.bar.lock())
    }

    /// `_default_position`: abajo y con la forma centrada en la pantalla.
    pub fn default_position(&self) -> (i32, i32) {
        let (layout, dock, scale, bar_scale) = {
            let bar = self.bar.lock();
            (bar.layout, bar.dock, bar.scale, bar.bar_scale)
        };
        let work = monitor::work_area();
        let (_, height) = frame_size_parts(layout, dock, scale, bar_scale);
        let (form_x, _) = form_center(layout, Some(dock));
        let bs = scale * bar_scale;
        (
            ((work.0 + work.2) as f64 / 2.0 - form_x * bs).round_ties_even() as i32,
            work.3 - height - (6.0 * bs).round_ties_even() as i32,
        )
    }

    /// Esquina de la ventana para que la forma quede en `anchor`.
    pub fn origin_for(&self, anchor: (f64, f64), dock: &str) -> (i32, i32) {
        let (layout, scale, bar_scale) = {
            let bar = self.bar.lock();
            (bar.layout, bar.scale, bar.bar_scale)
        };
        let (form_x, form_y) = form_center(layout, Some(dock));
        let bs = scale * bar_scale;
        (
            (anchor.0 - form_x * bs).round_ties_even() as i32,
            (anchor.1 - form_y * bs).round_ties_even() as i32,
        )
    }

    /// Centro de la forma en pantalla, o `None` sin ventana.
    pub fn form_anchor(&self) -> Option<(f64, f64)> {
        let win = self.window()?;
        let (layout, dock, scale, bar_scale) = {
            let bar = self.bar.lock();
            (bar.layout, bar.dock, bar.scale, bar.bar_scale)
        };
        let (form_x, form_y) = form_center(layout, Some(dock));
        let (x, y) = win.position();
        let bs = scale * bar_scale;
        Some((x as f64 + form_x * bs, y as f64 + form_y * bs))
    }

    /// `hub_extent`: rectángulo en pantalla de la forma y sus botones.
    pub fn hub_extent(&self) -> Option<(f64, f64, f64, f64)> {
        let win = self.window()?;
        let (rect, bs) = {
            let bar = self.bar.lock();
            let mic = bar.model.mic_space.x.clamp(0.0, 1.0);
            (bar.model.hub_rect(mic), bar.scale * bar.bar_scale)
        };
        let (x, y) = win.position();
        Some((
            x as f64 + rect.0 * bs,
            y as f64 + rect.1 * bs,
            x as f64 + rect.2 * bs,
            y as f64 + rect.3 * bs,
        ))
    }

    /// `form_extent`: centro de la forma y su medio alto (px físicos).
    pub fn form_extent(&self) -> Option<(f64, f64, f64)> {
        let anchor = self.form_anchor()?;
        let (layout, bs) = {
            let bar = self.bar.lock();
            (bar.layout, bar.scale * bar.bar_scale)
        };
        let (_, height) = form_dims(layout);
        Some((anchor.0, anchor.1, height / 2.0 * bs))
    }

    /// Mantiene la barra dentro del área de trabajo.
    pub fn clamp_position(&self, x: i32, y: i32) -> (i32, i32) {
        let work = monitor::work_area();
        let bar = self.bar.lock();
        let (width, height) = frame_size(&bar);
        let margin = (PAD * bar.scale * bar.bar_scale).round_ties_even() as i32;
        (
            (work.0 - margin).max(x.min(work.2 - width + margin)),
            (work.1 - margin).max(y.min(work.3 - height + margin)),
        )
    }

    /// Ventana de la barra, si ya se creó.
    pub fn window(&self) -> Option<Arc<dyn HudWindow>> {
        self.win.lock().clone()
    }

    /// `DropUI._motion_reduced`.
    pub(super) fn motion_reduced(&self) -> bool {
        match self.bar.lock().motion {
            "reduced" => true,
            "full" => false,
            _ => !crate::anim::animations_enabled(),
        }
    }

    /// `set_motion`.
    pub fn set_motion(&self, motion: &str) {
        let motion = MOTION_MODES
            .iter()
            .find(|candidate| **candidate == motion)
            .copied()
            .unwrap_or("system");
        self.bar.lock().motion = motion;
        let reduced = self.motion_reduced();
        let mut bar = self.bar.lock();
        bar.reduced = reduced;
        bar.reduced_at = monotonic();
    }

    /// `_ensure_window`: crea la barra con el dock que quepa y la arranca.
    pub fn ensure_window(&self) -> bool {
        let window = {
            let mut slot = self.win.lock();
            if slot.is_none() {
                let (layout, dock, scale, bar_scale, position) = {
                    let bar = self.bar.lock();
                    (bar.layout, bar.dock, bar.scale, bar.bar_scale, bar.position)
                };
                let (x, y) = position.unwrap_or_else(|| self.default_position());
                let bs = scale * bar_scale;
                let (form_x, form_y) = form_center(layout, Some(dock));
                let anchor = (x as f64 + form_x * bs, y as f64 + form_y * bs);
                let dock = choose_dock(layout, Some(dock), anchor, monitor::work_area(), bs);
                {
                    let mut bar = self.bar.lock();
                    bar.dock = dock;
                    bar.model.dock = dock;
                }
                let (origin_x, origin_y) = self.origin_for(anchor, dock);
                let (x, y) = self.clamp_position(origin_x, origin_y);
                let (width, height) = frame_size_parts(layout, dock, scale, bar_scale);
                *slot = Some(
                    self.factory
                        .bar(self.self_weak.clone(), x, y, width, height),
                );
            }
            slot.as_ref().map(Arc::clone)
        };
        window.is_some_and(|window| window.start())
    }

    /// `_retarget`: recoloca la ventana con la forma en `anchor`.
    pub(super) fn retarget(&self, anchor: (f64, f64)) {
        let Some(win) = self.window() else {
            return;
        };
        let (layout, dock, scale, bar_scale) = {
            let bar = self.bar.lock();
            (bar.layout, bar.dock, bar.scale, bar.bar_scale)
        };
        let bs = scale * bar_scale;
        let dock = choose_dock(layout, Some(dock), anchor, monitor::work_area(), bs);
        {
            let mut bar = self.bar.lock();
            bar.dock = dock;
            bar.model.dock = dock;
        }
        let (origin_x, origin_y) = self.origin_for(anchor, dock);
        let (x, y) = self.clamp_position(origin_x, origin_y);
        let (width, height) = self.size();
        win.resize(width, height);
        win.set_pos(x, y);
        self.bar.lock().position = Some((x, y));
    }

    /// `save_key` sin dejar que un fallo de disco tumbe la barra.
    pub(super) fn save_config(&self, key: &str, value: ConfigValue) {
        if let Some(config) = &self.config {
            config.save(key, value);
        }
    }

    /// `_live_setting` hacia la app.
    pub(super) fn live_setting(&self, key: &str, value: ConfigValue) {
        self.host.live_setting(key, value);
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn bar_scale_snaps_to_the_slider_steps() {
        assert_eq!(clean_bar_scale(1.0), 1.0);
        assert_eq!(clean_bar_scale(1.024), 1.0);
        assert_eq!(clean_bar_scale(1.025), 1.0);
        assert_eq!(clean_bar_scale(1.075), 1.05);
        assert_eq!(clean_bar_scale(0.1), 0.6);
        assert_eq!(clean_bar_scale(9.0), 2.0);
        assert_eq!(clean_bar_scale(f64::NAN), 1.0);
        assert_eq!(clean_bar_scale(1.9999), 2.0);
    }

    #[test]
    fn form_dims_are_the_pill_or_the_circle() {
        assert_eq!(form_dims("pill"), (PILL_W, PILL_H));
        assert_eq!(form_dims("circle"), (CIRCLE_D, CIRCLE_D));
    }

    #[test]
    fn the_default_layout_and_dock_come_from_the_configuration() {
        struct Config;
        impl DropConfig for Config {
            fn value(&self, key: &str) -> Option<ConfigValue> {
                match key {
                    "ui_layout" => Some(ConfigValue::Text(String::from("CIRCLE"))),
                    "ui_scale" => Some(ConfigValue::Float(1.2)),
                    "drop_dock" => Some(ConfigValue::Text(String::from("left"))),
                    "drop_layout" => Some(ConfigValue::Text(String::from("circle"))),
                    "ui_motion" => Some(ConfigValue::Text(String::from("full"))),
                    _ => None,
                }
            }
        }
        let ui = DropUI::new(DropUiOptions {
            config: Some(Arc::new(Config)),
            ..DropUiOptions::default()
        });
        assert_eq!(ui.layout(), "circle");
        assert_eq!(ui.dock(), "left");
        assert_eq!(ui.bar_scale(), 1.2);
        assert_eq!(ui.motion(), "full");
    }

    #[test]
    fn a_dock_saved_with_another_layout_is_ignored() {
        struct Config;
        impl DropConfig for Config {
            fn value(&self, key: &str) -> Option<ConfigValue> {
                match key {
                    "ui_layout" => Some(ConfigValue::Text(String::from("pill"))),
                    "drop_layout" => Some(ConfigValue::Text(String::from("circle"))),
                    "drop_dock" => Some(ConfigValue::Text(String::from("left"))),
                    _ => None,
                }
            }
        }
        let ui = DropUI::new(DropUiOptions {
            config: Some(Arc::new(Config)),
            ..DropUiOptions::default()
        });
        assert_eq!(ui.dock(), "sides");
    }
}
