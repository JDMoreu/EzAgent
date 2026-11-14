//! Port of `src/ezagent/voice/hud/menu.py 699-1342` (F5-07).
//!
//! Estado del menú, geometría y datos que llegan de fuera. Es modelo puro: no
//! dibuja ni toca Win32. Los otros módulos del menú reutilizan los tipos y los
//! campos públicos de [`Menu`].

use std::collections::{BTreeMap, BTreeSet};

use ez_gfx::sdf::Layer;

use crate::anim::{Tween, EASE_OUT};
use crate::layout;
use crate::model::settings::{
    MenuSettings, SettingValue, SettingsValues, ALL_KEYS, SELECT_KEYS, SLIDER_KEYS, TOGGLE_KEYS,
};
use crate::tokens::{Color, ColorMap, Rect, T_MICRO, W};

mod placement;
#[cfg(test)]
mod tests;

pub(crate) use placement::{block_delays, default_groups, T_SLOW_MS};

// --------------------------------------------------------------------------
// Callbacks de la capa de aplicación (los tipos del Python en `menu.py`).
// --------------------------------------------------------------------------

/// `LiveCallback = Callable[[str, Any], None]`.
pub type LiveCallback = Box<dyn Fn(&str, &SettingValue)>;
/// `SaveCallback = Callable[[dict[str, Any]], bool | None]`.
pub type SaveCallback = Box<dyn Fn(&SettingsValues) -> Option<bool>>;
/// `TestCallback = Callable[[dict[str, Any]], None]`.
pub type TestCallback = Box<dyn Fn(&SettingsValues)>;
/// `VoidCallback = Callable[[], None]`.
pub type VoidCallback = Box<dyn Fn()>;
/// `on_text_edit: Callable[[bool], None]`.
pub type TextEditCallback = Box<dyn Fn(bool)>;
/// `on_action: Callable[[str], None]`.
pub type ActionCallback = Box<dyn Fn(&str)>;
/// `on_copy: Callable[[str], bool]`.
pub type CopyCallback = Box<dyn Fn(&str) -> bool>;
/// `on_hotkey_capture: Callable[[bool], None]`.
pub type HotkeyCaptureCallback = Box<dyn Fn(bool)>;
/// `Callable[[str], dict[str, Color]]` de `set_palette_overrides`.
pub type PaletteProvider = Box<dyn Fn(&str) -> ColorMap>;
/// `modifier_source: Callable[[], frozenset[str]]`.
pub type ModifierSource = Box<dyn Fn() -> BTreeSet<String>>;

/// Callbacks que la capa de aplicación entrega al menú.
///
/// // DECISION: los callbacks de Rust no lanzan excepciones; `_notify` del
/// // Python atrapaba `Exception`, pero aquí no se capturan pánicos.
#[derive(Default)]
pub struct MenuCallbacks {
    pub live_change: Option<LiveCallback>,
    pub save: Option<SaveCallback>,
    pub test: Option<TestCallback>,
    pub hide: Option<VoidCallback>,
    pub close: Option<VoidCallback>,
    pub text_edit: Option<TextEditCallback>,
    pub action: Option<ActionCallback>,
    pub copy: Option<CopyCallback>,
    pub hotkey_capture: Option<HotkeyCaptureCallback>,
    /// Por defecto no hay modificadores pulsados (`hotkey_field` es F5-12).
    pub modifier_source: Option<ModifierSource>,
}

/// Opciones del constructor (`scale`, `theme` y `screen_h` del Python).
#[derive(Clone, Debug)]
pub struct MenuOptions {
    pub scale: f64,
    pub theme: String,
    pub screen_h: f64,
}

impl Default for MenuOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            theme: "dark".into(),
            screen_h: 1080.0,
        }
    }
}

/// Un grupo del menú: nombre y filas `(clave, tipo)`.
pub type MenuGroup = (String, Vec<(String, String)>);
/// Lista de grupos visibles.
pub type MenuGroups = Vec<MenuGroup>;

