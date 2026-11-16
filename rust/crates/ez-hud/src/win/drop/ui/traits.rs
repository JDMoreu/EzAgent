//! Contratos públicos de `DropUI` con la app y con las piezas que aún no
//! existen (menú, chat y ventanas dobles para pruebas).
//!
//! // DECISION: en Python estos acoplamientos son atributos y lambdas que se
//! // reasignan desde `app.py`. En Rust se declaran como traits pequeños y
//! // estables: `ez-app` los implementa una vez y las pruebas usan dobles sin
//! // abrir ventanas reales.

use std::sync::{Arc, Weak};

use ez_gfx::sdf::Layer;
use parking_lot::Mutex;

use crate::layout::Group;
use crate::model::settings::{MenuSettings, SettingsValues};
use crate::paint::peek::PeekItem;
use crate::tokens::ColorMap;
use crate::win::layered::RgbaFrame;

use super::DropUI;

/// Valor que `DropUI` lee o guarda en la configuración (`voice.json`).
///
/// // DECISION: `ez-hud` no declara `serde_json`; este enum evita nombrar
/// // `serde_json::Value` en la API y el adaptador de `ez-app` lo traduce.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl ConfigValue {
    /// `float(value)`: enteros y flotantes.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }

    /// `isinstance(value, int)` del Python (solo enteros).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// Texto, si lo es.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

/// Configuración que `DropUI` lee y persiste (`VoiceConfig` en `ez-core`).
pub trait DropConfig: Send + Sync {
    /// `getattr(cfg, key, None)`: `None` también para JSON `null`.
    fn value(&self, _key: &str) -> Option<ConfigValue> {
        None
    }

    /// `cfg.save_key(key, value)`.
    fn save(&self, _key: &str, _value: ConfigValue) {}

    /// Pide el guardado sin bloquear al hilo que llama (la barra guarda su
    /// posición desde el hilo de pintura). Por omisión, como [`Self::save`].
    fn save_soon(&self, key: &str, value: ConfigValue) {
        self.save(key, value);
    }

    /// El atributo en memoria antes de un «Guardar» de Ajustes.
    fn remember(&self, _key: &str, _value: ConfigValue) {}
}

impl DropConfig for () {}

/// Acciones y avisos de `DropUI` hacia la app (`on_action`, `on_live_setting`…).
pub trait DropHost: Send + Sync {
    /// `on_action`: `panel`, `chat`, `mute`, `listen` o `hangup`.
    fn action(&self, _action: &str) {}
    /// `on_position_change(x, y)` al soltar un arrastre de la barra.
    fn position_changed(&self, _x: i32, _y: i32) {}
    /// `on_live_setting(key, value)`.
    fn live_setting(&self, _key: &str, _value: ConfigValue) {}
    /// `on_save_settings(values)`: `None` si la app no devuelve nada.
    fn save_settings(&self, _values: &SettingsValues) -> Option<bool> {
        None
    }
    /// `on_test_voice(values)`.
    fn test_voice(&self, _values: &SettingsValues) {}
    /// `on_menu_action(action)`.
    fn menu_action(&self, _action: &str) {}
    /// `on_hotkey_capture(active)`.
    fn hotkey_capture(&self, _active: bool) {}
    /// `on_activity_open(key)`: trae al frente la terminal del agente.
    fn activity_open(&self, _key: &str) {}
    /// `on_activity_dismiss(key)`: la app olvida el aviso en su hilo.
    fn activity_dismiss(&self, _key: &str) {}
}

impl DropHost for () {}

/// `on_live_setting`.
pub type LiveCallback = Arc<dyn Fn(&str, ConfigValue) + Send + Sync>;
/// `on_save_settings`.
pub type SaveCallback = Arc<dyn Fn(&SettingsValues) -> Option<bool> + Send + Sync>;
/// `on_test_voice`.
pub type TestCallback = Arc<dyn Fn(&SettingsValues) + Send + Sync>;
/// Acción sin valor (`on_hide`, `on_close`).
pub type VoidCallback = Arc<dyn Fn() + Send + Sync>;
/// `on_text_edit` / `on_hotkey_capture`.
pub type BoolCallback = Arc<dyn Fn(bool) + Send + Sync>;
/// `on_action` / `on_menu_action`.
pub type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;
/// `on_copy`: `true` si llegó al portapapeles.
pub type CopyCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;
/// `menu_settings_provider`.
pub type MenuSettingsProvider = Arc<dyn Fn() -> Option<MenuSettings> + Send + Sync>;

/// Llamadas que el menú hace de vuelta a `DropUI` (el `Menu(...)` de Python).
pub struct MenuCallbacks {
    pub live_change: LiveCallback,
    pub save: SaveCallback,
    pub test: TestCallback,
    pub hide: VoidCallback,
    pub close: VoidCallback,
    pub text_edit: BoolCallback,
    pub action: TextCallback,
    pub copy: CopyCallback,
    pub hotkey_capture: BoolCallback,
}

