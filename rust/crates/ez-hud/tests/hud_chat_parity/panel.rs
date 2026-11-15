//! Cuadros del panel completo (`ChatView.render`) contra los PNG dorados.

use std::path::Path;

use ez_gfx::sdf::Layer;
use ez_hud::model::chat::view::{ChatMessage, ChatView, ConfirmCard, Mode, Role, SessionInfo};
use ez_parity::fixtures::FixtureCase;

use super::support::{flag, line, num};
use super::view::{view as make_view, FakeModel};

macro_rules! message_from {
    ($raw:expr) => {{
        let raw = $raw;
        let role = match raw.get("role").and_then(|v| v.as_str()).unwrap_or("user") {
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => Role::User,
        };
        let wall = raw
            .get("t_wall")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|v| v.as_i64()).collect::<Vec<_>>())
            .unwrap_or_default();
        ChatMessage {
            id: raw.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
            role,
            text: raw
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: raw
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("chat")
                .to_string(),
            t: super::support::epoch_from_wall(&wall),
            streaming: raw
                .get("streaming")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            error: raw.get("error").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }};
}

macro_rules! session_from {
    ($raw:expr) => {{
        let raw = $raw;
        let wall = raw
            .get("started_wall")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|v| v.as_i64()).collect::<Vec<_>>())
            .unwrap_or_default();
        SessionInfo {
            session: raw.get("session").and_then(|v| v.as_i64()).unwrap_or(0),
            title: raw
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            started: super::support::epoch_from_wall(&wall),
            messages: raw.get("messages").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        }
    }};
}

pub(super) fn model_of(case: &FixtureCase) -> FakeModel {
    let messages = case
        .input
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().map(|raw| message_from!(raw)).collect())
        .unwrap_or_default();
    let sessions = case
        .input
        .get("history")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().map(|raw| session_from!(raw)).collect())
        .unwrap_or_default();
    let confirm = case
        .input
        .get("confirm")
        .filter(|value| !value.is_null())
        .map(|raw| ConfirmCard {
            summary: raw
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            strong: raw.get("strong").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    FakeModel {
        messages,
        sessions,
        busy: flag(case, "busy"),
        confirm,
        viewing: case.input.get("viewing").and_then(|v| v.as_i64()),
        current: case.input.get("current_session").and_then(|v| v.as_i64()),
        voice: flag(case, "voice_replies"),
        muted: flag(case, "mic_muted"),
    }
}

fn set_input(view: &mut ChatView, case: &FixtureCase) {
    let Some(input) = case.input.get("input") else {
        return;
    };
    let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if !text.is_empty() {
        view.input.insert(text);
    }
    let cursor = input
        .get("cursor")
        .and_then(|v| v.as_u64())
        .map(|value| value as usize);
    let anchor = input
        .get("anchor")
        .and_then(|v| v.as_u64())
        .map(|value| value as usize);
    match (cursor, anchor) {
        (Some(cursor), Some(anchor)) if cursor >= anchor => {
            view.input.place_caret(anchor);
            for _ in anchor..cursor {
                view.input
                    .move_caret(ez_hud::model::chat::view::Move::Right, true, false);
            }
        }
        (Some(cursor), Some(anchor)) => {
            view.input.place_caret(anchor);
            for _ in cursor..anchor {
                view.input
                    .move_caret(ez_hud::model::chat::view::Move::Left, true, false);
            }
        }
        (Some(cursor), None) => view.input.place_caret(cursor),
        _ => {}
    }
}

pub(super) fn panel(case: &FixtureCase) {
    let width = num(case, "width");
    let height = num(case, "height");
    let scale = num(case, "scale");
    let theme = line(case, "theme");
    let custom = case.input.get("custom").and_then(|v| v.as_object());
    if let Some(custom) = custom {
        let primary = custom
            .get("primary")
            .and_then(|v| v.as_str())
            .unwrap_or("#000000");
        let background = custom
            .get("background")
            .and_then(|v| v.as_str())
            .unwrap_or("#000000");
        ez_hud::tokens::set_themed(ez_hud::theme::derive(primary, background).as_ref());
    } else {
        ez_hud::tokens::set_themed(None);
    }
    let mut view: ChatView = make_view(model_of(case));
    view.configure(
        width,
        height,
        scale,
        &theme,
        ez_hud::tokens::palette(&theme).expect("la paleta del caso"),
    );
    view.mode = if line(case, "mode") == "history" {
        Mode::History
    } else {
        Mode::Chat
    };
    view.set_scroll(num(case, "scroll"));
    view.hover = case
        .input
        .get("hover")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    view.status = line(case, "status");
    view.in_call = flag(case, "in_call");
    view.set_focus_instant(num(case, "focus_t0"));
    set_input(&mut view, case);
    let pending = case.input.get("pending_delete").and_then(|v| v.as_i64());
    let deleting = case
        .input
        .get("deleting")
        .filter(|value| !value.is_null())
        .map(|raw| {
            let info = SessionInfo {
                session: raw.get("session").and_then(|v| v.as_i64()).unwrap_or(0),
                title: raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                started: {
                    let values = raw
                        .get("started_wall")
                        .and_then(|v| v.as_array())
                        .map(|items| items.iter().filter_map(|v| v.as_i64()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    super::support::epoch_from_wall(&values)
                },
                messages: raw.get("messages").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            };
            (
                raw.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                info,
                raw.get("at").and_then(|v| v.as_f64()).unwrap_or(0.0),
            )
        });
    view.set_history_state(pending, deleting);
    let layer = view.render(num(case, "now"));
    compare_png(case, &layer);
    ez_hud::tokens::set_themed(None);
}

fn compare_png(case: &FixtureCase, layer: &Layer) {
    let Some(png) = case.output.get("png").and_then(|v| v.as_str()) else {
        return;
    };
    let path = Path::new("../../../parity/golden").join(png);
    let expected = ez_parity::image::decode(&path).expect("PNG dorado");
    let produced = straight_float(layer);
    let delta = ez_parity::image::compare(&produced, &expected);
    let thresholds =
        ez_parity::image::Thresholds::load(Path::new("../../../parity/thresholds.toml"))
            .expect("umbrales")
            .pixels
            .text;
    if !ez_parity::image::within(&delta, &thresholds) {
        super::png_failures()
            .lock()
            .expect("sin envenenar")
            .push(format!(
                "{} ({png}): max={} p995={} p999={} ssim={}",
                case.case, delta.max, delta.p995, delta.p999, delta.ssim
            ));
    }
}

/// Panel float32 premultiplicado → PNG RGBA con alfa directo (`straight_float`).
fn straight_float(layer: &Layer) -> image::RgbaImage {
    let mut data = Vec::with_capacity(layer.as_raw().len());
    let width = layer.width();
    let height = layer.height();
    for pixel in layer.as_raw().as_chunks::<4>().0 {
        let alpha = pixel[3].clamp(0.0, 1.0);
        for channel in &pixel[..3] {
            let value = if pixel[3] > 0.0 {
                (channel / pixel[3].max(1e-6)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            data.push((value * 255.0).round_ties_even() as u8);
        }
        data.push((alpha * 255.0).round_ties_even() as u8);
    }
    image::RgbaImage::from_raw(width, height, data).expect("búfer RGBA del panel")
}
