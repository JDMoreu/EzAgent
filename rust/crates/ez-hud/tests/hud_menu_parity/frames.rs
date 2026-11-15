//! Cuadros PNG del menú y de la ventana de ajustes.

use std::path::{Path, PathBuf};

use ez_gfx::sdf::Layer;
use ez_hud::model::menu::geometry::Menu;
use ez_hud::paint::menu::advanced::Advanced;
use ez_hud::paint::settings_window::{self as sw, Sidebar};
use ez_hud::tokens;
use ez_parity::fixtures::FixtureCase;
use ez_parity::image as parity_image;
use ez_parity::image::PixelThresholds;

use super::replay::apply_step;
use super::support::*;

fn golden(png: &str) -> PathBuf {
    Path::new("../../../parity/golden").join(png)
}

fn thresholds() -> PixelThresholds {
    let all = parity_image::Thresholds::load(Path::new("../../../parity/thresholds.toml"))
        .expect("umbrales legibles");
    all.pixels.text
}

fn straight(layer: &Layer) -> image::RgbaImage {
    let mut data = Vec::with_capacity((layer.width() * layer.height() * 4) as usize);
    for pixel in layer.as_raw().as_chunks::<4>().0 {
        let alpha = pixel[3].clamp(0.0, 1.0);
        for channel in &pixel[..3] {
            let value = if alpha > 0.0 {
                (channel / alpha).clamp(0.0, 1.0)
            } else {
                0.0
            };
            data.push((f64::from(value) * 255.0).round_ties_even() as u8);
        }
        data.push((f64::from(alpha) * 255.0).round_ties_even() as u8);
    }
    image::RgbaImage::from_raw(layer.width(), layer.height(), data).expect("RGBA del tamaño")
}

fn check(case: &FixtureCase, png: &str, layer: &Layer) {
    let produced = straight(layer);
    let expected = parity_image::decode(&golden(png)).expect("PNG dorado legible");
    let delta = parity_image::compare(&produced, &expected);
    assert!(
        parity_image::within(&delta, &thresholds()),
        "{} ({}): max={} p995={} p999={} ssim={}",
        case.case,
        png,
        delta.max,
        delta.p995,
        delta.p999,
        delta.ssim
    );
}

fn frame_time(case: &FixtureCase) -> f64 {
    case.case
        .rsplit('@')
        .next()
        .and_then(|text| text.parse::<f64>().ok())
        .map(|ms| ms / 1000.0)
        .unwrap_or(1.2)
}

macro_rules! steps_of {
    ($case:expr) => {{
        let mut steps: Vec<_> = $case
            .input
            .get("actions")
            .and_then(|value| value.as_array())
            .map(|items| items.iter().collect())
            .unwrap_or_default();
        steps.sort_by(|left, right| {
            let a = left.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = right.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
            a.total_cmp(&b)
        });
        steps
    }};
}

fn replay(menu: &mut Menu, case: &FixtureCase, recorder: &Recorder, target: f64) {
    let mut clock = 0.0;
    for step in steps_of!(case) {
        let t = step.get("t").and_then(|v| v.as_f64()).unwrap_or(clock);
        while clock < t - 1e-9 {
            clock = (clock + 1.0 / 60.0).min(t);
            menu.update(clock);
        }
        if t <= target + 1e-9 {
            apply_step!(menu, recorder, step);
            clock = clock.max(t);
        }
    }
    while clock < target - 1e-9 {
        clock = (clock + 1.0 / 60.0).min(target);
        menu.update(clock);
    }
}

fn dpi(case: &FixtureCase) -> f64 {
    case.input
        .get("dpi")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
}

pub(super) fn menu_frame(case: &FixtureCase) {
    activate(case);
    let recorder = Recorder::new(case, settings_of(case).saved_snapshot());
    let mut menu = build_menu(case, &recorder);
    let t = frame_time(case);
    replay(&mut menu, case, &recorder, t);
    let scale = dpi(case);
    let width = ((menu.width() * scale).round_ties_even() as u32).max(1);
    let height =
        (((f64::from(tokens::H_ISLAND) + menu.viewport_height()) * scale).round_ties_even() as u32)
            .max(1);
    let mut layer = Layer::new(width, height);
    menu.draw_with(&mut layer, t, &mut Advanced);
    let png = case
        .output
        .get("png")
        .and_then(|value| value.as_str())
        .expect("PNG del caso");
    check(case, png, &layer);
}

pub(super) fn settings_frame(case: &FixtureCase) {
    activate(case);
    let recorder = Recorder::new(case, settings_of(case).saved_snapshot());
    let mut menu = build_menu(case, &recorder);
    let window = case.input.get("window").and_then(|v| v.as_array());
    let (win_w, win_h) = window.map_or((760.0, 620.0), |items| {
        (
            items.first().and_then(|v| v.as_f64()).unwrap_or(760.0),
            items.get(1).and_then(|v| v.as_f64()).unwrap_or(620.0),
        )
    });
    let viewport = sw::content_size(win_w, win_h).1;
    menu.set_max_height(Some(viewport));
    let section = case
        .input
        .get("section")
        .and_then(|v| v.as_str())
        .unwrap_or("appearance");
    let t = frame_time(case);
    replay(&mut menu, case, &recorder, t);
    let scale = dpi(case);
    let width = ((win_w * scale).round_ties_even() as u32).max(1);
    let height = ((win_h * scale).round_ties_even() as u32).max(1);
    let base = case
        .input
        .get("base")
        .and_then(|v| v.as_str())
        .unwrap_or("dark");
    let body = ez_hud::paint::drop::render_card_body(
        width,
        height,
        scale,
        base,
        true,
        tokens::palette_version(),
    )
    .expect("cuerpo de la tarjeta");
    let mut frame = Layer::from_raw(width, height, body.to_vec()).expect("tarjeta");
    let hover = case.input.get("sidebar_hover").and_then(|v| v.as_str());
    Sidebar::new().draw(
        &mut frame,
        scale,
        &ez_hud::theme::card_palette(base),
        base,
        section,
        hover,
        t,
        false,
        true,
    );
    let content_w = ((menu.width() * scale).round_ties_even() as u32).max(1);
    let y0 = (f64::from(tokens::H_ISLAND) * scale).round_ties_even() as u32;
    let content_h = y0 + (((viewport + 4.0) * scale).round_ties_even() as u32).max(1);
    let mut content = Layer::new(content_w, content_h);
    menu.draw_with(&mut content, t, &mut Advanced);
    let slice_h = height.min(content_h.saturating_sub(y0));
    if slice_h > 0 {
        let start = (y0 as usize * content_w as usize) * 4;
        let end = start + slice_h as usize * content_w as usize * 4;
        let slice = Layer::from_raw(content_w, slice_h, content.as_raw()[start..end].to_vec())
            .expect("recorte del contenido");
        sw::composite_over(
            &mut frame,
            &slice,
            (sw::SIDEBAR_W * scale).round_ties_even() as usize,
            0,
        );
    }
    sw::draw_grip(&mut frame, scale, &ez_hud::theme::card_palette(base));
    let amount = case
        .input
        .get("fade")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    sw::fade(&mut frame, amount);
    let png = case
        .output
        .get("png")
        .and_then(|value| value.as_str())
        .expect("PNG del caso");
    check(case, png, &frame);
}
