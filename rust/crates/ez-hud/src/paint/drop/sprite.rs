//! `drop.py 635-676`: sprites de los botones redondos con su icono.

use std::cell::RefCell;
use std::collections::HashMap;

use ez_gfx::icons::{colorize, icon_mask, slash_masks, Color, IconError};
use ez_gfx::raster::Image;
use ndarray::Array2;

use crate::tokens;

use super::colors::{disc, ui, Ui};
use super::BAR_ICON;

/// Valores por defecto de Python para las opciones del sprite.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpriteOptions {
    pub hang: bool,
    pub hover: bool,
    pub muted: f64,
    pub signal: bool,
    pub call: bool,
}

/// Botón de la barra → icono de la familia única de `icons`.
fn bar_glyph(glyph: &str) -> &str {
    match glyph {
        "gear" => "menu",
        "chat" => "chat",
        "mic" => "microphone",
        "hang" | "no" => "close",
        "yes" => "check",
        "call" => "waves",
        other => other,
    }
}

type SpriteKey = (String, String, i64, bool, bool, i64, bool, bool, i64);
type Layer = (Array2<f32>, [u8; 4]);

thread_local! {
    static SPRITES: RefCell<HashMap<SpriteKey, Image>> = RefCell::new(HashMap::new());
}

fn sprite_key(
    glyph: &str,
    theme: &str,
    scale: f64,
    options: &SpriteOptions,
    muted: f64,
) -> SpriteKey {
    (
        glyph.to_string(),
        theme.to_string(),
        (scale * 1000.0).round_ties_even() as i64,
        options.hang,
        options.hover,
        (muted * 8.0) as i64,
        options.signal,
        options.call,
        tokens::palette_version(),
    )
}

fn sprite_kind(options: &SpriteOptions) -> &'static str {
    if options.call {
        "call"
    } else if options.hang {
        "hang"
    } else if options.hover {
        "hover"
    } else {
        "disc"
    }
}

fn sprite_ink(colors: &Ui, options: &SpriteOptions, muted: f64) -> [u8; 4] {
    if options.call {
        colors.call_icon
    } else if options.hang {
        colors.hang_icon
    } else if muted >= 0.5 {
        colors.fault
    } else if options.signal {
        colors.signal
    } else {
        colors.icon
    }
}

fn sprite_layers(
    glyph: &str,
    scale: f64,
    side: u32,
    muted: f64,
    ink: [u8; 4],
    fault: [u8; 4],
) -> Result<Vec<Layer>, IconError> {
    let mask = icon_mask(bar_glyph(glyph), BAR_ICON, scale, Some(side))?;
    if glyph == "mic" && muted > 0.0 {
        let (gap, line) = slash_masks(BAR_ICON, scale, muted, Some(side))?;
        let cut = Array2::from_shape_fn(mask.dim(), |(row, column)| {
            mask[[row, column]] * (1.0 - gap[[row, column]])
        });
        Ok(vec![(cut, ink), (line, fault)])
    } else {
        Ok(vec![(mask, ink)])
    }
}

fn apply_layers(out: &mut [f32], out_side: u32, offset: usize, side: u32, layers: Vec<Layer>) {
    for (cover, color) in layers {
        let ink = colorize(&cover, Color::Rgba(color[0], color[1], color[2], color[3]));
        for row in 0..side as usize {
            for column in 0..side as usize {
                let index = ((offset + row) * out_side as usize + offset + column) * 4;
                for channel in 0..4 {
                    let value = ink[[row, column, channel]];
                    out[index + channel] =
                        value + out[index + channel] * (1.0 - ink[[row, column, 3]]);
                }
            }
        }
    }
}

/// Botón redondo con sombra suave (RGBA recto).
///
/// `muted` (0..1) es el progreso de la raya de pausa y se cuantiza en 8 pasos
/// para que la animación reutilice sprites en caché.
pub fn button_sprite(
    glyph: &str,
    theme: &str,
    scale: f64,
    options: &SpriteOptions,
) -> Result<Image, IconError> {
    let muted = (options.muted.clamp(0.0, 1.0) * 8.0).round_ties_even() / 8.0;
    let theme = if theme == "light" { "light" } else { "dark" };
    let key = sprite_key(glyph, theme, scale, options, muted);
    if let Some(sprite) = SPRITES.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(sprite);
    }
    let colors = ui(theme);
    let dot = disc(theme, scale, sprite_kind(options));
    let out_side = dot.side;
    let mut out = (*dot.values).clone();
    let ink = sprite_ink(&colors, options, muted);
    let mut side = (BAR_ICON * scale).round_ties_even().max(1.0) as u32;
    side += out_side.saturating_sub(side) % 2; // misma paridad que el disco
    let layers = sprite_layers(glyph, scale, side, muted, ink, colors.fault)?;
    let offset = (out_side.saturating_sub(side) / 2) as usize;
    apply_layers(&mut out, out_side, offset, side, layers);
    let image = Image::from_f32_premultiplied_rgba(out_side, out_side, &out)
        .expect("el sprite tiene su tamaño");
    SPRITES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 320 {
            cache.clear();
        }
        cache.insert(key, image.clone());
    });
    Ok(image)
}
