//! Port of `src/ezagent/voice/hud/chat_header.py` (F5-14).
//!
//! Header, conversation history and empty state of the chat view. The methods
//! extend [`ChatView`] and use its state, like the Python `HeaderMixin`.

use ez_core::pyvalue::{collapse_spaces, head_chars};
use ez_gfx::sdf::Layer;
use ez_gfx::text::{baseline_top, cap_height, Align, FS_MICRO, FS_ROW, FS_SECONDARY, FS_TITLE};

use crate::tokens::Rect;

use super::footer::{BTN, PAD};
use super::view::{canvas, ChatView, Mode, Role, SessionInfo, ZoneData};

/// Height of the header band.
pub const HEADER_H: f64 = 56.0;
/// History row fading out after its deletion (seconds).
pub const DELETE_SECS: f64 = 0.18;
/// Logical diameter of the live orb beside the title.
pub const ORB_D: f64 = 28.0;
/// «Nueva conversación» row of the history: the height of a compact row.
pub const NEW_ROW_H: f64 = 46.0;
/// Logical height of a history row.
const ROW_H: f64 = 60.0;
const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

/// `time.localtime(t)` as «13 nov 2026».
fn date(t: f64) -> String {
    let iso = ez_core::pytime::date_local(t.trunc()).unwrap_or_default();
    let mut parts = iso.split('-');
    let year = parts.next().unwrap_or("1970");
    let month: usize = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let day: u32 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    format!("{day} {} {year}", MONTHS[month.clamp(1, 12) - 1])
}

/// `title` cut to 39 characters plus «…» when it reaches 40.
fn short_title(title: &str) -> String {
    if title.chars().count() < 40 {
        title.to_string()
    } else {
        format!("{}…", head_chars(title, 39))
    }
}

impl ChatView {
    fn conversation_title(&self) -> String {
        // DECISION: reuse the title derived from the first request and stored
        // for history, so reopening a session shows the same name.
        let selected = self.model.viewing().or(self.model.current_session());
        for session in self.model.sessions() {
            if Some(session.session) == selected {
                return if session.title.is_empty() {
                    "Nueva conversación".to_string()
                } else {
                    session.title
                };
            }
        }
        let (_, messages) = self.model.snapshot();
        let first = messages
            .iter()
            .find(|message| message.role == Role::User)
            .map_or("", |message| message.text.as_str());
        let title = collapse_spaces(first);
        if title.is_empty() {
            "Nueva conversación".to_string()
        } else {
            title
        }
    }

    /// `_draw_empty`: welcome title and help lines.
    pub(crate) fn draw_empty(&mut self, buf: &mut Layer, top: f64, bottom: f64) {
        let mid = (top + bottom) / 2.0 - 18.0;
        let cx = self.g.width / 2.0;
        self.text(
            buf,
            "¿En qué te ayudo?",
            FS_TITLE,
            self.color("TEXT"),
            cx,
            mid - 14.0,
            Align::Center,
        );
        let lines = [
            "Escribe aquí lo que necesites.",
            "Puedo resumir, comparar y mostrarte",
            "documentos e imágenes.",
        ];
        for (index, line) in lines.iter().enumerate() {
            self.text(
                buf,
                line,
                FS_SECONDARY,
                self.color("TEXT_3"),
                cx,
                mid + 14.0 + index as f64 * 18.0,
                Align::Center,
            );
        }
    }