/// Tipos de `MenuHit.kind` (`Literal` del Python; `palette` ya lo devuelve
/// `hit` aunque falte en la anotación).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuHitKind {
    Hide,
    Select,
    Option,
    Slider,
    Toggle,
    Text,
    Meter,
    Action,
    Copy,
    Save,
    Palette,
}

impl MenuHitKind {
    /// Literal que usa el Python.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Select => "select",
            Self::Option => "option",
            Self::Slider => "slider",
            Self::Toggle => "toggle",
            Self::Text => "text",
            Self::Meter => "meter",
            Self::Action => "action",
            Self::Copy => "copy",
            Self::Save => "save",
            Self::Palette => "palette",
        }
    }
}

/// Resultado de `Menu.hit` en coordenadas lógicas de ventana.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuHit {
    pub kind: MenuHitKind,
    pub key: String,
    pub rect: Rect,
    pub value: Option<SettingValue>,
    pub index: Option<usize>,
    pub disabled: bool,
}

impl MenuHit {
    pub fn new(kind: MenuHitKind, key: impl Into<String>, rect: Rect) -> Self {
        Self {
            kind,
            key: key.into(),
            rect,
            value: None,
            index: None,
            disabled: false,
        }
    }

    pub fn with_value(mut self, value: SettingValue) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// --------------------------------------------------------------------------
// Medidas que `menu.py` nombra fuera de `tokens` y `menu_layout`.
// DECISION: se copian aquí porque el brief no permite tocar `tokens.rs` ni
// `layout.rs`; siguen siendo la única fuente dentro del modelo del menú.
// --------------------------------------------------------------------------

/// Fundido de cada bloque al abrir.
pub const BLOCK_FADE_MS: f64 = 180.0;
/// Desplazamiento vertical de entrada de cada bloque.
pub const BLOCK_OFFSET: f64 = 6.0;
/// Fundido del contenido al cerrar (`T_MICRO`).
pub const CONTENT_CLOSE_MS: f64 = T_MICRO as f64;
/// Retardo de la retracción de altura tras el fundido (A-05).
pub const CLOSE_HEIGHT_DELAY_MS: f64 = 60.0;
/// Animación del interruptor.
pub const TOGGLE_MS: f64 = 180.0;
/// Animación del scroll del menú.
pub const SCROLL_TWEEN_MS: f64 = 120.0;
/// Tiempo que el indicador de scroll queda visible.
pub const SCROLL_INDICATOR_HOLD_MS: f64 = 400.0;
/// Fundido del indicador de scroll.
pub const SCROLL_INDICATOR_FADE_MS: f64 = 400.0;
/// Subida del indicador de scroll.
pub const INDICATOR_SHOW_MS: f64 = 120.0;
/// Altura de una opción de la lista de un selector.
pub const LIST_ITEM_H: f64 = 32.0;
/// Opciones visibles de la lista de un selector.
pub const LIST_MAX_VISIBLE: usize = 5;
/// Alto visible de la lista de un selector.
pub const LIST_VIEW_H: f64 = LIST_ITEM_H * LIST_MAX_VISIBLE as f64;
/// Reposo del pulgar de un slider.
pub const SLIDER_THUMB_REST: f64 = 14.0;
/// Duración del retorno de la pulsación de un botón.
pub const BUTTON_RELEASE_MS: f64 = 120.0;
/// Fundido cruzado del pie (A-11).
pub const FOOTER_FADE_MS: f64 = 180.0;
/// Fundido del color del botón Guardar (A-10).
pub const SAVE_COLOR_MS: f64 = 180.0;
/// Fundido del botón «Probar».
pub const TEST_FADE_MS: f64 = 120.0;
/// Acciones que no son claves persistidas.
pub const ACTION_KEYS: [&str; 7] = [
    "test_voice",
    "open_config_dir",
    "open_system_md",
    "open_permissions",
    "open_voice_json",
    "repeat_last",
    "new_conversation",
];

fn tween_map<'a>(keys: impl IntoIterator<Item = &'a str>, value: f64) -> BTreeMap<String, Tween> {
    keys.into_iter()
        .map(|key| (key.to_string(), Tween::new(value)))
        .collect()
}

