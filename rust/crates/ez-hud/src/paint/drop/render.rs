//! `drop.py 738-844`: el fotograma completo de la barra.

use std::collections::BTreeMap;

use ez_core::metrics::monotonic;
use ez_gfx::icons::IconError;
use ez_gfx::raster::Image;
use ez_gfx::resample::Resampling;

use crate::model::drop::{emerge_order, DropModel};
use crate::motion::Spring;
use crate::paint::marks::orbit_tile;

use super::canvas::{mute_tint, put_alpha, round2, Canvas};
use super::fluid::{form_dims, form_mask, render_fluid};
use super::sprite::{button_sprite, SpriteOptions};
use super::MUTE_SHRINK;

/// Argumentos de `render_model`, con los valores por defecto del Python.
// DECISION: un struct en vez de los keyword-only de Python (Rust no tiene
// argumentos con nombre ni valores por defecto); los campos opcionales llevan
// el default de `render_model`.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions<'a> {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub theme: &'a str,
    pub muted: bool,
    pub in_call: bool,
    pub menu_open: bool,
    pub now: Option<f64>,
    pub hover_item: Option<&'a str>,
    pub chat_open: bool,
    pub orbit: &'a [&'a str],
    pub pulse: f64,
}

impl Default for RenderOptions<'_> {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            scale: 1.0,
            theme: "light",
            muted: false,
            in_call: true,
            menu_open: false,
            now: None,
            hover_item: None,
            chat_open: false,
            orbit: &[],
            pulse: 1.0,
        }
    }
}

/// Dibuja un fotograma BGRA premultiplicado de `width`×`height` px físicos.
// DECISION: el Python lanza `ValueError` si un glifo no existe; aquí se
// propaga `IconError` para que el hilo de UI decida (todos los glifos de la
// barra son válidos).
pub fn render_model(model: &DropModel, options: &RenderOptions<'_>) -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(options.width, options.height);
    let positions = model.positions();
    if !model.form_hidden {
        draw_buttons(&mut canvas, model, options, &positions)?;
    }
    draw_form(&mut canvas, model, options, &positions)?;
    Ok(canvas.image.to_bgra_premultiplied())
}

/// `mp.x` si la pausa está activa (o el resorte aún no ha terminado); si no, 1.
fn mute_progress(model: &DropModel, muted: bool) -> f64 {
    let spring = &model.mute_p;
    let value = if spring.t > 0.0 || !muted {
        spring.x
    } else {
        1.0
    };
    value.clamp(0.0, 1.0)
}

/// Cerca del tamaño de reposo el sprite se dibuja ya a ese tamaño (2 %); solo
/// la aparición se reescala.
fn tile_scale(scale: f64, sprite_scale: f64) -> (f64, f64) {
    if (0.85..=1.15).contains(&sprite_scale) {
        let quantized = (sprite_scale * 50.0).round_ties_even() / 50.0;
        (scale * quantized, sprite_scale / quantized)
    } else {
        (scale, sprite_scale)
    }
}

fn draw_buttons(
    canvas: &mut Canvas,
    model: &DropModel,
    options: &RenderOptions<'_>,
    positions: &BTreeMap<&'static str, (f64, f64, f64, f64)>,
) -> Result<(), IconError> {
    let ask = model.ask.x.clamp(0.0, 1.0);
    let mute = mute_progress(model, options.muted);
    // Los lejanos primero: al salir pasan por detrás de los cercanos.
    for group in emerge_order(model.layout, Some(model.dock)).iter().rev() {
        for name in group {
            let (cx, cy, sprite_scale, alpha) = positions[*name];
            if sprite_scale <= 0.02 || alpha <= 0.01 {
                continue;
            }
            let (size, zoom) = tile_scale(options.scale, sprite_scale);
            let hover = options.hover_item == Some(*name);
            let tiles = sprite_tiles(model, name, options, size, hover, mute, ask)?;
            for (tile, share) in tiles {
                if share > 0.004 {
                    canvas.paste(
                        &tile,
                        cx * options.scale,
                        cy * options.scale,
                        zoom,
                        alpha * share,
                    );
                }
            }
        }
    }
    Ok(())
}

fn sprite(glyph: &str, theme: &str, size: f64, options: SpriteOptions) -> Result<Image, IconError> {
    button_sprite(glyph, theme, size, &options)
}

