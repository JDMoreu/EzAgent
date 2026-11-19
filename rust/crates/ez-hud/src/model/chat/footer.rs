//! Port of `src/ezagent/voice/hud/chat_footer.py` (F5-14).
//!
//! Footer of the chat view: multi-line text field, send button and the
//! confirmation card. The methods extend [`ChatView`], like the Python
//! `FooterMixin`.

use ez_core::pyvalue::{head_chars, py_strip};
use ez_gfx::sdf::Layer;
use ez_gfx::text::{baseline_top, cap_height, Align, FontStyle, FS_MICRO, FS_SECONDARY};

use crate::paint::chat::wrap_spans;
use crate::tokens::Rect;

use super::view::{canvas, ChatView};

/// Inner margin of the window.
pub const PAD: f64 = 16.0;
/// Diameter of the round buttons.
pub const BTN: f64 = 32.0;
/// Height of the input box with one line.
pub const INPUT_MIN: f64 = 46.0;
/// Extra height of every additional line.
pub const INPUT_LINE: f64 = 21.0;
/// Lines the input box shows before scrolling.
pub const INPUT_MAX_LINES: usize = 6;
/// Height of the hint strip under the input box.
pub const HINT_H: f64 = 0.0;
/// Radius of the caret bar.
const CARET_RADIUS: f64 = 0.8;
/// Width taken by the mic and speaker buttons.
pub const AUDIO_W: f64 = BTN * 2.0 + 16.0;

/// Height of the box: one line measures [`INPUT_MIN`] and each further line
/// adds [`INPUT_LINE`].
pub fn input_box_height(rows: usize) -> f64 {
    INPUT_MIN + rows.saturating_sub(1) as f64 * INPUT_LINE
}

/// Splits one logical line into runs that fit `width`: `(start, end)` each.
pub fn break_line(text: &str, width: f64, scale: f64) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut last_space: Option<usize> = None;
    for i in 0..=chars.len() {
        if i < chars.len() && chars[i] == ' ' {
            last_space = Some(i);
        }
        let candidate: String = chars[start..i.min(chars.len())].iter().collect();
        let overflows = i < chars.len()
            && canvas::body_length(&(candidate + &chars[i].to_string()), scale) > width;
        if i == chars.len() || overflows {
            if i == chars.len() {
                out.push((start, i));
                break;
            }
            let mut cut = match last_space {
                Some(space) if space >= start => space + 1,
                _ => i,
            };
            if cut <= start {
                cut = if i > start { i } else { start + 1 };
            }
            out.push((start, cut));
            start = cut;
            last_space = None;
        }
    }
    if out.is_empty() {
        vec![(0, chars.len())]
    } else {
        out
    }
}

impl ChatView {
    /// `_wrap_input`: visible lines of the field as `(start, end)` in the text.
    fn wrap_input(&self, width: f64) -> Vec<(usize, usize)> {
        let text = self.input.text();
        let mut lines = Vec::new();
        let mut start = 0usize;
        for logical in text.split('\n') {
            for (a, b) in break_line(logical, width, self.g.scale) {
                lines.push((start + a, start + b));
            }
            start += logical.chars().count() + 1;
        }
        if lines.is_empty() {
            vec![(0, 0)]
        } else {
            lines
        }
    }

    /// `_footer_height`: box plus the confirmation and viewing extras.
    pub(crate) fn footer_height(&mut self) -> f64 {
        let width = self.g.width - 2.0 * PAD - AUDIO_W - 12.0 - BTN - 14.0;
        self.input_lines = self.wrap_input(width);
        let rows = self.input_lines.len().min(INPUT_MAX_LINES);
        let box_h = input_box_height(rows);
        let mut extra = 0.0;
        if self.model.confirm().is_some() {
            extra += 84.0;
        }
        if self.model.viewing().is_some() {
            extra += 30.0;
        }
        box_h + HINT_H + 24.0 + extra
    }