fn toggle_tweens(settings: &MenuSettings) -> BTreeMap<String, Tween> {
    TOGGLE_KEYS
        .iter()
        .map(|key| {
            let value = if settings.get(key).is_some_and(|value| value.truthy()) {
                1.0
            } else {
                0.0
            };
            ((*key).to_string(), Tween::new(value))
        })
        .collect()
}

struct TweenMaps {
    chevron: BTreeMap<String, Tween>,
    row_hover: BTreeMap<String, Tween>,
    button_hover: BTreeMap<String, Tween>,
    press: BTreeMap<String, Tween>,
    thumb: BTreeMap<String, Tween>,
    thumb_target: BTreeMap<String, f64>,
    slider_color: BTreeMap<String, Tween>,
    toggle_pos: BTreeMap<String, Tween>,
    toggle_color: BTreeMap<String, Tween>,
}

fn tween_maps(settings: &MenuSettings) -> TweenMaps {
    let row_keys = ALL_KEYS
        .iter()
        .copied()
        .chain(["input_meter", "theme_preview"])
        .chain(ACTION_KEYS);
    TweenMaps {
        chevron: tween_map(SELECT_KEYS, 0.0),
        row_hover: tween_map(row_keys, 0.0),
        button_hover: tween_map(["hide", "save", "test"], 0.0),
        press: tween_map(["hide", "save", "test"], 0.0),
        thumb: tween_map(SLIDER_KEYS, SLIDER_THUMB_REST),
        thumb_target: SLIDER_KEYS
            .iter()
            .map(|key| ((*key).to_string(), SLIDER_THUMB_REST))
            .collect(),
        slider_color: tween_map(SLIDER_KEYS, 0.0),
        toggle_pos: toggle_tweens(settings),
        toggle_color: toggle_tweens(settings),
    }
}

/// Contenido interactivo del menú desplegable del HUD.
///
/// `Default` deja un menú cerrado con la configuración vacía; se construye con
/// [`Menu::new`], que fija escala, tema, grupos y tweens.
#[derive(Default)]
pub struct Menu {
    pub settings: MenuSettings,
    pub scale: f64,
    pub theme: &'static str,
    pub screen_h: f64,
    pub available_h: Option<f64>,
    /// Ancho extra de la ventana de ajustes; lo anclado a la izquierda no se mueve.
    pub dx: f64,
    pub groups: MenuGroups,
    pub title: String,
    pub max_h: Option<f64>,
    pub callbacks: MenuCallbacks,

    pub capturing: bool,
    pub capture_mods: BTreeSet<String>,
    pub capture_msg: Option<(String, f64)>,
    pub copy_hover: Option<String>,
    pub copied: BTreeMap<String, f64>,
    pub palette_overrides: Option<PaletteProvider>,
    pub armed: BTreeMap<String, f64>,

    pub open: bool,
    pub closed_notified: bool,
    pub now: f64,
    pub height_tween: Tween,
    pub content_alpha: Tween,
    pub blocks: BTreeMap<String, Tween>,

    pub selector_key: Option<String>,
    pub selector_closing: bool,
    pub selector_extra: Tween,
    pub selector_item_start: f64,
    pub list_scroll: f64,
    pub list_scroll_target: f64,
    pub chevron: BTreeMap<String, Tween>,

    pub row_hover: BTreeMap<String, Tween>,
    pub button_hover: BTreeMap<String, Tween>,
    pub press: BTreeMap<String, Tween>,
    pub hover_id: Option<String>,
    pub hover_option: Option<usize>,
    pub pointer: Option<(f64, f64)>,
    pub pressed_hit: Option<MenuHit>,
    pub slider_drag: Option<MenuHit>,
    pub slider_last_emit: f64,
    pub slider_pending: Option<(String, SettingValue)>,
    pub slider_changed: bool,

