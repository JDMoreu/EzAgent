//! Port of `src/ezagent/voice/hud/chat_labels.py` (F5-13).
//!
//! Labels of the chat messages: «TÚ» / «EZAGENT» on top and the date and time
//! underneath. Kept apart from the view so that module does not grow further;
//! the view passes its own text painter and colours.

use ez_core::pytime;
use ez_gfx::text::{baseline_top, cap_height, Align, FontStyle};
use ez_gfx::Rgba;

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

/// Date and time under a message.
pub const FS_STAMP: FontStyle = FontStyle::new("CHAT_STAMP", "Geist", 11.0, 400);
/// Height reserved under every user and EzAgent message.
pub const STAMP_H: f64 = 16.0;
/// The time is aligned with the bubble edge, inside its curve.
pub const STAMP_INSET: f64 = 4.0;

/// Text painter of the view: `(text, style, color, x, top, align)`.
pub type Draw<'a> = dyn FnMut(&str, FontStyle, Rgba, f64, f64, Align) + 'a;

/// What a label needs from a chat message (`getattr(msg, …)` in Python).
pub trait LabelMessage {
    /// `msg.role`.
    fn role(&self) -> &str;
    /// `msg.streaming`.
    fn streaming(&self) -> bool;
    /// `msg.t`.
    fn timestamp(&self) -> f64;
}

/// Height of the date-and-time line under `msg` (also while streaming, so the
/// layout does not jump).
pub fn stamp_height<M: LabelMessage + ?Sized>(msg: &M) -> f64 {
    match msg.role() {
        "user" | "assistant" => STAMP_H,
        _ => 0.0,
    }
}

/// Label above the message, placed by its baseline (the accent of «TÚ» used
/// to sink it).
pub fn draw_label(
    draw: &mut Draw<'_>,
    label: &str,
    x: f64,
    y: f64,
    color: Rgba,
    scale: f64,
    right: bool,
) {
    let base = y
        + 2.0
        + cap_height(ez_gfx::text::FS_MICRO, scale)
            .expect("the HUD micro style is known and the font is bundled");
    let top = baseline_top(label, ez_gfx::text::FS_MICRO, base, scale)
        .expect("the HUD micro style is known and the font is bundled");
    draw(label, ez_gfx::text::FS_MICRO, color, x, top, align(right));
}

/// Date and time under the message, aligned with its bubble edge.
///
/// `x` and `width` are the bubble's; `y` is its bottom edge. User messages
/// (on the right) align the time to the right.
pub fn draw_stamp<M: LabelMessage + ?Sized>(
    draw: &mut Draw<'_>,
    msg: &M,
    x: f64,
    width: f64,
    y: f64,
    color: Rgba,
    scale: f64,
) {
    let text = stamp_for(msg);
    if text.is_empty() {
        return;
    }
    let right = msg.role() == "user";
    let base = y
        + 4.0
        + cap_height(FS_STAMP, scale).expect("the stamp style is known and the font is bundled");
    let x = if right {
        x + width - STAMP_INSET
    } else {
        x + STAMP_INSET
    };
    let top = baseline_top(&text, FS_STAMP, base, scale)
        .expect("the stamp style is known and the font is bundled");
    draw(&text, FS_STAMP, color, x, top, align(right));
}

/// Date and time of a user or EzAgent message; "" while streaming.
pub fn stamp_for<M: LabelMessage + ?Sized>(msg: &M) -> String {
    if msg.role() != "user" && msg.role() != "assistant" {
        return String::new();
    }
    if msg.streaming() {
        return String::new();
    }
    stamp(msg.timestamp())
}

fn align(right: bool) -> Align {
    if right {
        Align::Right
    } else {
        Align::Left
    }
}

