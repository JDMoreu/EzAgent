//! Port of `src/ezagent/voice/hud/tokens.py` (F2-12).
//!
//! Única fuente de verdad para colores, medidas, tipografía, movimiento y
//! coordenadas de la base visual del HUD. Ningún otro módulo de la interfaz
//! debe contener valores visuales literales.
//!
//! Convenciones:
//! - Todas las medidas están en píxeles lógicos a 96 DPI. Al dibujar se
//!   multiplican por `scale = dpi / 96`.
//! - Los colores son RGB 0-255; `LINE` y `LINE_2` son RGBA porque incluyen alfa.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::LazyLock;

use parking_lot::RwLock;

/// `RGB = tuple[int, int, int]`.
pub type Rgb = [u8; 3];
/// `RGBA = tuple[int, int, int, int]`.
pub type Rgba = [u8; 4];

/// `Color = RGB | RGBA`: el Python distingue por longitud de la tupla.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    Rgb(Rgb),
    Rgba(Rgba),
}

impl Color {
    /// Los tres canales (`color[:3]`).
    pub fn channels(self) -> Rgb {
        match self {
            Self::Rgb(rgb) => rgb,
            Self::Rgba(rgba) => [rgba[0], rgba[1], rgba[2]],
        }
    }

    /// `color[3]` cuando el color lleva alfa.
    pub fn alpha(self) -> Option<u8> {
        match self {
            Self::Rgb(_) => None,
            Self::Rgba(rgba) => Some(rgba[3]),
        }
    }

    /// `len(color)`: 3 o 4.
    pub fn channel_count(self) -> usize {
        match self {
            Self::Rgb(_) => 3,
            Self::Rgba(_) => 4,
        }
    }

    /// Tupla de este color con el alfa de otro (`(*out, color[3])`).
    pub fn with_alpha_of(self, other: Self) -> Self {
        match (self, other.alpha()) {
            (Self::Rgb(rgb), Some(alpha)) => Self::Rgba([rgb[0], rgb[1], rgb[2], alpha]),
            _ => self,
        }
    }
}

impl From<Rgb> for Color {
    fn from(rgb: Rgb) -> Self {
        Self::Rgb(rgb)
    }
}

impl From<Rgba> for Color {
    fn from(rgba: Rgba) -> Self {
        Self::Rgba(rgba)
    }
}

/// `dict[str, Color]`: los `dict` de Python conservan el orden de inserción y
/// comparan sin orden, igual que un `BTreeMap`.
pub type ColorMap = BTreeMap<&'static str, Color>;

/// `dict[str, dict[str, Color]]`: sobrescrituras por tema.
pub type ThemeOverrides = BTreeMap<&'static str, ColorMap>;

fn rgb(values: [u8; 3]) -> Color {
    Color::Rgb(values)
}

fn rgba(values: [u8; 4]) -> Color {
    Color::Rgba(values)
}

// DECISION: retain the shipped Marea identity as the shared palette for all surfaces.
/// Paleta oscura de fábrica.
pub static DARK: LazyLock<ColorMap> = LazyLock::new(|| {
    ColorMap::from([
        ("BODY", rgb([13, 22, 26])),
        ("GROUP", rgb([20, 33, 38])),
        ("GROUP_HOVER", rgb([28, 45, 51])),
        ("TRACK", rgb([44, 70, 76])),
        ("TEXT", rgb([228, 242, 240])),
        ("TEXT_2", rgb([172, 196, 194])),
        ("TEXT_3", rgb([138, 164, 163])),
        ("LINE", rgba([228, 242, 240, 24])),
        ("LINE_2", rgba([228, 242, 240, 15])),
        ("MINT", rgb([94, 226, 196])),
        ("WARM", rgb([255, 178, 62])),
        ("ERR", rgb([255, 93, 82])),
        ("MINT_BTN", rgb([94, 226, 196])),
        ("MINT_INK", rgb([6, 34, 30])),
        ("WARM_BTN", rgb([255, 178, 62])),
        ("WARM_INK", rgb([26, 16, 0])),
        ("ERR_INK", rgb([13, 22, 26])),
        ("EYE_IDLE", rgb([94, 174, 155])),
        ("EYE_MUTE", rgb([183, 140, 58])),
        ("WORD_IDLE", rgb([94, 174, 155])),
        ("WORD_MINT", rgb([94, 226, 196])),
        ("WORD_WARM", rgb([255, 178, 62])),
        ("WORD_MUTE", rgb([183, 140, 58])),
        ("WORD_ERR", rgb([255, 93, 82])),
    ])
});