    // -------------------------------------------------------------- history
    /// `_draw_history`: «new conversation» plus the saved conversations.
    pub(crate) fn draw_history(&mut self, buf: &mut Layer, top: f64, bottom: f64, now: f64) {
        let mut sessions = self.model.sessions();
        self.reconcile_ghost(&mut sessions, now);
        let ghost = self.deleting.clone();
        let mut y = top + 8.0 - self.scroll.offset();
        self.draw_new_conversation(buf, y, bottom);
        y += 56.0;
        let mut total = 64.0;
        if sessions.is_empty() {
            self.text(
                buf,
                "Aún no hay conversaciones guardadas.",
                FS_ROW,
                self.color("TEXT_3"),
                PAD,
                y + 16.0,
                Align::Left,
            );
            self.scroll.set_max(0.0);
            return;
        }
        let busy = self.model.busy();
        let current = self.model.current_session();
        if busy {
            self.text(
                buf,
                "Detén la respuesta para cambiar de conversación.",
                FS_MICRO,
                self.color("TEXT_3"),
                PAD,
                y + 4.0,
                Align::Left,
            );
            y += 22.0;
            total += 22.0;
        }
        let progress = ghost.as_ref().map_or(0.0, |ghost| {
            let raw = ((now - ghost.2) / DELETE_SECS).clamp(0.0, 1.0);
            raw * raw * (3.0 - 2.0 * raw)
        });
        for info in &sessions {
            let fading = ghost
                .as_ref()
                .is_some_and(|ghost| info.session == ghost.1.session);
            let row_h = if fading {
                ROW_H * (1.0 - progress)
            } else {
                ROW_H
            };
            total += row_h;
            let is_current = current == Some(info.session);
            if row_h > 1.0 && y + row_h > top && y < bottom {
                if fading {
                    self.draw_ghost_row(buf, info, y, row_h, busy, is_current, 1.0 - progress);
                } else {
                    self.draw_history_row(buf, info, y, busy, is_current, true);
                }
            }
            y += row_h;
        }
        self.scroll.set_max((total - (bottom - top)).max(0.0));
    }

    /// Keeps or drops the fading row: it only exists while the conversation is
    /// still gone from the list and the fade has not ended.
    fn reconcile_ghost(&mut self, sessions: &mut Vec<SessionInfo>, now: f64) {
        let Some(ghost) = self.deleting.clone() else {
            return;
        };
        if now - ghost.2 >= DELETE_SECS || sessions.iter().any(|row| row.session == ghost.1.session)
        {
            self.deleting = None;
        } else {
            let index = ghost.0.min(sessions.len());
            sessions.insert(index, ghost.1);
        }
    }

    /// `_draw_new_conversation`: one more history row, painted in the accent.
    fn draw_new_conversation(&mut self, buf: &mut Layer, y: f64, bottom: f64) {
        if y < HEADER_H || y + NEW_ROW_H > bottom {
            return;
        }
        let x0 = 8.0;
        let x1 = self.g.width - 8.0;
        let hovered = self.hover.as_deref() == Some("new");
        let mint = self.color("MINT");
        // Card surface plus an accent veil: it reads as a row in both themes.
        self.fill(
            buf,
            Rect::new(x0, y, x1, y + NEW_ROW_H),
            12.0,
            self.color("GROUP"),
            1.0,
        );
        self.fill(
            buf,
            Rect::new(x0, y, x1, y + NEW_ROW_H),
            12.0,
            mint,
            if hovered { 0.18 } else { 0.1 },
        );
        let cy = y + NEW_ROW_H / 2.0;
        self.icon(buf, "compose", 20.0 + 9.0, cy, 18.0, mint, 1.0);
        let label = "Nueva conversación";
        let base = cy
            + cap_height(FS_ROW, self.g.scale)
                .expect("the HUD row style is known and the font is bundled")
                / 2.0;
        let top = baseline_top(label, FS_ROW, base, self.g.scale)
            .expect("the HUD row style is known and the font is bundled");
        self.text(buf, label, FS_ROW, mint, 20.0 + 28.0, top, Align::Left);
        self.hit(Rect::new(x0, y, x1, y + NEW_ROW_H), "new", None);
    }

    /// `_draw_ghost_row`: a deleted row shrinks and fades while the others rise.
    #[allow(clippy::too_many_arguments)]
    fn draw_ghost_row(
        &mut self,
        buf: &mut Layer,
        info: &SessionInfo,
        y: f64,
        height: f64,
        busy: bool,
        is_current: bool,
        alpha: f64,
    ) {
        let rows = (height * self.g.scale).round_ties_even().max(1.0) as u32;
        let mut tmp = Layer::new(buf.width(), rows);
        self.draw_history_row(&mut tmp, info, 0.0, busy, is_current, false);
        canvas::composite_over_alpha(buf, &tmp, 0, canvas::px(y * self.g.scale), alpha as f32);
    }

