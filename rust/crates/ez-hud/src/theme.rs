//! Port of `src/ezagent/voice/hud/bar_theme.py, theme_accent.py` (F2-12).
//!
//! Colores compartidos por la barra y su tarjeta de ajustes, más el tema
//! personalizado: la paleta elegida tiñe también chat, ajustes, botones y
//! avisos. La paleta Marea de `tokens` se re-tiñe, no se reemplaza, para que el
//! contraste y la jerarquía sobrevivan a cualquier preset.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::tokens::{self, Color, ColorMap, ThemeOverrides};

/// Más redonda que la tarjeta antigua de 18 px: más suave, más orgánica.
pub const CARD_RADIUS: i32 = 22;

/// Arriba del degradado del cuerpo de la tarjeta (abajo es `BODY`).
pub static BODY_TOP: LazyLock<BTreeMap<&'static str, Color>> = LazyLock::new(|| {
    BTreeMap::from([
        ("light", Color::Rgb([238, 249, 246])),
        ("dark", Color::Rgb([17, 30, 34])),
    ])
});

/// Claves de superficie que el tema personalizado re-tiñe.
pub const SURFACE_KEYS: [&str; 7] = [
    "BODY",
    "GROUP",
    "GROUP_HOVER",
    "TRACK",
    "TEXT",
    "TEXT_2",
    "TEXT_3",
];
/// Claves de líneas decorativas (llevan alfa).
pub const LINE_KEYS: [&str; 2] = ["LINE", "LINE_2"];
/// Claves del acento.
pub const ACCENT_KEYS: [&str; 5] = ["MINT", "MINT_BTN", "WORD_MINT", "EYE_IDLE", "WORD_IDLE"];

/// Texto de acento y su tinta sobre botones.
pub const MIN_CONTRAST: f64 = 4.5;
/// En oscuro el acento debe brillar, no solo leerse.
pub const DARK_ACCENT_CONTRAST: f64 = 6.0;
/// Luminosidad desde la que se busca: el acento más vivo que se lee.
pub const ACCENT_START: f64 = 0.62;

/// `"light"` si el tema es claro; cualquier otro valor cae a oscuro.
fn theme_name(theme: &str) -> &'static str {
    if theme == "light" {
        "light"
    } else {
        "dark"
    }
}

/// Claves que reemplazan la paleta de tokens en la tarjeta para `theme`.
pub fn card_overrides(theme: &str) -> ColorMap {
    // El tema ya viene normalizado: `palette` no puede fallar aquí.
    tokens::palette(theme_name(theme)).expect("tema normalizado")
}

/// Paleta semántica compartida, copiada para que los cambios locales no salgan.
pub fn card_palette(theme: &str) -> ColorMap {
    card_overrides(theme)
}

/// Arriba del degradado del cuerpo; con el tema personalizado, en su tono.
pub fn body_top(theme: &str) -> Color {
    let name = theme_name(theme);
    tokens::palette(name)
        .expect("tema normalizado")
        .get("BODY_TOP")
        .copied()
        .unwrap_or(BODY_TOP[name])
}

/// `numpy` y `str` no: el espacio de `str.strip()` de Python.
fn is_py_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

