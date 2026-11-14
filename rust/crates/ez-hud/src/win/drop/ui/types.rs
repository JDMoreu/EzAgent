//! Tipos de estado y opciones de `DropUI`.

use std::sync::Arc;

use crate::model::drop::DropModel;
use crate::model::drop::Palette;
use crate::paint::peek::IconPainter;
use crate::win::monitor::{self, Rect};
use crate::win::settings_window::SettingsWindow;

use super::traits::{DropConfig, DropHost, HudWindow, MenuFactory, WindowFactory};

/// Estado mutable de la barra (el que comparten el hilo de pintura y los eventos).
pub(super) struct BarState {
    pub(super) scale: f64,
    pub(super) bar_scale: f64,
    pub(super) layout: &'static str,
    pub(super) dock: &'static str,
    pub(super) buttons: String,
    pub(super) motion: &'static str,
    pub(super) position: Option<(i32, i32)>,
    pub(super) model: DropModel,
    pub(super) state: String,
    pub(super) rms: f64,
    pub(super) mic_level: f64,
    pub(super) muted: bool,
    pub(super) visible: bool,
    pub(super) in_call: bool,
    pub(super) menu_open: bool,
    pub(super) chat_open: bool,
    pub(super) asking: bool,
    pub(super) hover_until: f64,
    pub(super) pressed: Option<String>,
    pub(super) form_click: Option<f64>,
    pub(super) hover_item: Option<String>,
    pub(super) last: f64,
    pub(super) reduced: bool,
    pub(super) reduced_at: f64,
    pub(super) persist_pos_at: f64,
}

/// Tema personalizado: paleta de Marea, base clara/oscura y los dos hex.
pub(super) struct CustomTheme {
    pub(super) palette: Option<Palette>,
    pub(super) base: String,
    pub(super) hex: Option<(String, String)>,
}

/// Instantánea que lee la vista rápida sin tocar los locks de la barra.
#[derive(Clone, Debug)]
pub(super) struct PeekView {
    pub(super) scale: f64,
    pub(super) bs: f64,
    pub(super) theme: String,
    pub(super) reduced: bool,
    pub(super) work: Rect,
    pub(super) hub: Option<(f64, f64, f64, f64)>,
    pub(super) form: Option<(f64, f64, f64)>,
}

impl Default for PeekView {
    fn default() -> Self {
        Self {
            scale: 1.0,
            bs: 1.0,
            theme: String::from("dark"),
            reduced: false,
            work: monitor::DEFAULT_WORK_AREA,
            hub: None,
            form: None,
        }
    }
}

/// Ventana de la tarjeta y su geometría (`SettingsWindow`, F5-12).
pub struct CardSlot {
    pub(super) window: Arc<dyn HudWindow>,
    pub(super) geometry: SettingsWindow,
}

/// Clave de la caché del cuadro quieto de la tarjeta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardKey {
    pub width: u32,
    pub height: u32,
    pub scale: u64,
    pub theme: String,
    pub palette: i64,
    pub section: String,
    pub sidebar_hover: Option<String>,
    pub still: u64,
}

/// Fase del puntero que reparte `card_pointer` (`"down"`, `"move"`, `"up"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
}

/// Texto que llega a la tarjeta (`card_text("key" | "char" | "paste", ...)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardText {
    Key(u32, bool),
    Char(char),
    Paste(String),
}

/// Buffer RGB del fluido estirado a un tamaño (`fluid_array` del chat).
#[derive(Clone, Debug, PartialEq)]
pub struct FluidBuffer {
    pub width: usize,
    pub height: usize,
    /// `width * height * 3` valores en `0..=1`.
    pub data: Vec<f32>,
}

/// Parámetros de construcción de [`DropUI`] (`DropUI(root, position, cfg)`).
pub struct DropUiOptions {
    pub position: Option<(i32, i32)>,
    pub config: Option<Arc<dyn DropConfig>>,
    pub host: Option<Arc<dyn DropHost>>,
    pub icons: Option<Arc<dyn IconPainter>>,
    pub factory: Option<Arc<dyn WindowFactory>>,
    pub menu_factory: Option<Arc<dyn MenuFactory>>,
    /// `root.winfo_fpixels("1i") / 96`; 1.0 sin Tk.
    pub scale: f64,
}

impl Default for DropUiOptions {
    fn default() -> Self {
        Self {
            position: None,
            config: None,
            host: None,
            icons: None,
            factory: None,
            menu_factory: None,
            scale: 1.0,
        }
    }
}