    /// `_draw_history_row`: title, date and controls of one conversation.
    // DECISION: the Python default `interactive=True` is passed explicitly;
    // the ghost row is the only caller that turns it off.
    #[allow(clippy::too_many_arguments)]
    fn draw_history_row(
        &mut self,
        buf: &mut Layer,
        info: &SessionInfo,
        y: f64,
        busy: bool,
        is_current: bool,
        interactive: bool,
    ) {
        let width = self.g.width;
        let asking = self.pending_delete == Some(info.session);
        let right = width - 8.0 - 24.0;
        let rect = Rect::new(
            8.0,
            y,
            width - 8.0 - if asking { 84.0 } else { 44.0 },
            y + 58.0,
        );
        if interactive
            && self.hover.as_deref() == Some(&format!("session:{}", info.session))
            && !busy
        {
            self.fill(
                buf,
                Rect::new(8.0, y, width - 8.0, y + 58.0),
                12.0,
                self.color("GROUP_HOVER"),
                1.0,
            );
        }
        let tone = if busy { "TEXT_3" } else { "TEXT" };
        if asking {
            self.draw_delete_question(buf, info, y, right);
            return;
        }
        self.draw_session_row(
            buf,
            info,
            y,
            tone,
            right,
            is_current,
            busy,
            interactive,
            rect,
        );
    }

    /// Row asking «¿Eliminar esta conversación?» with its two buttons.
    fn draw_delete_question(&mut self, buf: &mut Layer, info: &SessionInfo, y: f64, right: f64) {
        self.text(
            buf,
            "¿Eliminar esta conversación?",
            FS_ROW,
            self.color("ERR"),
            20.0,
            y + 20.0,
            Align::Left,
        );
        self.row_button(
            buf,
            "check",
            right - 40.0,
            y + 29.0,
            "delete_yes",
            info.session,
            "ERR",
        );
        self.row_button(
            buf,
            "close",
            right,
            y + 29.0,
            "delete_no",
            info.session,
            "TEXT_2",
        );
    }

    /// Title, date and controls of a saved conversation.
    #[allow(clippy::too_many_arguments)]
    fn draw_session_row(
        &mut self,
        buf: &mut Layer,
        info: &SessionInfo,
        y: f64,
        tone: &str,
        right: f64,
        is_current: bool,
        busy: bool,
        interactive: bool,
        rect: Rect,
    ) {
        let title = short_title(&info.title);
        self.text(
            buf,
            &title,
            FS_ROW,
            self.color(tone),
            20.0,
            y + 11.0,
            Align::Left,
        );
        let meta = format!("{} · {} mensajes", date(info.started), info.messages);
        self.text(
            buf,
            &meta,
            FS_MICRO,
            self.color("TEXT_3"),
            20.0,
            y + 35.0,
            Align::Left,
        );
        if is_current {
            self.text(
                buf,
                "ACTUAL",
                FS_MICRO,
                self.color("MINT"),
                right - 16.0,
                y + 35.0,
                Align::Right,
            );
        }
        if !busy && interactive {
            self.hit(rect, "session", Some(ZoneData::Session(info.session)));
            self.row_button(
                buf,
                "trash",
                right,
                y + 29.0,
                "delete_ask",
                info.session,
                "TEXT_3",
            );
        }
    }

    /// `_row_button`: round button of a history row (delete, confirm, cancel).
    #[allow(clippy::too_many_arguments)]
    fn row_button(
        &mut self,
        buf: &mut Layer,
        icon: &str,
        cx: f64,
        cy: f64,
        kind: &'static str,
        session: i64,
        tone: &str,
    ) {
        let hovered = self.hover.as_deref() == Some(&format!("{kind}:{session}"));
        if hovered {
            self.circle(buf, cx, cy, BTN, self.color("GROUP_HOVER"), 1.0);
        }
        let tone = if hovered && kind == "delete_ask" {
            "ERR"
        } else {
            tone
        };
        self.icon(buf, icon, cx, cy, 18.0, self.color(tone), 1.0);
        self.hit(
            Rect::new(
                cx - BTN / 2.0,
                cy - BTN / 2.0,
                cx + BTN / 2.0,
                cy + BTN / 2.0,
            ),
            kind,
            Some(ZoneData::Session(session)),
        );
    }