    pub thumb: BTreeMap<String, Tween>,
    pub thumb_target: BTreeMap<String, f64>,
    pub slider_color: BTreeMap<String, Tween>,
    pub toggle_pos: BTreeMap<String, Tween>,
    pub toggle_color: BTreeMap<String, Tween>,

    pub editing_key: Option<String>,
    pub edit_buffer: String,
    pub edit_cursor: usize,
    pub edit_select_all: bool,
    pub edit_t0: f64,
    pub edit_error: Option<(String, String)>,

    pub meter_target: f64,
    pub meter_level: f64,
    pub meter_muted: bool,
    pub meter_active: bool,
    pub meter_t: f64,
    pub meter_drag: bool,
    pub meter_pending: Option<(String, SettingValue)>,
    pub meter_last_emit: f64,
    pub meter_changed: bool,

    pub scroll_tween: Tween,
    pub scroll_target: f64,
    pub indicator_last: f64,
    pub indicator_until: f64,

    pub check_until: f64,
    pub saved_flash_until: f64,
    pub error_until: f64,
    pub save_backup: Option<SettingsValues>,
    pub save_snapshot: Option<SettingsValues>,
    pub saving_pending: bool,
    pub testing_t: Tween,

    pub footer_state: Option<(String, String, bool)>,
    pub footer_prev: Option<(String, String, bool)>,
    pub footer_state_at: f64,
    pub save_variant: Option<String>,
    pub save_prev_fill: Option<Color>,
    pub save_prev_text: Option<Color>,
    pub save_variant_at: f64,

    pub reduced_motion: bool,

    /// `_layer_shape` del Python: alto, ancho y canales de las capas de pintura.
    pub layer_shape: Option<(usize, usize, usize)>,
    /// `_layers`: cabecera, contenido y pie (los reutiliza F5-10).
    pub layers: Option<[Layer; 3]>,
    /// `_layer_signature`: huella de la última pintura. F5-10 la calcula como
    /// hash de los valores que afectan a las capas (el Python guarda la tupla).
    pub layer_signature: Option<u64>,
}

fn clean_theme(theme: &str) -> &'static str {
    match theme {
        "auto" => "auto",
        "light" => "light",
        _ => "dark",
    }
}

impl Menu {
    /// `Menu(settings, scale=1.0, theme="dark", screen_h=1080.0, ...)`.
    ///
    /// Devuelve el texto del `ValueError` del Python si `scale` no es positivo.
    pub fn new(
        settings: MenuSettings,
        options: MenuOptions,
        callbacks: MenuCallbacks,
    ) -> Result<Self, String> {
        if options.scale <= 0.0 {
            return Err("scale debe ser mayor que cero".into());
        }
        let mut menu = Self {
            settings,
            scale: options.scale,
            theme: clean_theme(&options.theme),
            screen_h: options.screen_h.max(1.0),
            groups: default_groups(),
            title: "AJUSTES".into(),
            callbacks,
            closed_notified: true,
            slider_last_emit: -1e9,
            meter_last_emit: -1e9,
            indicator_last: -1e9,
            indicator_until: -1e9,
            footer_state_at: -1e9,
            save_variant_at: -1e9,
            ..Self::default()
        };
        menu.install_tweens();
        Ok(menu)
    }

    fn install_tweens(&mut self) {
        let tweens = tween_maps(&self.settings);
        self.blocks = block_delays()
            .into_keys()
            .map(|name| (name, Tween::new(0.0)))
            .collect();
        self.chevron = tweens.chevron;
        self.row_hover = tweens.row_hover;
        self.button_hover = tweens.button_hover;
        self.press = tweens.press;
        self.thumb = tweens.thumb;
        self.thumb_target = tweens.thumb_target;
        self.slider_color = tweens.slider_color;
        self.toggle_pos = tweens.toggle_pos;
        self.toggle_color = tweens.toggle_color;
    }

