//! Pointer and keyboard interaction of the chat view (`chat_view.py`, F5-14).

use ez_core::metrics::monotonic;

use super::{Action, ActionData, ChatView, Mode, Move, ZoneData, KEYED};

impl ChatView {
    fn at(&self, x: f64, y: f64) -> Option<(String, Option<ZoneData>)> {
        for zone in self.hits.iter().rev() {
            if zone.rect.x0 <= x && x <= zone.rect.x1 && zone.rect.y0 <= y && y <= zone.rect.y1 {
                let mut key = zone.kind.to_string();
                if KEYED.contains(&zone.kind) {
                    if let Some(ZoneData::Session(session)) = &zone.data {
                        key.push(':');
                        key.push_str(&session.to_string());
                    }
                }
                return Some((key, zone.data.clone()));
            }
        }
        None
    }

    /// Mouse moved: updates the hover and says whether the panel changed.
    pub fn pointer_move(&mut self, x: f64, y: f64) -> bool {
        let new = self.at(x, y).map(|(key, _)| key);
        if new != self.hover {
            self.hover = new;
            return true;
        }
        false
    }

    /// Whether the pointer over `(x, y)` should show the hand cursor.
    pub fn cursor_is_hand(&self, x: f64, y: f64) -> bool {
        self.at(x, y).is_some_and(|(key, _)| key != "input")
    }

    /// Mouse press: an action for the owner, if the zone has one.
    pub fn pointer_down(&mut self, x: f64, y: f64) -> Option<Action> {
        let (key, data) = self.at(x, y)?;
        if key == "input" {
            self.place_caret(x, y);
            return None;
        }
        let kind = key.split(':').next().unwrap_or("");
        self.activate(kind, data)
    }

    fn activate(&mut self, kind: &str, data: Option<ZoneData>) -> Option<Action> {
        match kind {
            "send" => self.send(),
            "new" => {
                self.start_new();
                Some(Action::new("new"))
            }
            "close" => Some(Action::new("close")),
            "stop" => Some(Action::new("stop")),
            "voice" => Some(Action::with(
                "voice",
                ActionData::Flag(!self.model.voice_replies()),
            )),
            "mic" => Some(Action::with(
                "mic",
                ActionData::Flag(self.in_call && !self.model.mic_muted()),
            )),
            "settings" => Some(Action::new("settings")),
            "history" | "back" | "session" | "delete_ask" | "delete_no" | "delete_yes" => {
                self.activate_history(kind, data)
            }
            _ => self.activate_payload(kind, data),
        }
    }

    /// «Nueva conversación»: back to chat, glued to the end.
    fn start_new(&mut self) {
        self.mode = Mode::Chat;
        self.scroll.reset();
        self.pending_delete = None;
    }

    /// History and saved-conversation controls.
    fn activate_history(&mut self, kind: &str, data: Option<ZoneData>) -> Option<Action> {
        match kind {
            "history" => {
                self.mode = match self.mode {
                    Mode::Chat => Mode::History,
                    Mode::History => Mode::Chat,
                };
                self.scroll.reset();
                self.pending_delete = None;
                None
            }
            "back" => {
                self.model.view_session(None);
                self.mode = Mode::Chat;
                self.scroll.reset();
                None
            }
            "session" => {
                self.start_new();
                let session = session_of(data)?;
                if self.model.current_session() != Some(session) {
                    return Some(Action::with("switch", ActionData::Session(session)));
                }
                None
            }
            "delete_ask" => {
                self.pending_delete = session_of(data);
                None
            }
            "delete_no" => {
                self.pending_delete = None;
                None
            }
            "delete_yes" => {
                self.pending_delete = None;
                let session = session_of(data)?;
                let rows = self.model.sessions();
                if let Some(index) = rows.iter().position(|row| row.session == session) {
                    self.deleting = Some((index, rows[index].clone(), monotonic()));
                }
                Some(Action::with("delete", ActionData::Session(session)))
            }
            _ => None,
        }
    }

