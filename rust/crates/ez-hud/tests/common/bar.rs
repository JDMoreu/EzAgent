//! Casos de `hud_bar`: cuadros PNG contra `parity/golden/hud/` y curvas.

use ez_gfx::raster::Image;
use ez_gfx::resample::Resampling;
use ez_gfx::text::{draw_text, text_size, Align, Color as TextColor, FloatSurface, TextOptions};
use ez_hud::model::drop::{colors_for, layout_size, DropModel, Update, POSE_OF};
use ez_hud::paint::drop::{button_sprite, render_model, RenderOptions, SpriteOptions};
use ez_hud::paint::flow::{custom_palette, FlowField, PRESETS};
use ez_hud::paint::marks::orbit_tile;
use ez_hud::{theme, tokens};

use crate::common::json::{arr3, field, Json};

const WARM_S: f64 = 1.2;
const DT: f64 = 1.0 / 60.0;

#[derive(Clone, Debug)]
struct Look {
    mode: String,
    base: String,
    preset: Option<String>,
    dpi: f64,
    layout: String,
    dock: String,
    pose: String,
    muted: bool,
    in_call: bool,
    orbit: Vec<String>,
    pulse: f64,
    hover: bool,
    asking: bool,
    menu_open: bool,
    rms: f64,
}

impl Look {
    fn from_desc(desc: &Json) -> Self {
        Self {
            mode: field(desc, "theme")
                .as_str()
                .unwrap_or_default()
                .to_string(),
            base: field(desc, "base").as_str().unwrap_or_default().to_string(),
            preset: field(desc, "preset").as_str().map(str::to_string),
            dpi: field(desc, "dpi").as_f64().expect("dpi"),
            layout: field(desc, "layout").as_str().unwrap_or("pill").to_string(),
            dock: field(desc, "dock").as_str().unwrap_or("sides").to_string(),
            pose: field(desc, "state").as_str().unwrap_or("idle").to_string(),
            muted: field(desc, "muted").as_bool().unwrap_or(false),
            in_call: field(desc, "in_call").as_bool().unwrap_or(true),
            orbit: field(desc, "orbit")
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            pulse: field(desc, "pulse").as_f64().unwrap_or(1.0),
            hover: field(desc, "hover").as_bool().unwrap_or(true),
            asking: field(desc, "asking").as_bool().unwrap_or(false),
            menu_open: field(desc, "menu_open").as_bool().unwrap_or(false),
            rms: field(desc, "rms").as_f64().unwrap_or(0.0),
        }
    }

    fn idle_before(&self) -> Self {
        Self {
            pose: "idle".to_string(),
            muted: false,
            in_call: true,
            hover: false,
            asking: false,
            menu_open: false,
            rms: 0.0,
            orbit: Vec::new(),
            pulse: 1.0,
            ..self.clone()
        }
    }

    fn custom(&self) -> Option<ez_hud::model::drop::Palette> {
        let preset = self.preset.as_deref()?;
        if self.mode != "custom" {
            return None;
        }
        let (_, colors) = PRESETS.iter().find(|(name, _)| *name == preset)?;
        let [primary, secondary, accent, background] = *colors;
        custom_palette(primary, secondary, accent, background)
    }

    fn activate(&self) {
        let themed = if self.mode == "custom" {
            self.preset.as_deref().and_then(|preset| {
                let (_, colors) = PRESETS.iter().find(|(name, _)| *name == preset)?;
                theme::derive(colors[0], colors[3])
            })
        } else {
            None
        };
        tokens::set_themed(themed.as_ref());
    }
}

fn bar_pose(pose: &str) -> &'static str {
    POSE_OF
        .iter()
        .find(|(state, _)| *state == pose)
        .map_or("idle", |(_, bar)| *bar)
}

fn step(model: &mut DropModel, look: &Look, now: f64, dt: f64) {
    model.update(&Update {
        now,
        dt,
        pose: bar_pose(&look.pose),
        rms: look.rms,
        theme: &look.base,
        hover: look.hover,
        asking: look.asking,
        menu_open: look.menu_open,
        reduced: false,
        buttons: "hover",
        muted: look.muted,
        hover_item: None,
        pressed: None,
        in_call: look.in_call,
        custom: look.custom(),
    });
}

fn round_half_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}

fn window_size(look: &Look) -> (u32, u32) {
    let (width, height) = layout_size(&look.layout, Some(&look.dock));
    (
        (width * look.dpi).round_ties_even().max(1.0) as u32,
        (height * look.dpi).round_ties_even().max(1.0) as u32,
    )
}

/// `_straight`: BGRA premultiplicado u8 -> RGBA recto u8.
fn straight_from_bgra(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for pixel in data.as_chunks::<4>().0 {
        let alpha = f32::from(pixel[3]) / 255.0;
        let rgb = [
            f32::from(pixel[2]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[0]) / 255.0,
        ];
        out.extend(premultiplied_to_straight(rgb, alpha));
    }
    out
}