/// `int(text, 16)`: signo opcional y guiones bajos entre dígitos.
fn int_hex(text: &str) -> Option<i64> {
    let text = text.trim_matches(is_py_space);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let mut value: i64 = 0;
    let mut has_digit = false;
    let mut after_digit = false;
    for character in digits.chars() {
        if character == '_' {
            if !after_digit {
                return None;
            }
            after_digit = false;
            continue;
        }
        let digit = character.to_digit(16)? as i64;
        value = value.checked_mul(16)?.checked_add(digit)?;
        has_digit = true;
        after_digit = true;
    }
    if !has_digit || !after_digit {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// `#RRGGBB` (o `RRGGBB`) -> canales; `None` si no vale.
pub fn parse_hex(value: &str) -> Option<[i64; 3]> {
    let text = value.trim_matches(is_py_space).trim_start_matches('#');
    if text.chars().count() != 6 {
        return None;
    }
    let mut bounds = [0usize; 6];
    for (slot, (index, _)) in bounds.iter_mut().zip(text.char_indices()) {
        *slot = index;
    }
    Some([
        int_hex(&text[bounds[0]..bounds[2]])?,
        int_hex(&text[bounds[2]..bounds[4]])?,
        int_hex(&text[bounds[4]..])?,
    ])
}

// ------------------------------------------------------------------ colorsys
fn rgb_to_hls(rgb: [i64; 3]) -> (f64, f64, f64) {
    let red = rgb[0] as f64 / 255.0;
    let green = rgb[1] as f64 / 255.0;
    let blue = rgb[2] as f64 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let sum = max + min;
    let range = max - min;
    let light = sum / 2.0;
    if min == max {
        return (0.0, light, 0.0);
    }
    let sat = if light <= 0.5 {
        range / sum
    } else {
        range / (2.0 - max - min)
    };
    let rc = (max - red) / range;
    let gc = (max - green) / range;
    let bc = (max - blue) / range;
    let hue = if red == max {
        bc - gc
    } else if green == max {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((hue / 6.0).rem_euclid(1.0), light, sat)
}

fn one_third_value(m1: f64, m2: f64, hue: f64) -> f64 {
    let hue = hue.rem_euclid(1.0);
    if hue < 1.0 / 6.0 {
        m1 + (m2 - m1) * hue * 6.0
    } else if hue < 0.5 {
        m2
    } else if hue < 2.0 / 3.0 {
        m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0
    } else {
        m1
    }
}

fn hls_to_rgb(hue: f64, light: f64, sat: f64) -> (f64, f64, f64) {
    if sat == 0.0 {
        return (light, light, light);
    }
    let m2 = if light <= 0.5 {
        light * (1.0 + sat)
    } else {
        light + sat - (light * sat)
    };
    let m1 = 2.0 * light - m2;
    (
        one_third_value(m1, m2, hue + 1.0 / 3.0),
        one_third_value(m1, m2, hue),
        one_third_value(m1, m2, hue - 1.0 / 3.0),
    )
}

fn channel_byte(value: f64) -> u8 {
    value.clamp(0.0, 255.0).round_ties_even() as u8
}

fn rgb_from_hls(hue: f64, light: f64, sat: f64) -> Color {
    let (red, green, blue) = hls_to_rgb(hue, light.clamp(0.0, 1.0), sat.clamp(0.0, 1.0));
    Color::Rgb([
        channel_byte(red * 255.0),
        channel_byte(green * 255.0),
        channel_byte(blue * 255.0),
    ])
}

/// El mismo color con otro tono: conserva luminosidad, saturación y alfa.
fn rehue(color: Color, hue: f64) -> Color {
    let channels = color.channels();
    let (_, light, sat) = rgb_to_hls([channels[0] as i64, channels[1] as i64, channels[2] as i64]);
    rgb_from_hls(hue, light, sat).with_alpha_of(color)
}

// ----------------------------------------------------------------- contraste
fn linear_channel(channel: u8) -> f64 {
    let c = channel as f64 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Luminancia relativa WCAG de un color RGB 0..255.
pub fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * linear_channel(rgb[0])
        + 0.7152 * linear_channel(rgb[1])
        + 0.0722 * linear_channel(rgb[2])
}

/// Relación de contraste WCAG entre dos colores RGB (sin alfa).
pub fn contrast_rgb(a: [u8; 3], b: [u8; 3]) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    let hi = la.max(lb);
    let lo = la.min(lb);
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG contrast ratio between two colours.
pub fn contrast(a: Color, b: Color) -> f64 {
    contrast_rgb(a.channels(), b.channels())
}

/// Acento con `hue`/`sat`: desde `start`, el primero que llega a `target` sobre
/// `against`.
fn readable(hue: f64, sat: f64, start: f64, against: Color, darker: bool, target: f64) -> Color {
    let mut light = start;
    for _ in 0..60 {
        let color = rgb_from_hls(hue, light, sat);
        if contrast_rgb(color.channels(), against.channels()) >= target {
            return color;
        }
        light += if darker { -0.01 } else { 0.01 };
    }
    rgb_from_hls(hue, if darker { 0.15 } else { 0.85 }, sat)
}

/// Sobrescrituras de la paleta para `light` y `dark`; None si los colores no valen.
pub fn derive(primary: &str, background: &str) -> Option<ThemeOverrides> {
    let main = parse_hex(primary)?;
    let ground = parse_hex(background)?;
    let (accent_hue, _, accent_sat) = rgb_to_hls(main);
    let surface_hue = rgb_to_hls(ground).0;
    let accent_sat = accent_sat.clamp(0.45, 0.85);
    let mut out = ThemeOverrides::new();
    for (theme, base, dark) in [
        ("light", &*tokens::LIGHT, false),
        ("dark", &*tokens::DARK, true),
    ] {
        let mut pal = ColorMap::new();
        for key in SURFACE_KEYS.iter().chain(LINE_KEYS.iter()) {
            if let Some(original) = base.get(*key) {
                pal.insert(*key, rehue(*original, surface_hue));
            }
        }
        let target = if dark {
            DARK_ACCENT_CONTRAST
        } else {
            MIN_CONTRAST
        };
        let body = pal.get("BODY").copied().unwrap_or(Color::Rgb([0, 0, 0]));
        let accent = readable(accent_hue, accent_sat, ACCENT_START, body, !dark, target);
        for key in ACCENT_KEYS {
            pal.insert(key, accent);
        }
        if dark {
            pal.insert(
                "MINT_INK",
                readable(accent_hue, 0.6, 0.12, accent, true, MIN_CONTRAST),
            );
        } else {
            pal.insert("MINT_INK", Color::Rgb([255, 255, 255]));
        }
        pal.insert("BODY_TOP", rehue(BODY_TOP[theme], surface_hue));
        out.insert(theme, pal);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{set_themed, test_guard};

    #[test]
    fn hex_parsing_trims_and_rejects_bad_values() {
        assert_eq!(parse_hex("#7A6FF0"), Some([122, 111, 240]));
        assert_eq!(parse_hex("  7a6ff0 \n"), Some([122, 111, 240]));
        assert_eq!(parse_hex("##00ff7f"), Some([0, 255, 127]));
        assert_eq!(parse_hex("#000000"), Some([0, 0, 0]));
        assert_eq!(parse_hex("no-es-hex"), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#GGGGGG"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn contrast_matches_the_wcag_formula() {
        let black = Color::Rgb([0, 0, 0]);
        let white = Color::Rgb([255, 255, 255]);
        assert!((contrast(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast(white, black) - 21.0).abs() < 1e-9);
        assert_eq!(contrast(black, black), 1.0);
        let dark_text = tokens::DARK["TEXT"];
        let dark_body = tokens::DARK["BODY"];
        assert!(contrast(dark_text, dark_body) > 7.0);
    }

    #[test]
    fn card_palette_is_the_normalized_token_palette() {
        let _guard = test_guard();
        set_themed(None);
        let dark = card_palette("custom");
        assert_eq!(dark["BODY"], tokens::DARK["BODY"]);
        assert_eq!(dark["LINE"], tokens::DARK["LINE"]);
        assert_eq!(card_overrides("light")["BODY"], tokens::LIGHT["BODY"]);
        assert_eq!(body_top("dark"), Color::Rgb([17, 30, 34]));
        assert_eq!(body_top("light"), Color::Rgb([238, 249, 246]));
    }

    #[test]
    fn a_custom_theme_retints_surfaces_and_keeps_accents_readable() {
        let _guard = test_guard();
        set_themed(None);
        let derived = derive("#7A6FF0", "#2E2A6B").expect("lavanda");
        let dark = &derived["dark"];
        let light = &derived["light"];
        let mint = dark["MINT"];
        assert!(dark.contains_key("BODY_TOP"));
        assert!(!dark.contains_key("WARM") && !dark.contains_key("ERR"));
        assert!(contrast_rgb(mint.channels(), dark["BODY"].channels()) >= DARK_ACCENT_CONTRAST);
        assert!(
            contrast_rgb(dark["MINT_INK"].channels(), dark["MINT_BTN"].channels()) >= MIN_CONTRAST
        );
        assert!(contrast_rgb(light["MINT"].channels(), light["BODY"].channels()) >= MIN_CONTRAST);
        assert_eq!(light["MINT_INK"], Color::Rgb([255, 255, 255]));
        let accent_hue = rgb_to_hls([0x7A, 0x6F, 0xF0]).0;
        assert!((rgb_to_hls(to_int(mint)).0 - accent_hue).abs() < 0.02);
        let surface_hue = rgb_to_hls([0x2E, 0x2A, 0x6B]).0;
        assert!((rgb_to_hls(to_int(dark["GROUP"])).0 - surface_hue).abs() < 0.03);
    }

    #[test]
    fn bad_colours_leave_the_factory_palette() {
        assert_eq!(derive("no-es-hex", "#000000"), None);
        assert_eq!(derive("#7A6FF0", "zzzzzz"), None);
    }

    #[test]
    fn themed_palette_reaches_the_card_and_the_body_top() {
        let _guard = test_guard();
        set_themed(None);
        set_themed(derive("#7A6FF0", "#2E2A6B").as_ref());
        assert_eq!(
            card_palette("dark")["BODY"],
            tokens::palette("dark").expect("oscuro")["BODY"]
        );
        assert_ne!(body_top("dark"), Color::Rgb([17, 30, 34]));
        set_themed(None);
    }

    fn to_int(color: Color) -> [i64; 3] {
        let channels = color.channels();
        [channels[0] as i64, channels[1] as i64, channels[2] as i64]
    }
}