/// Paleta clara de fábrica.
pub static LIGHT: LazyLock<ColorMap> = LazyLock::new(|| {
    ColorMap::from([
        ("BODY", rgb([250, 253, 252])),
        ("GROUP", rgb([237, 246, 244])),
        ("GROUP_HOVER", rgb([224, 239, 236])),
        ("TRACK", rgb([199, 224, 219])),
        ("TEXT", rgb([16, 34, 38])),
        ("TEXT_2", rgb([56, 82, 86])),
        ("TEXT_3", rgb([78, 102, 106])),
        ("LINE", rgba([16, 34, 38, 20])),
        ("LINE_2", rgba([16, 34, 38, 15])),
        ("MINT", rgb([12, 112, 117])),
        ("WARM", rgb([163, 95, 0])),
        ("ERR", rgb([200, 36, 26])),
        ("MINT_BTN", rgb([12, 112, 117])),
        ("MINT_INK", rgb([255, 255, 255])),
        ("WARM_BTN", rgb([163, 95, 0])),
        ("WARM_INK", rgb([255, 255, 255])),
        ("ERR_INK", rgb([255, 255, 255])),
        ("EYE_IDLE", rgb([12, 112, 117])),
        ("EYE_MUTE", rgb([163, 95, 0])),
        ("WORD_IDLE", rgb([12, 112, 117])),
        ("WORD_MINT", rgb([12, 112, 117])),
        ("WORD_WARM", rgb([163, 95, 0])),
        ("WORD_MUTE", rgb([163, 95, 0])),
        ("WORD_ERR", rgb([200, 36, 26])),
    ])
});

/// `tuple(DARK)`: las claves de la paleta en su orden de inserción.
pub const COLOR_KEYS: [&str; 24] = [
    "BODY",
    "GROUP",
    "GROUP_HOVER",
    "TRACK",
    "TEXT",
    "TEXT_2",
    "TEXT_3",
    "LINE",
    "LINE_2",
    "MINT",
    "WARM",
    "ERR",
    "MINT_BTN",
    "MINT_INK",
    "WARM_BTN",
    "WARM_INK",
    "ERR_INK",
    "EYE_IDLE",
    "EYE_MUTE",
    "WORD_IDLE",
    "WORD_MINT",
    "WORD_WARM",
    "WORD_MUTE",
    "WORD_ERR",
];

/// Alfa máximo del glow aditivo de los ojos en oscuro.
pub const GLOW_MAX_DARK: f64 = 0.10;
/// Alfa máximo del glow aditivo de los ojos en claro.
pub const GLOW_MAX_LIGHT: f64 = 0.0;

/// Tema personalizado (`theme_accent`): sobrescrituras por tema y su versión.
///
/// `parking_lot::RwLock` sustituye a la reasignación sin lock del Python (que
/// se apoya en el GIL); la versión entra en las claves de caché de quien dibuja
/// con la paleta.
static THEMED: RwLock<ThemeOverrides> = RwLock::new(BTreeMap::new());
static VERSION: AtomicI64 = AtomicI64::new(0);

/// `repr()` de una cadena para los mensajes de error de Python.
fn py_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Devuelve la paleta del tema.
///
/// `auto` se resuelve en la capa de configuración; si llega aquí sin resolver
/// se asume oscuro, que es el tema base del HUD.
pub fn palette(theme: &str) -> Result<ColorMap, String> {
    // DECISION: `auto` no se consulta al registro desde este módulo puro; la
    // resolución pertenece a la capa de configuración. Aquí cae a oscuro.
    let (base, resolved): (&ColorMap, &'static str) = match theme {
        "light" => (&LIGHT, "light"),
        "dark" | "auto" => (&DARK, "dark"),
        other => return Err(format!("tema desconocido: {}", py_repr(other))),
    };
    let themed = THEMED.read();
    match themed.get(resolved) {
        None => Ok(base.clone()),
        Some(over) => {
            let mut merged = base.clone();
            merged.extend(over.iter().map(|(key, color)| (*key, *color)));
            Ok(merged)
        }
    }
}