impl MenuCallbacks {
    /// Las llamadas que `DropUI` pasa al menú, todas por el `Weak` de la barra.
    pub(super) fn for_ui(ui: &Weak<DropUI>) -> Self {
        Self {
            live_change: live_callback(ui),
            save: save_callback(ui),
            test: test_callback(ui),
            hide: hide_callback(ui),
            close: close_callback(ui),
            text_edit: text_edit_callback(ui),
            action: action_callback(ui),
            copy: copy_callback(ui),
            hotkey_capture: hotkey_callback(ui),
        }
    }
}

fn live_callback(ui: &Weak<DropUI>) -> LiveCallback {
    let ui = ui.clone();
    Arc::new(move |key, value| {
        if let Some(ui) = ui.upgrade() {
            ui.live_setting(key, value);
        }
    })
}

fn save_callback(ui: &Weak<DropUI>) -> SaveCallback {
    let ui = ui.clone();
    Arc::new(move |values| ui.upgrade().and_then(|ui| ui.host.save_settings(values)))
}

fn test_callback(ui: &Weak<DropUI>) -> TestCallback {
    let ui = ui.clone();
    Arc::new(move |values| {
        if let Some(ui) = ui.upgrade() {
            ui.host.test_voice(values);
        }
    })
}

fn hide_callback(ui: &Weak<DropUI>) -> VoidCallback {
    let ui = ui.clone();
    Arc::new(move || {
        if let Some(ui) = ui.upgrade() {
            ui.hide_from_menu();
        }
    })
}

fn close_callback(ui: &Weak<DropUI>) -> VoidCallback {
    let ui = ui.clone();
    Arc::new(move || {
        if let Some(ui) = ui.upgrade() {
            ui.menu_closed();
        }
    })
}

fn text_edit_callback(ui: &Weak<DropUI>) -> BoolCallback {
    let ui = ui.clone();
    Arc::new(move |active| {
        if let Some(ui) = ui.upgrade() {
            ui.menu_text_edit(active);
        }
    })
}

fn action_callback(ui: &Weak<DropUI>) -> TextCallback {
    let ui = ui.clone();
    Arc::new(move |action| {
        if let Some(ui) = ui.upgrade() {
            ui.menu_action(action);
        }
    })
}

fn copy_callback(ui: &Weak<DropUI>) -> CopyCallback {
    let ui = ui.clone();
    Arc::new(move |text| ui.upgrade().is_some_and(|ui| ui.copy_text(text)))
}

fn hotkey_callback(ui: &Weak<DropUI>) -> BoolCallback {
    let ui = ui.clone();
    Arc::new(move |active| {
        if let Some(ui) = ui.upgrade() {
            ui.host.hotkey_capture(active);
        }
    })
}

/// API del menú que consume la tarjeta de ajustes (`menu.py`).
///
/// F5-09/F5-10 aún no existen como crate; este trait fija lo que `DropUI`
/// necesita para abrir, pintar y repartir el puntero del menú.
pub trait MenuPanel: Send + 'static {
    fn set_theme(&mut self, _theme: &str) {}
    fn set_scale(&mut self, _scale: f64) {}
    fn set_reduced_motion(&mut self, _reduced: bool) {}
    fn set_available_height(&mut self, _height: f64) {}
    fn set_width(&mut self, _width: f64) {}
    fn set_max_height(&mut self, _height: f64) {}
    fn set_groups(&mut self, _groups: &[Group], _title: &str, _now: f64) {}
    fn set_palette_overrides(&mut self, _overrides: fn(&str) -> ColorMap) {}
    fn open(&mut self, _now: f64) {}
    fn close(&mut self, _now: f64) {}
    fn resize(&mut self, _width: f64, _height: f64, _now: f64) {}
    fn set_testing(&mut self, _testing: bool) {}
    fn set_settings(&mut self, _settings: MenuSettings) {}
    fn set_llm_models(&mut self, _provider: &str, _models: &[String]) {}
    /// `menu.settings.set("ui_scale", target)` sin exponer los ajustes.
    fn set_live_scale(&mut self, _scale: f64) {}
    fn on_applied(&mut self, _ok: bool) {}
    fn set_input_level(&mut self, _level: f64, _muted: bool, _active: bool) {}
    fn update(&mut self, _now: f64) {}
    fn height(&mut self, _now: f64) -> f64 {
        0.0
    }
    fn viewport_height(&mut self) -> f64 {
        0.0
    }
    fn width(&mut self) -> f64 {
        0.0
    }
    /// Clave de un cuadro quieto (sin la marca viva), o `None` si se mueve.
    fn frame_key(&mut self, _now: f64) -> Option<u64> {
        None
    }
    fn is_animating(&mut self, _now: f64) -> bool {
        false
    }
    fn draw(&mut self, _buf: &mut Layer, _now: f64) {}
    fn hit(&self, _x: f64, _y: f64, _now: f64) -> bool {
        false
    }
    fn pointer_move(&mut self, _x: f64, _y: f64, _now: f64) {}
    fn pointer_down(&mut self, _x: f64, _y: f64, _now: f64) {}
    fn pointer_up(&mut self, _x: f64, _y: f64, _now: f64) {}
    fn editing_key(&self) -> bool {
        false
    }
    fn wheel(&mut self, _x: f64, _y: f64, _delta: i32, _now: f64) -> bool {
        false
    }
    fn text_key(&mut self, _vk: u32, _ctrl: bool) -> bool {
        false
    }
    fn text_input(&mut self, _character: char) -> bool {
        false
    }
    fn paste_text(&mut self, _text: &str) -> bool {
        false
    }
    fn finish_text_edit(&mut self, _commit: bool) {}
}