    /// Alto visible objetivo del cuerpo del menú, en px lógicos.
    pub fn viewport_height(&self) -> f64 {
        let height = self
            .max_h
            .unwrap_or_else(|| layout::visible_menu_height(self.screen_h));
        let height = match self.available_h {
            Some(available) => height.min(available.max(1.0)),
            None => height,
        };
        height.max(1.0)
    }

    /// Grupos visibles: las filas del tema personalizado solo con «Personalizado».
    pub fn shown_groups(&self) -> MenuGroups {
        let custom = self.settings.get("theme").and_then(|value| value.as_str()) == Some("custom");
        let advanced = self
            .settings
            .get("show_advanced")
            .is_some_and(|value| value.truthy());
        let mut hidden: BTreeSet<&str> = BTreeSet::new();
        if !custom {
            hidden.extend(layout::CUSTOM_THEME_ROWS);
        }
        if !advanced {
            hidden.extend(layout::ADVANCED_ROWS);
        }
        let shown: MenuGroups = self
            .groups
            .iter()
            .map(|(name, rows)| {
                let rows = rows
                    .iter()
                    .filter(|(key, _kind)| !hidden.contains(key.as_str()))
                    .cloned()
                    .collect();
                (name.clone(), rows)
            })
            .collect();
        let filtered: MenuGroups = shown
            .iter()
            .filter(|(_name, rows)| !rows.is_empty())
            .cloned()
            .collect();
        if filtered.is_empty() {
            shown
        } else {
            filtered
        }
    }

    /// Ancho lógico del menú (`None` o menos de 320 = el de la tarjeta).
    pub fn set_width(&mut self, width: Option<f64>) {
        self.dx = match width {
            None => 0.0,
            Some(width) => (width - W as f64).max(0.0),
        };
    }

    /// Ancho actual del menú.
    pub fn width(&self) -> f64 {
        W as f64 + self.dx
    }

    /// Cambia el ancho y el alto visibles al instante (ventana redimensionada).
    pub fn resize(&mut self, width: f64, height: f64, now: f64) {
        self.set_width(Some(width));
        self.set_max_height(Some(height));
        if self.open {
            self.height_tween
                .to(self.viewport_height(), now, 0.0, EASE_OUT);
        }
    }

    /// Alto máximo propio (ventana de ajustes); `None` vuelve al de la tarjeta.
    pub fn set_max_height(&mut self, height: Option<f64>) {
        self.max_h = height.map(|height| height.max(160.0));
    }

    /// Muestra solo `groups` (una sección de la ventana de ajustes).
    ///
    /// `title` vacío deja la cabecera solo con «–»; `None` vuelve a «AJUSTES».
    pub fn set_groups(&mut self, groups: Vec<MenuGroup>, title: Option<&str>, now: Option<f64>) {
        let moment = now.unwrap_or(self.now);
        self.cancel_text_edit_quiet();
        self.groups = if groups.is_empty() {
            default_groups()
        } else {
            groups
        };
        self.title = title.map_or_else(|| "AJUSTES".to_string(), str::to_string);
        self.selector_key = None;
        self.selector_closing = false;
        self.selector_extra.to(0.0, moment, 0.0, EASE_OUT);
        self.list_scroll = 0.0;
        self.list_scroll_target = 0.0;
        self.scroll_tween.to(0.0, moment, 0.0, EASE_OUT);
        self.scroll_target = 0.0;
    }

    /// Nombres de los grupos visibles.
    pub fn groups(&self) -> Vec<&str> {
        self.groups
            .iter()
            .map(|(name, _rows)| name.as_str())
            .collect()
    }

    /// Limita el menú al espacio real arriba/abajo de la barra flotante.
    pub fn set_available_height(&mut self, height: Option<f64>) {
        self.available_h = height.map(|height| height.max(1.0));
    }

    /// Intención lógica: `false` en cuanto se pide cerrar.
    pub fn is_open(&self) -> bool {
        self.open
    }
}