/// Activa (o quita, con `None`) la paleta derivada del tema personalizado.
pub fn set_themed(overrides: Option<&ThemeOverrides>) {
    let new = overrides.cloned().unwrap_or_default();
    let mut themed = THEMED.write();
    if *themed != new {
        *themed = new;
        VERSION.fetch_add(1, Ordering::SeqCst);
    }
}

/// Sube cada vez que cambia la paleta: úsese en las claves de caché.
pub fn palette_version() -> i64 {
    VERSION.load(Ordering::SeqCst)
}

/// Alfa máximo del glow aditivo de los ojos (spec 3.4).
pub fn glow_max(theme: &str) -> f64 {
    if theme == "light" {
        GLOW_MAX_LIGHT
    } else {
        GLOW_MAX_DARK
    }
}

// --------------------------------------------------------------------------
// Tipografía (spec 3.2)
// --------------------------------------------------------------------------

/// Estilo tipográfico cerrado del HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontStyle {
    pub token: &'static str,
    pub family: &'static str,
    pub size: f64,
    pub weight: i32,
    pub tracking_em: f64,
    pub uppercase: bool,
}

impl FontStyle {
    /// Estilo sin tracking ni versalitas, como el constructor del dataclass.
    pub const fn new(token: &'static str, family: &'static str, size: f64, weight: i32) -> Self {
        Self {
            token,
            family,
            size,
            weight,
            tracking_em: 0.0,
            uppercase: false,
        }
    }

    pub fn mono(&self) -> bool {
        self.family == "GeistMono"
    }
}

pub const FS_TITLE: FontStyle = FontStyle::new("FS_TITLE", "Geist", 16.0, 600);
pub const FS_ROW: FontStyle = FontStyle::new("FS_ROW", "Geist", 14.0, 400);
pub const FS_BODY: FontStyle = FontStyle::new("FS_BODY", "Geist", 13.0, 400);
pub const FS_SECONDARY: FontStyle = FontStyle::new("FS_SECONDARY", "Geist", 12.0, 600);
pub const FS_MICRO: FontStyle = FontStyle {
    token: "FS_MICRO",
    family: "GeistMono",
    size: 11.0,
    weight: 500,
    tracking_em: 0.10,
    uppercase: true,
};

/// `FONT_STYLES`: los cinco estilos por su token.
pub const FONT_STYLES: [(&str, FontStyle); 5] = [
    ("FS_TITLE", FS_TITLE),
    ("FS_ROW", FS_ROW),
    ("FS_BODY", FS_BODY),
    ("FS_SECONDARY", FS_SECONDARY),
    ("FS_MICRO", FS_MICRO),
];

/// `FONT_STYLES[key]`, con `KeyError` convertido en `None`.
pub fn font_style(token: &str) -> Option<FontStyle> {
    FONT_STYLES
        .iter()
        .find(|(key, _)| *key == token)
        .map(|(_, style)| *style)
}

// DECISION: la ruta se resuelve desde el crate en tiempo de compilación, igual
// que `Path(__file__).resolve().parents[1] / "assets" / "fonts"` en el Python.
/// `FONT_DIR`: carpeta de las fuentes Geist del proyecto.
pub static FONT_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/ezagent/voice/assets/fonts")
});