/// Crea el menú con los ajustes y las llamadas que `DropUI` le pasa.
pub trait MenuFactory: Send + Sync {
    fn create(
        &self,
        settings: MenuSettings,
        scale: f64,
        theme: &str,
        screen_h: f64,
        callbacks: MenuCallbacks,
    ) -> Option<Arc<Mutex<dyn MenuPanel>>>;
}

/// Chat escrito (`chat_ui.py`, F5-14/F5-15). `DropUI` solo lo abre, lo cierra,
/// lo consulta y lo deja avanzar una vez por cuadro.
pub trait ChatPanel: Send + Sync {
    fn toggle(&self) {}
    fn is_open(&self) -> bool {
        false
    }
    fn close_now(&self) {}
    fn poll(&self, _now: f64) {}
    /// `chat_model.set_mic_muted(muted)`.
    fn set_mic_muted(&self, _muted: bool) {}
}

/// Chat que no existe todavía: la barra funciona sin panel.
pub struct NoChat;

impl ChatPanel for NoChat {}

/// Operaciones de ventana que usa `DropUI`; el doble de pruebas la implementa
/// sin Win32.
pub trait HudWindow: Send + Sync {
    /// Arranca el hilo de la ventana; `false` si no hay sesión gráfica.
    fn start(&self) -> bool;
    fn show(&self);
    fn hide(&self);
    fn stop(&self);
    fn set_pos(&self, x: i32, y: i32);
    fn resize(&self, width: i32, height: i32);
    fn position(&self) -> (i32, i32);
    fn size(&self) -> (i32, i32);
    fn is_visible(&self) -> bool;
    /// `win._dragging and win._drag_moved`.
    fn dragging_moved(&self) -> bool {
        false
    }
    /// `win._drag_off`: el agarre se corre cuando cambia el dock.
    fn shift_drag(&self, _dx: i32, _dy: i32) {}
    fn set_keyboard_enabled(&self, _enabled: bool) {}
    fn copy_text(&self, _text: &str) -> bool {
        false
    }
}

/// Crea la ventana nativa de la barra o de la tarjeta.
pub trait WindowFactory: Send + Sync {
    fn bar(&self, ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32) -> Arc<dyn HudWindow>;
    fn card(&self, ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32)
        -> Arc<dyn HudWindow>;
}

/// Fotograma de la barra: BGRA premultiplicado, como `render_model` en Python,
/// más la cadencia que pide el modelo.
#[derive(Clone, Debug, PartialEq)]
pub struct BarFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA premultiplicado (0..255).
    pub data: Vec<u8>,
    pub fps: f64,
}

impl BarFrame {
    /// `1 / fps`, acotado como en `LayeredWindow`.
    pub fn interval(&self) -> f64 {
        1.0 / self.fps.max(1.0)
    }

    /// RGBA recto para [`crate::win::layered`].
    pub fn to_rgba(&self) -> Option<RgbaFrame> {
        bgra_to_rgba(&self.data, self.width, self.height)
    }
}

/// Fotograma de la tarjeta de ajustes, en el mismo formato que [`BarFrame`].
#[derive(Clone, Debug, PartialEq)]
pub struct CardFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA premultiplicado (0..255).
    pub data: Vec<u8>,
    pub fps: f64,
}

impl CardFrame {
    /// `1 / fps`, acotado como en `LayeredWindow`.
    pub fn interval(&self) -> f64 {
        1.0 / self.fps.max(1.0)
    }

    /// RGBA recto para [`crate::win::layered`].
    pub fn to_rgba(&self) -> Option<RgbaFrame> {
        bgra_to_rgba(&self.data, self.width, self.height)
    }
}

/// `Image.fromarray(BGRA premultiplicado, "RGBa").convert("RGBA")`.
pub(crate) fn bgra_to_rgba(data: &[u8], width: u32, height: u32) -> Option<RgbaFrame> {
    if width == 0 || height == 0 || data.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut rgba = data.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = ez_gfx::Image::from_premultiplied_rgba(width, height, rgba).ok()?;
    RgbaFrame::new(width, height, image.into_raw())
}

/// Elemento de actividad que la app entrega a `DropUI`.
pub type ActivityItem = PeekItem;