    /// Copy, open and confirmation actions.
    fn activate_payload(&mut self, kind: &str, data: Option<ZoneData>) -> Option<Action> {
        match kind {
            "copy_msg" | "copy_code" => data.map(|data| Action::with("copy", data.into_action())),
            "open" | "reveal" => {
                let action = if kind == "open" { "open" } else { "reveal" };
                data.map(|data| Action::with(action, data.into_action()))
            }
            "image" => data.map(|data| Action::with("open", data.into_action())),
            "confirm_yes" | "confirm_no" => Some(Action::with(
                "confirm",
                ActionData::Flag(kind == "confirm_yes"),
            )),
            _ => None,
        }
    }

    /// Mouse wheel: `delta > 0` goes up, towards older messages.
    pub fn wheel(&mut self, delta: i32) {
        self.scroll.wheel(delta);
    }

    fn send(&mut self) -> Option<Action> {
        if self.model.busy() && self.input.empty() {
            return Some(Action::new("stop"));
        }
        let text = self.input.take();
        if text.is_empty() {
            return None;
        }
        self.mode = Mode::Chat;
        self.scroll.reset();
        Some(Action::with("send", ActionData::Text(text)))
    }

    /// One key (Windows VK codes). Returns an action for the owner, if any.
    pub fn key(&mut self, vk: i32, ctrl: bool, shift: bool) -> Option<Action> {
        self.focus_t0 = monotonic();
        match vk {
            13 => {
                if shift {
                    self.input.newline();
                    None
                } else {
                    self.send()
                }
            }
            27 => self.escape(),
            65 | 67 | 88 if ctrl => self.clipboard_key(vk),
            78 if ctrl => {
                if self.can_start_new() {
                    self.activate("new", None)
                } else {
                    None
                }
            }
            35..=40 => {
                self.input.move_caret(move_of(vk), shift, ctrl);
                None
            }
            8 => {
                self.input.backspace(ctrl);
                None
            }
            46 => {
                self.input.delete(ctrl);
                None
            }
            33 => {
                self.wheel(120 * 6);
                None
            }
            34 => {
                self.wheel(-120 * 6);
                None
            }
            _ => None,
        }
    }

    fn clipboard_key(&mut self, vk: i32) -> Option<Action> {
        if vk == 65 {
            self.input.select_all();
            return None;
        }
        let selected = self.input.selected_text();
        if selected.is_empty() {
            return None;
        }
        if vk == 88 {
            self.input.backspace(false);
        }
        Some(Action::with("copy", ActionData::Text(selected)))
    }

    fn escape(&mut self) -> Option<Action> {
        if !self.input.empty() {
            self.input.clear();
            return None;
        }
        if self.pending_delete.is_some() {
            self.pending_delete = None;
            return None;
        }
        if self.mode == Mode::History {
            self.mode = Mode::Chat;
            return None;
        }
        if self.model.viewing().is_some() {
            self.model.view_session(None);
            return None;
        }
        Some(Action::new("close"))
    }

    /// A typed character.
    pub fn char(&mut self, ch: &str) {
        let Some(first) = ch.chars().next() else {
            return;
        };
        let code = first as u32;
        if code >= 32 && code != 127 {
            self.input.insert(ch);
            self.focus_t0 = monotonic();
        }
    }

    /// Pasted text.
    pub fn paste(&mut self, text: &str) {
        self.input.insert(text);
        self.focus_t0 = monotonic();
    }

    /// Files dropped on the window: they enter as links the chat shows as cards.
    pub fn add_files(&mut self, paths: &[String]) {
        let lines: Vec<String> = paths
            .iter()
            .filter(|path| !path.is_empty())
            .map(|path| format!("[{}](<{}>)", file_name(path), path))
            .collect();
        if lines.is_empty() {
            return;
        }
        let text = self.input.text();
        let prefix = if !text.is_empty() && !text.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        self.input
            .insert(&(prefix.to_string() + &lines.join("\n") + "\n"));
    }
}

fn session_of(data: Option<ZoneData>) -> Option<i64> {
    // DECISION: Python does `int(data)` and raises on a missing payload; a
    // zone without its session is ignored here instead of panicking.
    match data {
        Some(ZoneData::Session(session)) => Some(session),
        _ => None,
    }
}

fn move_of(vk: i32) -> Move {
    match vk {
        37 => Move::Left,
        39 => Move::Right,
        38 => Move::Up,
        36 => Move::Home,
        35 => Move::End,
        _ => Move::Down,
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(
        || path.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
