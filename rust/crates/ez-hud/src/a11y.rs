//! Port of `src/ezagent/voice/hud/a11y.py` (P-HUD-A).
//!
//! Contraste y accesibilidad de la paleta. Las fórmulas de luminancia y
//! contraste son las de la especificación (12.7). El validador separa dos
//! niveles:
//!
//! - **Obligatorio**: texto principal/secundario sobre cuerpos y grupos, tinta
//!   de botones sobre su fondo y acentos gráficos sobre `BODY`/`GROUP`.
//! - **Informativo**: `MINT`/`WARM` son acentos gráficos y superan 3:1, pero no
//!   se usan como texto de estado; si se comparan con el umbral de texto
//!   aparecen como aviso. `TEXT_3` sobre `TRACK`/`GROUP_HOVER` corresponde a
//!   estados deshabilitados o hover sin texto principal.

use crate::tokens::{palette, Color};

/// Umbral AA para texto.
pub const AA_TEXT: f64 = 4.5;
/// Umbral AA para gráficos.
pub const AA_GRAPHIC: f64 = 3.0;

/// Parejas con requisito AA obligatorio.
const TEXT_PAIRS: [(&str, &str); 17] = [
    ("TEXT", "BODY"),
    ("TEXT", "GROUP"),
    ("TEXT", "GROUP_HOVER"),
    ("TEXT", "TRACK"),
    ("TEXT_2", "BODY"),
    ("TEXT_2", "GROUP"),
    ("TEXT_2", "GROUP_HOVER"),
    ("TEXT_3", "BODY"),
    ("TEXT_3", "GROUP"),
    ("MINT_INK", "MINT_BTN"),
    ("WARM_INK", "WARM_BTN"),
    ("ERR_INK", "ERR"),
    ("WORD_IDLE", "BODY"),
    ("WORD_MINT", "BODY"),
    ("WORD_WARM", "BODY"),
    ("WORD_MUTE", "BODY"),
    ("WORD_ERR", "BODY"),
];

/// Parejas gráficas obligatorias.
const GRAPHIC_PAIRS: [(&str, &str); 10] = [
    ("MINT", "BODY"),
    ("WARM", "BODY"),
    ("ERR", "BODY"),
    ("EYE_IDLE", "BODY"),
    ("EYE_MUTE", "BODY"),
    ("MINT", "GROUP"),
    ("WARM", "GROUP"),
    ("ERR", "GROUP"),
    ("MINT_BTN", "BODY"),
    ("WARM_BTN", "BODY"),
];

/// Avisos: no bloquean, pero se muestran en el informe.
const ADVISORY_TEXT_PAIRS: [(&str, &str); 4] = [
    ("MINT", "BODY"),
    ("WARM", "BODY"),
    ("TEXT_3", "GROUP_HOVER"),
    ("TEXT_3", "TRACK"),
];

/// `_lin(c)`.
fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Luminancia relativa WCAG de un color RGB 0..255 (se usan los tres primeros).
pub fn luminance(rgb: &[u8]) -> f64 {
    let r = linear(rgb.first().copied().unwrap_or(0));
    let g = linear(rgb.get(1).copied().unwrap_or(0));
    let b = linear(rgb.get(2).copied().unwrap_or(0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Relación de contraste WCAG entre dos colores RGB (sin alfa).
pub fn contrast(a: &[u8], b: &[u8]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = (la.max(lb), la.min(lb));
    (hi + 0.05) / (lo + 0.05)
}

/// Una comprobación de contraste (`ContrastCheck`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContrastCheck {
    pub theme: &'static str,
    pub foreground: &'static str,
    pub background: &'static str,
    pub kind: &'static str,
    pub minimum: f64,
    pub ratio: f64,
    pub advisory: bool,
}

impl ContrastCheck {
    pub fn passed(&self) -> bool {
        self.ratio >= self.minimum
    }
}

fn check(
    theme: &'static str,
    foreground: &'static str,
    background: &'static str,
    kind: &'static str,
    advisory: bool,
) -> ContrastCheck {
    let colors = palette(theme).expect("el tema de la comprobación existe");
    let fg: Color = colors[foreground];
    let bg: Color = colors[background];
    ContrastCheck {
        theme,
        foreground,
        background,
        kind,
        minimum: if kind == "text" { AA_TEXT } else { AA_GRAPHIC },
        ratio: contrast(&fg.channels(), &bg.channels()),
        advisory,
    }
}

/// Lista de comprobaciones de contraste de un tema.
pub fn contrast_report(theme: &'static str, include_advisory: bool) -> Vec<ContrastCheck> {
    let mut checks: Vec<ContrastCheck> = TEXT_PAIRS
        .iter()
        .map(|(fg, bg)| check(theme, fg, bg, "text", false))
        .collect();
    checks.extend(
        GRAPHIC_PAIRS
            .iter()
            .map(|(fg, bg)| check(theme, fg, bg, "graphic", false)),
    );
    if include_advisory {
        checks.extend(
            ADVISORY_TEXT_PAIRS
                .iter()
                .map(|(fg, bg)| check(theme, fg, bg, "text", true)),
        );
    }
    checks
}

/// Comprobaciones que no alcanzan su mínimo.
pub fn failures(theme: &'static str, include_advisory: bool) -> Vec<ContrastCheck> {
    contrast_report(theme, include_advisory)
        .into_iter()
        .filter(|check| !check.passed())
        .collect()
}

/// True si todas las parejas obligatorias cumplen AA en `theme`.
pub fn validate_palette(theme: &'static str) -> bool {
    failures(theme, false).is_empty()
}

/// Valida las parejas obligatorias en oscuro y claro.
pub fn validate_all_palettes() -> [(&'static str, bool); 2] {
    [
        ("dark", validate_palette("dark")),
        ("light", validate_palette("light")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_and_contrast_match_the_wcag_anchors() {
        assert_eq!(luminance(&[0, 0, 0]), 0.0);
        assert!((luminance(&[255, 255, 255]) - 1.0).abs() < 1e-12);
        assert!((contrast(&[0, 0, 0], &[255, 255, 255]) - 21.0).abs() < 1e-9);
        assert_eq!(contrast(&[255, 255, 255], &[255, 255, 255]), 1.0);
        // El alfa se ignora: solo cuentan los tres primeros canales.
        assert_eq!(contrast(&[1, 2, 3, 4], &[1, 2, 3]), 1.0);
    }

    #[test]
    fn both_factory_palettes_pass_and_advisories_are_optional() {
        assert_eq!(validate_all_palettes(), [("dark", true), ("light", true)]);
        assert!(failures("dark", false).is_empty());
        let advisory = failures("dark", true);
        assert_eq!(advisory.len(), 1);
        assert_eq!(advisory[0].foreground, "TEXT_3");
        assert!(!advisory[0].passed());
        assert_eq!(contrast_report("light", true).len(), 31);
    }
}