    /// `_draw_footer`: viewing notice, confirmation, input box and tools.
    pub(crate) fn draw_footer(&mut self, buf: &mut Layer, now: f64, footer: f64, caret: bool) {
        let mut y = self.g.height - footer;
        if self.model.viewing().is_some() {
            self.text(
                buf,
                "Estás viendo una conversación anterior. Escribe para volver a la actual.",
                FS_MICRO,
                self.color("TEXT_3"),
                PAD,
                y + 8.0,
                Align::Left,
            );
            y += 30.0;
        }
        if self.model.confirm().is_some() {
            self.draw_confirm(buf, y + 2.0);
            y += 84.0;
        }
        let rows = self.input_lines.len().min(INPUT_MAX_LINES);
        let box_h = input_box_height(rows);
        let input = Rect::new(
            PAD + AUDIO_W,
            y + HINT_H + 6.0,
            self.g.width - PAD,
            y + HINT_H + 6.0 + box_h,
        );
        self.input_box = input;
        self.fill(buf, input, 16.0, self.color("GROUP"), 1.0);
        let outline = if self.input.empty() {
            self.color("LINE")
        } else {
            self.color("MINT")
        };
        self.stroke(buf, input, 16.0, outline, 0.7);
        self.draw_input_text(buf, input, now, caret);
        // Before the send button: the last registered zone wins.
        self.hit(input, "input", None);
        self.draw_send(buf, input);
        self.draw_composer_tools(buf, input.y1 - INPUT_MIN / 2.0);
    }

    /// Audio controls to the left, aligned with the composer row.
    fn draw_composer_tools(&mut self, buf: &mut Layer, cy: f64) {
        let muted = self.model.mic_muted() || !self.in_call;
        let voice = self.model.voice_replies();
        self.icon_button(
            buf,
            if muted {
                "microphone-slash"
            } else {
                "microphone"
            },
            PAD + BTN / 2.0,
            cy,
            "mic",
            !muted,
            false,
            true,
        );
        self.icon_button(
            buf,
            if voice { "speaker" } else { "speaker-slash" },
            PAD + BTN * 1.5 + 8.0,
            cy,
            "voice",
            voice,
            false,
            true,
        );
    }

    /// `_baseline` of row `row`: the capital-to-baseline band is centred.
    fn baseline(&self, input: Rect, row: usize) -> f64 {
        let cap = cap_height(body_style(), self.g.scale)
            .expect("the chat body style is known and the font is bundled");
        input.y0 + (INPUT_MIN + cap) / 2.0 + row as f64 * INPUT_LINE
    }

    /// `_draw_input_text`: hint or text lines, selection and caret.
    fn draw_input_text(&mut self, buf: &mut Layer, input: Rect, now: f64, caret: bool) {
        let x0 = input.x0 + 14.0;
        let body = body_style();
        let text = self.input.text().to_string();
        let first = self.input_lines.len().saturating_sub(INPUT_MAX_LINES);
        if self.input.empty() && text.is_empty() {
            let hint = "Escribe un mensaje…";
            let y = baseline_top(hint, body, self.baseline(input, 0), self.g.scale)
                .expect("the chat body style is known and the font is bundled");
            self.text(buf, hint, body, self.color("TEXT_3"), x0, y, Align::Left);
        }
        let selection = self.input.selection();
        let cap = cap_height(body, self.g.scale)
            .expect("the chat body style is known and the font is bundled");
        for (row, &(a, b)) in self.input_lines[first..].iter().enumerate() {
            let base = self.baseline(input, row);
            if let Some((start, end)) = selection {
                if start < b + 1 && end > a {
                    let from = start.max(a);
                    let to = end.min(b);
                    let x_a = canvas::body_length(canvas::slice(&text, a, from), self.g.scale);
                    let x_b = canvas::body_length(canvas::slice(&text, a, to), self.g.scale);
                    self.fill(
                        buf,
                        Rect::new(x0 + x_a, base - cap - 5.0, x0 + x_b + 3.0, base + 6.0),
                        4.0,
                        self.color("MINT"),
                        0.28,
                    );
                }
            }
            let line = canvas::slice(&text, a, b);
            if !py_strip(line).is_empty() {
                let y = baseline_top(line, body, base, self.g.scale)
                    .expect("the chat body style is known and the font is bundled");
                self.text(buf, line, body, self.color("TEXT"), x0, y, Align::Left);
            }
        }
        if caret {
            self.draw_caret(buf, now);
        }
    }

