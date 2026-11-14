//! `drop.py 566-635`: colores de los botones, disco con sombra y su caché.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ez_gfx::alpha_composite;
use ez_gfx::draw::Draw;
use ez_gfx::raster::{Image, Rgba};

use crate::model::drop::{BTN, BUD_SS, SHADOW};
use crate::tokens::{self, Color};

use super::canvas::reduce_rgba;

/// Colores de un botón, ya resueltos para un tema.
///
/// Es el `dict` `UI[theme]` del Python con los colores RGB normalizados a RGBA
/// (alfa 255 cuando el original no la lleva).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ui {
    pub disc: [u8; 4],
    pub hover: [u8; 4],
    pub icon: [u8; 4],
    pub fault: [u8; 4],
    pub hang: [u8; 4],
    pub hang_icon: [u8; 4],
    pub call: [u8; 4],
    pub call_icon: [u8; 4],
    pub ring: [u8; 4],
    pub shadow: [u8; 4],
    pub signal: [u8; 4],
}

/// Un disco ya pintado: RGBA premultiplicado `0..1` y su lado físico.
#[derive(Clone)]
pub(super) struct Disc {
    pub side: u32,
    pub values: Arc<Vec<f32>>,
}

thread_local! {
    static DISCS: RefCell<HashMap<(String, i64, String, i64), Disc>> =
        RefCell::new(HashMap::new());
}

// DECISION: los colores RGB de Python se guardan como RGBA con alfa 255; así el
// mismo `Rgba` alimenta tanto `ImageDraw` como `icons::colorize`.
const UI_LIGHT: Ui = Ui {
    disc: [255, 255, 255, 245],
    hover: [255, 255, 255, 255],
    icon: [16, 34, 38, 255],
    fault: [214, 51, 61, 255],
    hang: [16, 34, 38, 255],
    hang_icon: [255, 255, 255, 255],
    call: [12, 112, 117, 255],
    call_icon: [255, 255, 255, 255],
    ring: [255, 255, 255, 56],
    shadow: [12, 60, 66, 64],
    signal: [12, 112, 117, 255],
};

const UI_DARK: Ui = Ui {
    disc: [20, 33, 38, 245],
    hover: [28, 45, 51, 255],
    icon: [228, 242, 240, 255],
    fault: [255, 120, 118, 255],
    hang: [48, 66, 72, 255],
    hang_icon: [236, 244, 242, 255],
    call: [94, 226, 196, 255],
    call_icon: [6, 34, 30, 255],
    ring: [232, 238, 250, 30],
    shadow: [0, 0, 0, 120],
    signal: [94, 226, 196, 255],
};

/// `UI`: los colores de fábrica de los botones, por tema.
pub const UI: [(&str, Ui); 2] = [("light", UI_LIGHT), ("dark", UI_DARK)];

/// `SIGNAL`: color de «activo» (☰ con el menú abierto, ✓) por tema.
pub const SIGNAL: [(&str, [u8; 3]); 2] = [("light", [12, 112, 117]), ("dark", [94, 226, 196])];

impl Ui {
    fn channel(&self, name: &str) -> [u8; 4] {
        match name {
            "hover" => self.hover,
            "hang" => self.hang,
            "call" => self.call,
            _ => self.disc,
        }
    }
}

fn channels(color: Color) -> [u8; 3] {
    color.channels()
}

fn opaque(rgb: [u8; 3]) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], 255]
}

fn with_alpha(rgb: [u8; 3], alpha: u8) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], alpha]
}

/// `_ui`: los colores de `UI` con superficies, texto y acento de la paleta.
pub(super) fn ui(theme: &str) -> Ui {
    let name = if theme == "light" { "light" } else { "dark" };
    let mut out = if name == "light" { UI_LIGHT } else { UI_DARK };
    let palette = tokens::palette(name).expect("tema normalizado");
    let mint = opaque(channels(palette["MINT"]));
    out.call = mint;
    out.signal = mint;
    out.call_icon = opaque(channels(palette["MINT_INK"]));
    out.icon = opaque(channels(palette["TEXT"]));
    if name == "dark" {
        out.disc = with_alpha(channels(palette["GROUP"]), 245);
        out.hover = opaque(channels(palette["GROUP_HOVER"]));
        out.hang = opaque(channels(palette["TRACK"]));
        out.hang_icon = opaque(channels(palette["TEXT"]));
    } else {
        out.hang = opaque(channels(palette["TEXT"]));
    }
    out
}

/// Disco con sombra y sin icono: RGBA premultiplicado `0..1`.
// DECISION: el tema se normaliza antes de la clave (el Python guardaría dos
// entradas para `auto` y `dark`); los píxeles son los mismos.
pub(super) fn disc(theme: &str, scale: f64, kind: &str) -> Disc {
    let name = if theme == "light" { "light" } else { "dark" };
    let key = (
        name.to_string(),
        (scale * 1000.0).round_ties_even() as i64,
        kind.to_string(),
        tokens::palette_version(),
    );
    if let Some(cached) = DISCS.with(|cache| cache.borrow().get(&key).cloned()) {
        return cached;
    }
    let built = build_disc(name, scale, kind);
    DISCS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, built.clone());
    });
    built
}

fn build_disc(theme: &str, scale: f64, kind: &str) -> Disc {
    let colors = ui(theme);
    let q = BUD_SS as f64;
    let side = ((BTN + 2.0 * SHADOW) * scale).round_ties_even().max(0.0) as u32;
    let big = side.saturating_mul(BUD_SS as u32);
    let center = f64::from(big) / 2.0;
    let radius = BTN * scale * q / 2.0;
    let mut shadow = Image::new(big, big, Rgba::TRANSPARENT);
    Draw::new(&mut shadow)
        .ellipse(
            [
                center - radius,
                center - radius + 3.0 * scale * q,
                center + radius,
                center + radius + 3.0 * scale * q,
            ],
            Some(Rgba::from_array(colors.shadow)),
            None,
            0,
        )
        .expect("elipse de la sombra con coordenadas ordenadas");
    let shadow = shadow.gaussian_blur((4.0 * scale * q) as f32);
    let mut dot = Image::new(big, big, Rgba::TRANSPARENT);
    let ring = if kind == "hang" || kind == "call" || theme == "dark" {
        Some(Rgba::from_array(colors.ring))
    } else {
        None
    };
    Draw::new(&mut dot)
        .ellipse(
            [
                center - radius,
                center - radius,
                center + radius,
                center + radius,
            ],
            Some(Rgba::from_array(colors.channel(kind))),
            ring,
            (scale * q).round_ties_even().max(1.0) as u32,
        )
        .expect("elipse del disco con coordenadas ordenadas");
    let composed = alpha_composite(&shadow, &dot).expect("sombra y disco del mismo tamaño");
    let premultiplied = composed.to_premultiplied_rgba();
    let reduced = reduce_rgba(&premultiplied, big, big, BUD_SS as u32);
    Disc {
        side: big / BUD_SS as u32,
        values: Arc::new(reduced.iter().map(|&v| f32::from(v) / 255.0).collect()),
    }
}