fn sprite_tiles(
    model: &DropModel,
    name: &str,
    options: &RenderOptions<'_>,
    size: f64,
    hover: bool,
    mute: f64,
    ask: f64,
) -> Result<Vec<(Image, f64)>, IconError> {
    match name {
        "gear" => gear_tile(options, size, hover),
        "chat" => chat_tile(options, size, hover),
        "hang" => hang_tiles(model, options, size, hover),
        _ => mic_tiles(options, size, hover, mute, ask),
    }
}

fn gear_tile(
    options: &RenderOptions<'_>,
    size: f64,
    hover: bool,
) -> Result<Vec<(Image, f64)>, IconError> {
    let sprite = sprite(
        "gear",
        options.theme,
        size,
        SpriteOptions {
            hover,
            signal: options.menu_open,
            ..SpriteOptions::default()
        },
    )?;
    Ok(vec![(sprite, 1.0)])
}

fn chat_tile(
    options: &RenderOptions<'_>,
    size: f64,
    hover: bool,
) -> Result<Vec<(Image, f64)>, IconError> {
    let sprite = sprite(
        "chat",
        options.theme,
        size,
        SpriteOptions {
            hover,
            signal: options.chat_open,
            ..SpriteOptions::default()
        },
    )?;
    Ok(vec![(sprite, 1.0)])
}

fn hang_tiles(
    model: &DropModel,
    options: &RenderOptions<'_>,
    size: f64,
    hover: bool,
) -> Result<Vec<(Image, f64)>, IconError> {
    // Al preguntar, ✕ significa «no».
    let share = call_factor(&model.call, model.asking);
    let hang = sprite(
        "hang",
        options.theme,
        size,
        SpriteOptions {
            hang: true,
            hover,
            ..SpriteOptions::default()
        },
    )?;
    let call = sprite(
        "call",
        options.theme,
        size,
        SpriteOptions {
            call: true,
            hover,
            ..SpriteOptions::default()
        },
    )?;
    Ok(vec![(hang, 1.0 - share), (call, share)])
}

fn call_factor(spring: &Spring, asking: bool) -> f64 {
    if asking {
        0.0
    } else {
        spring.x.clamp(0.0, 1.0)
    }
}

fn mic_tiles(
    options: &RenderOptions<'_>,
    size: f64,
    hover: bool,
    mute: f64,
    ask: f64,
) -> Result<Vec<(Image, f64)>, IconError> {
    let mic = sprite(
        "mic",
        options.theme,
        size,
        SpriteOptions {
            hover,
            muted: mute,
            ..SpriteOptions::default()
        },
    )?;
    let yes = sprite(
        "yes",
        options.theme,
        size,
        SpriteOptions {
            hover,
            signal: true,
            ..SpriteOptions::default()
        },
    )?;
    Ok(vec![(mic, 1.0 - ask), (yes, ask)])
}

fn draw_form(
    canvas: &mut Canvas,
    model: &DropModel,
    options: &RenderOptions<'_>,
    positions: &BTreeMap<&'static str, (f64, f64, f64, f64)>,
) -> Result<(), IconError> {
    if model.form_hidden {
        return Ok(());
    }
    let (x, y, mut grow, _) = positions["pill"];
    let mute = mute_progress(model, options.muted);
    grow *= 1.0 - MUTE_SHRINK * mute;
    let mask = form_mask(model.layout, options.scale * grow);
    let fluid = render_fluid(model, options.now.unwrap_or_else(monotonic));
    let (rows, columns, _) = fluid.dim();
    let mut pixels = Vec::with_capacity(rows * columns * 3);
    for row in 0..rows {
        for column in 0..columns {
            for channel in 0..3 {
                pixels.push(fluid[[row, column, channel]]);
            }
        }
    }
    let mut interior = Image::from_rgb(columns as u32, rows as u32, pixels)
        .expect("la rejilla del fluido tiene tres canales");
    interior = interior.resize(mask.width(), mask.height(), Resampling::Bicubic);
    if mute > 0.01 {
        interior = mute_tint(&interior, mute);
    }
    put_alpha(&mut interior, &mask);
    canvas.paste(&interior, x * options.scale, y * options.scale, 1.0, 1.0);
    if !options.orbit.is_empty() {
        let (width, height) = form_dims(model.layout);
        let quantized = round2(grow);
        if let Some(tile) = orbit_tile(
            model.layout,
            options.orbit,
            width * quantized,
            height * quantized,
            options.scale,
            options.theme,
            options.pulse,
        ) {
            canvas.paste(&tile, x * options.scale, y * options.scale, 1.0, 1.0);
        }
    }
    Ok(())
}