    /// The caret as its own little layer, to paint it over the content.
    ///
    /// DECISION: the patch uses the same geometry and the same colour as the
    /// caret inside the panel, so the pixels are the same ones (DX-1, paso 4).
    /// The origin is in physical pixels inside the content layer.
    pub(crate) fn caret_patch(&self, now: f64) -> Option<(i32, i32, Layer)> {
        let rect = self.caret_rect(now)?;
        let scale = self.g.scale;
        // The SDF of a rounded rectangle is evaluated with a margin of one and
        // a half logical pixels: the patch has to cover all of it.
        let x0 = ((rect.x0 - 1.5) * scale).floor() as i32;
        let y0 = ((rect.y0 - 1.5) * scale).floor() as i32;
        let x1 = ((rect.x1 + 1.5) * scale).ceil() as i32;
        let y1 = ((rect.y1 + 1.5) * scale).ceil() as i32;
        let mut patch = Layer::new((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);
        // The patch starts at an integer number of physical pixels, so the
        // coverage of the SDF is the same one the panel would have computed.
        let dx = f64::from(x0) / scale;
        let dy = f64::from(y0) / scale;
        canvas::fill_round_rect(
            &mut patch,
            Rect::new(rect.x0 - dx, rect.y0 - dy, rect.x1 - dx, rect.y1 - dy),
            CARET_RADIUS,
            self.color("MINT"),
            scale,
            1.0,
        );
        Some((x0, y0, patch))
    }

    /// `_draw_caret`: blinking bar at the cursor.
    fn draw_caret(&self, buf: &mut Layer, now: f64) {
        let Some(rect) = self.caret_rect(now) else {
            return;
        };
        canvas::fill_round_rect(buf, rect, CARET_RADIUS, self.color("MINT"), self.g.scale, 1.0);
    }

    /// Where the caret is, in logical pixels, when its phase shows it.
    ///
    /// DECISION: the caret is the only part of the panel that changes twice
    /// a second; it is painted apart from the content layer (DX-1, paso 4).
    pub(crate) fn caret_rect(&self, now: f64) -> Option<Rect> {
        // DECISION: Python reads `view._focus_t0` for the caret blink; F5-14
        // keeps it private, so the phase is derived from the clock instead.
        if ((now - self.focus_t0) * 2.0).trunc() as i64 % 2 != 0 {
            return None;
        }
        let cursor = self.input.cursor();
        let text = self.input.text();
        let lines = &self.input_lines;
        let first = lines.len().saturating_sub(INPUT_MAX_LINES);
        let cap = cap_height(body_style(), self.g.scale)
            .expect("the chat body style is known and the font is bundled");
        for (row, &(a, b)) in lines.iter().enumerate() {
            if a <= cursor && cursor <= b {
                if row < first {
                    return None;
                }
                let x = self.input_box.x0
                    + 14.0
                    + canvas::body_length(canvas::slice(text, a, cursor), self.g.scale);
                let base = self.baseline(self.input_box, row - first);
                return Some(Rect::new(x, base - cap - 3.0, x + 1.6, base + 4.0));
            }
        }
        None
    }

    /// `_draw_send`: round send/stop button.
    fn draw_send(&mut self, buf: &mut Layer, input: Rect) {
        let cx = input.x1 - 14.0 - BTN / 2.0 + 6.0;
        let cy = input.y1 - 8.0 - BTN / 2.0;
        let stop = self.model.busy();
        let ready = stop || !self.input.empty();
        let tone = if ready { "MINT_BTN" } else { "TRACK" };
        let alpha = if !ready || self.hover.as_deref() != Some("send") {
            1.0
        } else {
            0.88
        };
        self.circle(buf, cx, cy, BTN, self.color(tone), alpha);
        let ink = if ready { "MINT_INK" } else { "TEXT_3" };
        let name = if stop { "stop" } else { "send" };
        self.icon(buf, name, cx, cy, 17.0, self.color(ink), 1.0);
        let kind = if stop { "stop" } else { "send" };
        self.hit(
            Rect::new(
                cx - BTN / 2.0,
                cy - BTN / 2.0,
                cx + BTN / 2.0,
                cy + BTN / 2.0,
            ),
            kind,
            None,
        );
    }

    /// `_draw_confirm`: summary plus the Sí / No buttons.
    fn draw_confirm(&mut self, buf: &mut Layer, y: f64) {
        let Some(card) = self.model.confirm() else {
            return;
        };
        let rect = Rect::new(PAD, y, self.g.width - PAD, y + 76.0);
        self.fill(buf, rect, 14.0, self.color("WARM"), 0.14);
        self.stroke(buf, rect, 14.0, self.color("WARM"), 0.7);
        let width = self.g.width - 2.0 * PAD - 28.0;
        let mut lines = wrap_spans(
            &self.host.parse_inline(&card.summary),
            "small",
            width,
            self.g.scale,
        );
        lines.truncate(2);
        let text = lines
            .iter()
            .map(|line| line.iter().map(|run| run.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ");
        let shown = if text.chars().count() < 90 {
            text
        } else {
            format!("{}…", head_chars(&text, 89))
        };
        self.text(
            buf,
            &shown,
            FS_SECONDARY,
            self.color("TEXT"),
            PAD + 14.0,
            y + 12.0,
            Align::Left,
        );
        for (index, (label, key)) in [("Sí", "confirm_yes"), ("No", "confirm_no")]
            .into_iter()
            .enumerate()
        {
            let bx = PAD + 14.0 + index as f64 * 92.0;
            let button = Rect::new(bx, y + 40.0, bx + 82.0, y + 66.0);
            let primary = index == 0;
            let tone = if primary { "MINT_BTN" } else { "TRACK" };
            let alpha = if self.hover.as_deref() == Some(key) {
                0.9
            } else {
                1.0
            };
            self.fill(buf, button, 10.0, self.color(tone), alpha);
            let ink = if primary { "MINT_INK" } else { "TEXT" };
            self.text(
                buf,
                label,
                FS_SECONDARY,
                self.color(ink),
                bx + 41.0,
                y + 46.0,
                Align::Center,
            );
            self.hit(button, key, None);
        }
    }

    /// `_place_caret`: puts the caret under the pointer.
    pub(crate) fn place_caret(&mut self, x: f64, y: f64) {
        let input = self.input_box;
        let lines = self.input_lines.clone();
        let first = lines.len().saturating_sub(INPUT_MAX_LINES);
        // Each row's band is centred on its capital-to-baseline strip.
        let row = ((y - (input.y0 + (INPUT_MIN - INPUT_LINE) / 2.0)) / INPUT_LINE).floor() as i64;
        let last = lines.len().saturating_sub(first).saturating_sub(1) as i64;
        let row = row.clamp(0, last.max(0)) as usize + first;
        let (a, b) = lines[row];
        let text = self.input.text().to_string();
        let target = x - (input.x0 + 14.0);
        let mut best = a;
        for index in a..=b {
            if canvas::body_length(canvas::slice(&text, a, index), self.g.scale) <= target + 3.0 {
                best = index;
            }
        }
        self.input.place_caret(best);
        self.focus_t0 = ez_core::metrics::monotonic();
    }
}

fn body_style() -> FontStyle {
    ez_gfx::text::chat::style("body").expect("the chat styles are closed")
}

#[cfg(test)]
mod tests;