    // --------------------------------------------------------------- header
    /// `_draw_header`: back or orb, title, status and the four buttons.
    pub(crate) fn draw_header(&mut self, buf: &mut Layer) {
        let cy = HEADER_H / 2.0;
        let viewing = self.model.viewing().is_some();
        let back = self.mode == Mode::History || viewing;
        self.orb = None;
        let x = if back {
            self.icon_button(
                buf,
                "chevron-left",
                PAD + BTN / 2.0 - 6.0,
                cy,
                "back",
                false,
                false,
                true,
            );
            PAD + BTN + 2.0
        } else {
            // The live orb is painted on top by the panel; here it only keeps
            // its room.
            self.orb = Some((PAD + ORB_D / 2.0, cy, ORB_D));
            PAD + ORB_D + 10.0
        };
        self.draw_title(buf, x, cy, back);
        self.draw_header_buttons(buf, cy);
        canvas::fill_span(
            buf,
            (HEADER_H - 1.0, HEADER_H),
            (PAD, self.g.width - PAD),
            self.color("LINE"),
            self.g.scale,
        );
    }

    /// Title (or title plus pose) fitted to the room left by the buttons.
    fn draw_title(&mut self, buf: &mut Layer, x: f64, cy: f64, back: bool) {
        let title = if self.mode == Mode::History {
            "Historial".to_string()
        } else {
            self.conversation_title()
        };
        let available = (self.g.width - PAD - 4.0 * (BTN + 2.0) - x).max(0.0);
        let title = canvas::fit_text(&title, FS_TITLE, available, self.g.scale);
        if !self.status.is_empty() && !back {
            self.text(
                buf,
                &title,
                FS_TITLE,
                self.color("TEXT"),
                x,
                cy - 15.0,
                Align::Left,
            );
            let status = canvas::fit_text(&self.status, FS_SECONDARY, available, self.g.scale);
            self.text(
                buf,
                &status,
                FS_SECONDARY,
                self.color("MINT"),
                x,
                cy + 4.0,
                Align::Left,
            );
        } else {
            self.text(
                buf,
                &title,
                FS_TITLE,
                self.color("TEXT"),
                x,
                cy - 9.0,
                Align::Left,
            );
        }
    }

    /// Close, settings, history and new-conversation buttons, right to left.
    fn draw_header_buttons(&mut self, buf: &mut Layer, cy: f64) {
        let mut right = self.g.width - PAD - BTN / 2.0 + 6.0;
        // DECISION: direct settings and history controls avoid a menu overlay
        // on the messages.
        for (name, key, active) in [
            ("chevron-down", "close", false),
            ("sliders", "settings", false),
            ("clock", "history", self.mode == Mode::History),
        ] {
            self.icon_button(buf, name, right, cy, key, active, false, true);
            right -= BTN + 2.0;
        }
        // New conversation with one click (and Ctrl+N); off while the current
        // one is still empty.
        let enabled = self.can_start_new();
        self.icon_button(buf, "compose", right, cy, "new", false, false, enabled);
    }

    /// `_icon_button`: round header button with hover, active and alert tones.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn icon_button(
        &mut self,
        buf: &mut Layer,
        name: &str,
        cx: f64,
        cy: f64,
        key: &'static str,
        active: bool,
        alert: bool,
        enabled: bool,
    ) {
        let rect = Rect::new(
            cx - BTN / 2.0,
            cy - BTN / 2.0,
            cx + BTN / 2.0,
            cy + BTN / 2.0,
        );
        if !enabled {
            self.icon(buf, name, cx, cy, 18.0, self.color("TEXT_3"), 0.45);
            return;
        }
        let hovered = self.hover.as_deref() == Some(key);
        if hovered || active {
            let tone = if active { "MINT" } else { "GROUP_HOVER" };
            self.circle(
                buf,
                cx,
                cy,
                BTN,
                self.color(tone),
                if active { 0.18 } else { 1.0 },
            );
        }
        let tone = if alert {
            "ERR"
        } else if active {
            "MINT"
        } else if hovered {
            "TEXT"
        } else {
            "TEXT_2"
        };
        self.icon(buf, name, cx, cy, 18.0, self.color(tone), 1.0);
        self.hit(rect, key, None);
    }
}

#[cfg(test)]
mod tests;