/// `trace.stamp`: «13 nov 2026 · 16:46:12» in local time.
// DECISION: `ez-hud` cannot depend on `ez-chat::trace`, so the formatter is
// repeated here; on an out-of-range instant it warns and returns "" like the
// `ez-chat` port.
fn stamp(t: f64) -> String {
    let Ok(iso) = pytime::local_iso_seconds(t.trunc()) else {
        tracing::warn!("No pude formatear la hora del chat: {t}");
        return String::new();
    };
    let Some((date, rest)) = iso.split_once('T') else {
        return String::new();
    };
    let mut parts = date.split('-');
    let year = parts.next().unwrap_or("1970");
    let month: usize = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let day: u32 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let clock = rest.get(..8).unwrap_or("00:00:00");
    let mut fields = clock.split(':');
    let hour = fields.next().unwrap_or("00");
    let minute = fields.next().unwrap_or("00");
    let second = fields.next().unwrap_or("00");
    format!(
        "{day} {} {year} · {hour}:{minute}:{second}",
        MONTHS[month.clamp(1, 12) - 1]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Msg {
        role: &'static str,
        streaming: bool,
        t: f64,
    }

    impl LabelMessage for Msg {
        fn role(&self) -> &str {
            self.role
        }

        fn streaming(&self) -> bool {
            self.streaming
        }

        fn timestamp(&self) -> f64 {
            self.t
        }
    }

    fn msg(role: &'static str) -> Msg {
        Msg {
            role,
            streaming: false,
            t: 1_735_689_612.0,
        }
    }

    #[test]
    fn only_user_and_assistant_messages_reserve_a_stamp() {
        assert_eq!(stamp_height(&msg("user")), STAMP_H);
        assert_eq!(stamp_height(&msg("assistant")), STAMP_H);
        assert_eq!(stamp_height(&msg("system")), 0.0);
    }

    #[test]
    fn streaming_and_foreign_roles_have_no_stamp() {
        assert_eq!(
            stamp_for(&Msg {
                streaming: true,
                ..msg("assistant")
            }),
            ""
        );
        assert_eq!(stamp_for(&msg("system")), "");
        assert!(!stamp_for(&msg("user")).is_empty());
    }

    #[test]
    fn stamp_matches_the_trace_text() {
        let t: f64 = 1_735_689_612.0;
        let iso = pytime::local_iso_seconds(t.trunc()).expect("valid instant");
        let date = &iso[..10];
        let clock = &iso[11..19];
        let mut parts = date.split('-');
        let year = parts.next().expect("year");
        let month: usize = parts.next().expect("month").parse().expect("number");
        let day: u32 = parts.next().expect("day").parse().expect("number");
        let expected = format!("{day} {} {year} · {clock}", MONTHS[month - 1]);
        assert_eq!(stamp(t), expected);
    }

    #[test]
    fn draw_label_places_the_text_by_its_baseline() {
        let mut calls = Vec::new();
        {
            let mut draw = |text: &str, style: FontStyle, color: Rgba, x: f64, y: f64, align| {
                calls.push((text.to_string(), style.token, color, x, y, align));
            };
            draw_label(
                &mut draw,
                "TÚ",
                10.0,
                20.0,
                Rgba::opaque(138, 164, 163),
                1.0,
                false,
            );
        }
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "TÚ");
        assert_eq!(calls[0].1, "FS_MICRO");
        assert_eq!(calls[0].3, 10.0);
        assert_eq!(calls[0].5, Align::Left);
        assert!(calls[0].4 <= 20.0);
    }

    #[test]
    fn draw_stamp_aligns_user_messages_to_the_right() {
        let mut calls = Vec::new();
        {
            let mut draw = |text: &str, _, _, x: f64, _, align| {
                calls.push((text.to_string(), x, align));
            };
            draw_stamp(
                &mut draw,
                &msg("user"),
                10.0,
                100.0,
                50.0,
                Rgba::opaque(1, 2, 3),
                1.0,
            );
        }
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, stamp_for(&msg("user")));
        assert_eq!(calls[0].1, 10.0 + 100.0 - STAMP_INSET);
        assert_eq!(calls[0].2, Align::Right);
    }

    #[test]
    fn draw_stamp_skips_messages_without_a_stamp() {
        let mut calls = 0;
        {
            let mut draw = |_: &str, _: FontStyle, _: Rgba, _: f64, _: f64, _: Align| {
                calls += 1;
            };
            draw_stamp(
                &mut draw,
                &msg("system"),
                0.0,
                100.0,
                50.0,
                Rgba::opaque(1, 2, 3),
                1.0,
            );
        }
        assert_eq!(calls, 0);
    }
}
