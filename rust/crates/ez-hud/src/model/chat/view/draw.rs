//! Message painting of the chat view (`chat_view.py`, F5-14).

use ez_gfx::sdf::Layer;
use ez_gfx::text::FontStyle;
use ez_gfx::Rgba;

use crate::paint::chat::labels::{self, draw_label, draw_stamp};
use crate::tokens::Rect;

use super::entries::Entry;
use super::{canvas, ChatMessage, ChatView, Role, GAP, PAD};

impl ChatView {
    /// `_draw_messages`: lays out and paints the current conversation.
    pub(crate) fn draw_messages(&mut self, buf: &mut Layer, now: f64, top: f64, bottom: f64) {
        let (_, messages) = self.model.snapshot();
        let viewport = bottom - top;
        if messages.is_empty() {
            self.draw_empty(buf, top, bottom);
            self.scroll.set_max(0.0);
            return;
        }
        let entries: Vec<std::rc::Rc<Entry>> =
            messages.iter().map(|msg| self.entry(msg, now)).collect();
        let stamps: Vec<f64> = messages.iter().map(labels::stamp_height).collect();
        let mut total = PAD
            + entries
                .iter()
                .zip(&stamps)
                .map(|(entry, stamp)| entry.height + stamp)
                .sum::<f64>();
        total += GAP * (entries.len() as f64 - 1.0) + GAP;
        self.scroll.set_max((total - viewport).max(0.0));
        let mut y = top - (self.scroll.max() - self.scroll.offset()) + PAD;
        for ((msg, entry), stamp) in messages.iter().zip(&entries).zip(&stamps) {
            self.draw_message(buf, msg, entry, y, top, bottom, now);
            if *stamp > 0.0 && y + entry.height >= top && y + entry.height + stamp <= bottom {
                let width = entry.image.width() as f64 / self.g.scale;
                let scale = self.g.scale;
                let ink = self.color("TEXT_3");
                let bottom_edge = y + entry.height;
                let mut draw =
                    |text: &str, style: FontStyle, color: Rgba, x: f64, at: f64, align| {
                        canvas::text(buf, text, style, color, x, at, scale, align);
                    };
                draw_stamp(&mut draw, msg, entry.x, width, bottom_edge, ink, scale);
            }
            y += entry.height + stamp + GAP;
        }
        if self.scroll.max() > 0.0 {
            self.draw_scrollbar(buf, top, viewport, total);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_message(
        &mut self,
        buf: &mut Layer,
        msg: &ChatMessage,
        entry: &Entry,
        y: f64,
        top: f64,
        bottom: f64,
        now: f64,
    ) {
        if y + entry.height < top || y > bottom {
            return;
        }
        canvas::blit(buf, &entry.image, entry.x, y, top, bottom, self.g.scale);
        let scale = self.g.scale;
        let ink = self.color("TEXT_3");
        if msg.role == Role::Assistant && y >= top - 1.0 && y + 16.0 <= bottom {
            {
                let mut draw =
                    |text: &str, style: FontStyle, color: Rgba, x: f64, at: f64, align| {
                        canvas::text(buf, text, style, color, x, at, scale, align);
                    };
                draw_label(&mut draw, "EZAGENT", PAD, y, ink, scale, false);
            }
            if msg.streaming && msg.text.is_empty() && y + 34.0 <= bottom {
                self.draw_dots(buf, PAD + 12.0, y + 40.0, now);
            }
        } else if msg.role == Role::User && y >= top - 1.0 && y + 16.0 <= bottom {
            let label = if msg.source == "voice" {
                "TÚ · AUDIO"
            } else {
                "TÚ"
            };
            let edge = entry.x + entry.image.width() as f64 / scale;
            let mut draw = |text: &str, style: FontStyle, color: Rgba, x: f64, at: f64, align| {
                canvas::text(buf, text, style, color, x, at, scale, align);
            };
            draw_label(&mut draw, label, edge, y, ink, scale, true);
        }
        self.draw_copy_label(buf, msg, entry, y, top, bottom, now);
        self.register_message_hits(entry, y, top, bottom);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_copy_label(
        &self,
        buf: &mut Layer,
        msg: &ChatMessage,
        entry: &Entry,
        y: f64,
        top: f64,
        bottom: f64,
        now: f64,
    ) {
        let Some(caption) = entry.caption_hit else {
            return;
        };
        if y + caption.y0 < top || y + caption.y1 > bottom {
            return;
        }
        let copied = self
            .flash
            .as_ref()
            .is_some_and(|(text, until)| text == &entry.plain && now < *until);
        let tone = if copied {
            "MINT"
        } else if self.hover.as_deref() == Some("copy_msg") {
            "TEXT"
        } else {
            "TEXT_2"
        };
        let cy = y + (caption.y0 + caption.y1) / 2.0;
        let cx = entry.x + (caption.x0 + caption.x1) / 2.0;
        canvas::fill_circle(buf, cx, cy, 10.5, self.copy_fill(msg), self.g.scale, 1.0);
        canvas::icon(
            buf,
            if copied { "check" } else { "copy" },
            cx,
            cy,
            15.0,
            self.color(tone),
            self.g.scale,
            1.0,
        );
    }

    fn copy_fill(&self, msg: &ChatMessage) -> Rgba {
        if msg.role != Role::User {
            return self.color("GROUP");
        }
        let mint = self.color("MINT");
        let body = self.color("BODY");
        let mix =
            |a: u8, b: u8| (f64::from(a) * 0.15 + f64::from(b) * 0.85).round_ties_even() as u8;
        Rgba::opaque(
            mix(mint.r, body.r),
            mix(mint.g, body.g),
            mix(mint.b, body.b),
        )
    }

    fn register_message_hits(&mut self, entry: &Entry, y: f64, top: f64, bottom: f64) {
        for zone in &entry.hits {
            let rect = Rect::new(
                entry.x + zone.rect.x0,
                y + zone.rect.y0,
                entry.x + zone.rect.x1,
                y + zone.rect.y1,
            );
            if rect.y1 > top && rect.y0 < bottom {
                self.hit(rect, zone.kind, zone.data.clone());
            }
        }
        if let Some(caption) = entry.caption_hit {
            if y + caption.y0 >= top && y + caption.y1 <= bottom {
                self.hit(
                    Rect::new(
                        entry.x + caption.x0,
                        y + caption.y0,
                        entry.x + caption.x1,
                        y + caption.y1,
                    ),
                    "copy_msg",
                    Some(super::ZoneData::Text(entry.plain.clone())),
                );
            }
        }
    }

    fn draw_dots(&self, buf: &mut Layer, x: f64, y: f64, now: f64) {
        for index in 0..3 {
            let phase = (now * 2.4 - index as f64 * 0.22).rem_euclid(1.0);
            let k = 0.35 + 0.65 * (1.0 - (phase - 0.35).abs() * 3.0).max(0.0);
            canvas::fill_circle(
                buf,
                x + 5.0 + index as f64 * 11.0,
                y,
                6.0,
                self.color("TEXT_2"),
                self.g.scale,
                k,
            );
        }
    }

    fn draw_scrollbar(&self, buf: &mut Layer, top: f64, viewport: f64, total: f64) {
        if let Some(thumb) = self.scroll.thumb(top, viewport, total, self.g.width) {
            canvas::fill_round_rect(buf, thumb, 1.5, self.color("TEXT_3"), self.g.scale, 0.55);
        }
    }
}