/// `FONT_FILES`: archivo de cada familia y peso.
pub static FONT_FILES: LazyLock<BTreeMap<&'static str, BTreeMap<i32, PathBuf>>> =
    LazyLock::new(|| {
        let geist = BTreeMap::from([
            (400, FONT_DIR.join("Geist-Regular.ttf")),
            (500, FONT_DIR.join("Geist-Medium.ttf")),
            (600, FONT_DIR.join("Geist-SemiBold.ttf")),
        ]);
        let mono = BTreeMap::from([
            (400, FONT_DIR.join("GeistMono-Regular.ttf")),
            (500, FONT_DIR.join("GeistMono-Medium.ttf")),
            (600, FONT_DIR.join("GeistMono-Medium.ttf")),
        ]);
        BTreeMap::from([("Geist", geist), ("GeistMono", mono)])
    });

/// `FONT_FALLBACKS`: respaldos del sistema si faltan las fuentes Geist.
///
/// DECISION: el proyecto incluye Geist; los respaldos solo cubren una
/// instalación incompleta y el cargador escoge el primero que exista.
pub static FONT_FALLBACKS: LazyLock<BTreeMap<&'static str, BTreeMap<i32, Vec<PathBuf>>>> =
    LazyLock::new(|| {
        let segoe = || PathBuf::from("C:/Windows/Fonts/segoeui.ttf");
        let semibold = || PathBuf::from("C:/Windows/Fonts/seguisb.ttf");
        let consola = || PathBuf::from("C:/Windows/Fonts/consola.ttf");
        let geist = BTreeMap::from([
            (400, vec![segoe()]),
            (500, vec![segoe()]),
            (600, vec![semibold(), segoe()]),
        ]);
        let mono = BTreeMap::from([
            (400, vec![consola()]),
            (500, vec![consola()]),
            (600, vec![consola()]),
        ]);
        BTreeMap::from([("Geist", geist), ("GeistMono", mono)])
    });

// --------------------------------------------------------------------------
// Espaciado y radios (spec 3.3)
// --------------------------------------------------------------------------

pub const SP_1: i32 = 2;
pub const SP_2: i32 = 4;
pub const SP_3: i32 = 6;
pub const SP_4: i32 = 8;
pub const SP_5: i32 = 12;
pub const SP_6: i32 = 16;
pub const SP_7: i32 = 20;
pub const SP_8: i32 = 24;

pub const R_MENU: i32 = 18;
pub const R_GROUP: i32 = 12;
pub const FLARE: i32 = 7;

// --------------------------------------------------------------------------
// Geometría del menú
// --------------------------------------------------------------------------

pub const W: i32 = 320;
pub const H_ISLAND: i32 = 36;
pub const BODY_X0: f64 = FLARE as f64;
pub const BODY_X1: f64 = (W - FLARE) as f64;

/// Alto de referencia del menú completo (spec 7.3).
pub const H_MENU: i32 = 560;

/// Rampa anti-alias de 1 px lógico (spec 4.4).
pub const AA_RAMP: f64 = 1.0;

/// Rectángulo en píxeles lógicos, origen en la ventana.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    /// True si ambos rectángulos comparten área positiva.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }
}

// --------------------------------------------------------------------------
// Movimiento (spec 3.5)
// --------------------------------------------------------------------------

pub const T_MICRO: i32 = 120;
pub const T_FAST: i32 = 180;
pub const T_BASE: i32 = 240;
pub const T_SLOW: i32 = 320;
pub const T_CLOSE_RETRACT: i32 = 260;

pub const VOICE_ATTACK: f64 = 45.0;
pub const VOICE_RELEASE: f64 = 200.0;
pub const FPS_ACTIVE: i32 = 60;
pub const FPS_IDLE: i32 = 30;

/// Puntos de control CSS `cubic-bezier(x1, y1, x2, y2)`.
pub const BEZIER_OUT: (f64, f64, f64, f64) = (0.2, 0.8, 0.2, 1.0);
pub const BEZIER_IN: (f64, f64, f64, f64) = (0.5, 0.0, 0.9, 0.4);
pub const BEZIER_INOUT: (f64, f64, f64, f64) = (0.4, 0.0, 0.2, 1.0);
pub const BEZIER_SPRING: (f64, f64, f64, f64) = (0.34, 1.56, 0.64, 1.0);

// --------------------------------------------------------------------------
// Iconos (spec 6.3)
// --------------------------------------------------------------------------