fn premultiplied_to_straight(rgb: [f32; 3], alpha: f32) -> [u8; 4] {
    let safe = alpha.max(1e-6);
    let straight = rgb.map(|value| {
        let value = if alpha > 0.0 { value / safe } else { 0.0 };
        (value.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
    });
    [
        straight[0],
        straight[1],
        straight[2],
        (alpha.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8,
    ]
}

fn float_buffer_to_rgba(data: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for pixel in data.as_chunks::<4>().0 {
        out.extend(premultiplied_to_straight(
            [pixel[0], pixel[1], pixel[2]],
            pixel[3],
        ));
    }
    out
}

fn grid(width: f64, height: f64) -> (ndarray::Array2<f32>, ndarray::Array2<f32>) {
    let nx = round_half_even(width / 2.0).max(2) as usize;
    let ny = round_half_even(height / 2.0).max(2) as usize;
    let unit = height / 2.0;
    let xs: Vec<f32> = (0..nx)
        .map(|index| (((index as f64 + 0.5) * (width / nx as f64) - width / 2.0) / unit) as f32)
        .collect();
    let ys: Vec<f32> = (0..ny)
        .map(|index| (((index as f64 + 0.5) * (height / ny as f64) - height / 2.0) / unit) as f32)
        .collect();
    let mut gx = ndarray::Array2::<f32>::zeros((ny, nx));
    let mut gy = ndarray::Array2::<f32>::zeros((ny, nx));
    for row in 0..ny {
        for column in 0..nx {
            gx[[row, column]] = xs[column];
            gy[[row, column]] = ys[row];
        }
    }
    (gx, gy)
}

fn form_dims(layout: &str) -> (f64, f64) {
    if layout == "circle" {
        (58.0, 58.0)
    } else {
        (108.0, 38.0)
    }
}

fn marea_image(field: &mut FlowField, look: &Look) -> Image {
    let (width, height) = form_dims(&look.layout);
    let (gx, gy) = grid(width, height);
    let pose = if look.pose == "sleep" {
        "idle"
    } else {
        &look.pose
    };
    let colors = match look.custom() {
        Some(custom) if ez_hud::model::drop::CUSTOM_POSES.contains(&pose) => custom,
        _ => colors_for(&look.base, pose),
    };
    let rendered = field.render(&gx.view(), &gy.view(), &look.base, &colors);
    let (rows, columns) = (rendered.shape()[0], rendered.shape()[1]);
    let mut rgb = Vec::with_capacity(rows * columns * 3);
    for row in 0..rows {
        for column in 0..columns {
            for channel in 0..3 {
                rgb.push(rendered[[row, column, channel]].clamp(0.0, 255.0) as u8);
            }
        }
    }
    let base = Image::from_rgb(columns as u32, rows as u32, rgb).expect("rejilla RGB");
    base.resize(
        (width * look.dpi).round_ties_even().max(1.0) as u32,
        (height * look.dpi).round_ties_even().max(1.0) as u32,
        Resampling::Bicubic,
    )
}

fn marea_field(look: &Look) -> FlowField {
    let aspect = if look.layout == "circle" {
        1.0
    } else {
        108.0 / 38.0
    };
    FlowField::new(&look.layout, Some(aspect), 7)
}

pub fn samples(desc: &Json) -> Vec<i64> {
    field(desc, "samples_ms")
        .as_array()
        .map(|items| items.iter().filter_map(Json::as_i64).collect())
        .unwrap_or_default()
}

fn marea_frame(desc: &Json, t_ms: i64) -> Result<Image, String> {
    let look = Look::from_desc(desc);
    look.activate();
    let mut flow = marea_field(&look);
    if field(desc, "animation").is_null() {
        for _ in 0..round_half_even(WARM_S * 30.0) {
            flow.step(1.0 / 30.0, look.rms, &look.pose, false);
        }
    } else {
        let mut previous = 0.0;
        for sample in samples(desc) {
            if sample > 0 {
                flow.step(
                    (sample as f64 - previous) / 1000.0,
                    look.rms,
                    &look.pose,
                    false,
                );
            }
            previous = sample as f64;
            if sample == t_ms {
                break;
            }
        }
    }
    Ok(marea_image(&mut flow, &look))
}

fn bar_frame(desc: &Json, t_ms: i64) -> Result<Vec<u8>, String> {
    let look = Look::from_desc(desc);
    look.activate();
    let mut model = DropModel::new(&look.layout, Some(&look.dock));
    if field(desc, "animation").is_null() {
        model.form_hidden = look.pose == "off";
        step(&mut model, &look, 0.0, 0.0);
        let mut now = 0.0;
        for _ in 0..round_half_even(WARM_S / DT).max(1) {
            now += DT;
            step(&mut model, &look, now, DT);
        }
    } else {
        let before = look.idle_before();
        for index in 0..round_half_even(WARM_S / DT) {
            step(&mut model, &before, -WARM_S + index as f64 * DT, DT);
        }
        step(&mut model, &look, 0.0, 0.0);
        let mut previous = 0.0;
        for sample in samples(desc) {
            if sample > 0 {
                let now = sample as f64 / 1000.0;
                step(&mut model, &look, now, now - previous);
            }
            previous = sample as f64 / 1000.0;
            if sample == t_ms {
                break;
            }
        }
    }
    let (width, height) = window_size(&look);
    let orbit: Vec<&str> = look.orbit.iter().map(String::as_str).collect();
    let pixels = render_model(
        &model,
        &RenderOptions {
            width,
            height,
            scale: look.dpi,
            theme: &look.base,
            muted: look.muted,
            in_call: look.in_call,
            menu_open: look.menu_open,
            now: Some(0.0),
            hover_item: None,
            chat_open: false,
            orbit: &orbit,
            pulse: look.pulse,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    Ok(straight_from_bgra(&pixels))
}

fn sprite_frame(desc: &Json) -> Result<Image, String> {
    let look = Look::from_desc(desc);
    look.activate();
    button_sprite(
        field(desc, "glyph").as_str().expect("glyph"),
        &look.base,
        look.dpi,
        &SpriteOptions {
            hang: field(desc, "hang").as_bool().unwrap_or(false),
            hover: field(desc, "hover").as_bool().unwrap_or(false),
            muted: field(desc, "muted").as_f64().unwrap_or(0.0),
            signal: field(desc, "signal").as_bool().unwrap_or(false),
            call: field(desc, "call").as_bool().unwrap_or(false),
        },
    )
    .map_err(|error| format!("{error:?}"))
}

fn orbit_frame(desc: &Json) -> Option<Image> {
    let look = Look::from_desc(desc);
    look.activate();
    let (width, height) = form_dims(&look.layout);
    let states: Vec<&str> = field(desc, "states")
        .as_array()
        .map(|items| items.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    orbit_tile(
        &look.layout,
        &states,
        width,
        height,
        look.dpi,
        &look.base,
        look.pulse,
    )
}

fn text_frame(desc: &Json) -> Result<Vec<u8>, String> {
    tokens::set_themed(None);
    let text = field(desc, "text").as_str().expect("text");
    let style =
        ez_gfx::text::font_style(field(desc, "style").as_str().expect("style")).expect("estilo");
    let dpi = field(desc, "dpi").as_f64().expect("dpi");
    let rgb = arr3(field(desc, "color"));
    let (logical_w, logical_h) =
        text_size(text, style, dpi, None, "left").map_err(|error| error.to_string())?;
    let pad = 4.0;
    let width = ((logical_w * dpi).ceil() as usize + 8).max(1);
    let height = ((logical_h * dpi).ceil() as usize + 8).max(1);
    let mut buffer = vec![0.0f32; width * height * 4];
    let mut surface = FloatSurface::new(&mut buffer, width, height);
    draw_text(
        &mut surface,
        text,
        style,
        TextColor::Rgb(rgb),
        (pad, pad),
        TextOptions {
            scale: dpi,
            align: Align::Left,
            tracking: None,
            alpha: 1.0,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(float_buffer_to_rgba(&buffer))
}

/// Bytes RGBA rectos y tamaño del cuadro que pide el caso.
pub fn frame(desc: &Json, function: &str, t_ms: i64) -> Result<(Vec<u8>, u32, u32), String> {
    match function {
        "render_model" => {
            let look = Look::from_desc(desc);
            let (width, height) = window_size(&look);
            Ok((bar_frame(desc, t_ms)?, width, height))
        }
        "button_sprite" => {
            let image = sprite_frame(desc)?;
            Ok((image.as_raw().to_vec(), image.width(), image.height()))
        }
        "FlowField.render" => {
            let image = marea_frame(desc, t_ms)?;
            Ok((image.as_raw().to_vec(), image.width(), image.height()))
        }
        "orbit_tile" => {
            let image = orbit_frame(desc).ok_or_else(|| "sin marcas".to_string())?;
            Ok((image.as_raw().to_vec(), image.width(), image.height()))
        }
        "draw_text" => {
            let bytes = text_frame(desc)?;
            let look = Look::from_desc(desc);
            let style = ez_gfx::text::font_style(field(desc, "style").as_str().expect("style"))
                .expect("estilo");
            let text = field(desc, "text").as_str().expect("text");
            let (logical_w, logical_h) =
                text_size(text, style, look.dpi, None, "left").map_err(|e| e.to_string())?;
            Ok((
                bytes,
                (logical_w * look.dpi).ceil() as u32 + 8,
                (logical_h * look.dpi).ceil() as u32 + 8,
            ))
        }
        other => Err(format!("función sin imagen: {other}")),
    }
}