pub const ICON_GRID: f64 = 24.0;
pub const ICON_STROKE: f64 = 1.8;
pub const ICON_STROKE_SMALL: f64 = 1.6;
pub const ICON_SMALL_THRESHOLD: f64 = 18.0;
/// Supermuestreo interno del rasterizador vectorial.
pub const ICON_SSAA: i32 = 4;

// Los tests tocan la paleta global: se serializan para no verse entre sí.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_palettes_keep_the_shipped_values() {
        assert_eq!(DARK["BODY"], Color::Rgb([13, 22, 26]));
        assert_eq!(LIGHT["BODY"], Color::Rgb([250, 253, 252]));
        assert_eq!(DARK["LINE"], Color::Rgba([228, 242, 240, 24]));
        assert_eq!(COLOR_KEYS.len(), DARK.len());
        assert_eq!(COLOR_KEYS[0], "BODY");
    }

    #[test]
    fn palette_resolves_auto_to_dark_and_rejects_unknown_themes() {
        let _guard = test_guard();
        let auto = palette("auto").expect("auto cae a oscuro");
        assert_eq!(auto["MINT"], Color::Rgb([94, 226, 196]));
        assert_eq!(palette("dark").expect("oscuro")["BODY"], auto["BODY"]);
        assert_eq!(
            palette("claro").expect_err("tema desconocido"),
            "tema desconocido: 'claro'"
        );
    }

    #[test]
    fn themed_overrides_merge_and_bump_the_version_once() {
        let _guard = test_guard();
        set_themed(None);
        let start = palette_version();
        let factory = palette("dark").expect("oscuro")["MINT"];
        let mut dark = ColorMap::new();
        dark.insert("MINT", Color::Rgb([1, 2, 3]));
        let themed = ThemeOverrides::from([("dark", dark)]);
        set_themed(Some(&themed));
        assert_eq!(palette_version(), start + 1);
        let merged = palette("dark").expect("oscuro");
        assert_eq!(merged["MINT"], Color::Rgb([1, 2, 3]));
        assert_eq!(merged["ERR"], DARK["ERR"]);
        assert_eq!(factory, Color::Rgb([94, 226, 196]));
        set_themed(Some(&themed));
        assert_eq!(palette_version(), start + 1);
        set_themed(None);
        assert_eq!(palette_version(), start + 2);
        assert_eq!(palette("dark").expect("oscuro")["MINT"], factory);
    }

    #[test]
    fn glow_and_styles_match_the_specification() {
        assert_eq!(glow_max("light"), 0.0);
        assert_eq!(glow_max("dark"), 0.10);
        assert_eq!(glow_max("auto"), 0.10);
        assert!(font_style("FS_MICRO").expect("micro").mono());
        assert!(!font_style("FS_TITLE").expect("título").mono());
        assert_eq!(font_style("nope"), None);
        assert!(FONT_FILES["Geist"][&600].ends_with("Geist-SemiBold.ttf"));
        assert_eq!(FONT_FALLBACKS["Geist"][&600].len(), 2);
    }

    #[test]
    fn rects_measure_and_overlap_like_the_dataclass() {
        let rect = Rect::new(0.0, 10.0, 20.0, 30.0);
        assert_eq!(rect.width(), 20.0);
        assert_eq!(rect.height(), 20.0);
        assert_eq!(rect.center(), (10.0, 20.0));
        assert!(rect.overlaps(&Rect::new(19.0, 29.0, 40.0, 40.0)));
        assert!(!rect.overlaps(&Rect::new(20.0, 10.0, 40.0, 40.0)));
    }

    #[test]
    fn colors_keep_their_python_length_and_alpha() {
        assert_eq!(Color::Rgb([1, 2, 3]).channel_count(), 3);
        assert_eq!(Color::Rgba([1, 2, 3, 4]).channel_count(), 4);
        assert_eq!(Color::Rgba([1, 2, 3, 4]).channels(), [1, 2, 3]);
        assert_eq!(
            Color::Rgb([9, 9, 9]).with_alpha_of(Color::Rgba([0, 0, 0, 7])),
            Color::Rgba([9, 9, 9, 7])
        );
    }
}
