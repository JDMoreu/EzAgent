//! Fakes and helpers shared by the tests of `model::chat`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ez_gfx::Image;

use crate::paint::chat::{Block, ChatHost, FileInfo, Paragraph, Span};

use super::{
    ChatMessage, ChatModel, ChatView, ConfirmCard, Role, SessionInfo, TextField, ViewHost,
};

/// Chat model with plain fields, enough for the view tests.
#[derive(Default)]
pub(crate) struct FakeModel {
    pub messages: Vec<ChatMessage>,
    pub sessions: Vec<SessionInfo>,
    pub busy: bool,
    pub confirm: Option<ConfirmCard>,
    pub viewing: Option<i64>,
    pub current: Option<i64>,
    pub voice: bool,
    pub muted: bool,
}

impl ChatModel for FakeModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        (0, self.messages.clone())
    }

    fn version(&self) -> i64 {
        0
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        self.sessions.clone()
    }

    fn busy(&self) -> bool {
        self.busy
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        self.confirm.clone()
    }

    fn viewing(&self) -> Option<i64> {
        self.viewing
    }

    fn current_session(&self) -> Option<i64> {
        self.current
    }

    fn voice_replies(&self) -> bool {
        self.voice
    }

    fn mic_muted(&self) -> bool {
        self.muted
    }

    fn view_session(&mut self, session: Option<i64>) {
        self.viewing = if session == self.current {
            None
        } else {
            session
        };
    }
}

/// Chat model shared with the test so it can be changed after the view owns it.
#[derive(Clone, Default)]
pub(crate) struct SharedModel(pub Rc<RefCell<FakeModel>>);

impl ChatModel for SharedModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        self.0.borrow().snapshot()
    }

    fn version(&self) -> i64 {
        self.0.borrow().version()
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        self.0.borrow().sessions()
    }

    fn busy(&self) -> bool {
        self.0.borrow().busy
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        self.0.borrow().confirm()
    }

    fn viewing(&self) -> Option<i64> {
        self.0.borrow().viewing()
    }

    fn current_session(&self) -> Option<i64> {
        self.0.borrow().current
    }

    fn voice_replies(&self) -> bool {
        self.0.borrow().voice
    }

    fn mic_muted(&self) -> bool {
        self.0.borrow().muted
    }

    fn view_session(&mut self, session: Option<i64>) {
        self.0.borrow_mut().view_session(session);
    }
}

/// View plus a handle on its model.
pub(crate) fn shared_view() -> (ChatView, Rc<RefCell<FakeModel>>) {
    let model = Rc::new(RefCell::new(FakeModel {
        current: Some(1),
        ..FakeModel::default()
    }));
    let mut view = ChatView::new(
        Box::new(SharedModel(Rc::clone(&model))),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    (view, model)
}

/// Minimal text field: codepoint indices, no soft wrapping.
#[derive(Default)]
pub(crate) struct FakeInput {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}

impl FakeInput {
    pub(crate) fn with_text(text: &str) -> Self {
        let mut input = Self::default();
        input.insert(text);
        input
    }

    fn byte_at(&self, position: usize) -> usize {
        self.text
            .char_indices()
            .nth(position)
            .map_or(self.text.len(), |(index, _)| index)
    }

    fn selection_bounds(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            None
        } else {
            Some((anchor.min(self.cursor), anchor.max(self.cursor)))
        }
    }

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_bounds() else {
            self.anchor = None;
            return false;
        };
        let from = self.byte_at(start);
        let to = self.byte_at(end);
        self.text.replace_range(from..to, "");
        self.cursor = start;
        self.anchor = None;
        true
    }
}

impl TextField for FakeInput {
    fn text(&self) -> &str {
        &self.text
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn insert(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        self.delete_selection();
        let clean: String = value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ")
            .chars()
            .filter(|ch| !ch.is_control() || *ch == '\n')
            .collect();
        if clean.is_empty() {
            return;
        }
        let at = self.byte_at(self.cursor);
        self.text.insert_str(at, &clean);
        self.cursor += clean.chars().count();
    }

    fn newline(&mut self) {
        self.insert("\n");
    }

    fn take(&mut self) -> String {
        let value = self.text.trim().to_string();
        self.clear();
        value
    }

    fn move_caret(&mut self, key: super::Move, select: bool, _word: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else if let Some((start, end)) = self.selection_bounds() {
            if key == super::Move::Left || key == super::Move::Right {
                self.cursor = if key == super::Move::Left { start } else { end };
                self.anchor = None;
                return;
            }
        } else {
            self.anchor = None;
        }
        match key {
            super::Move::Left => self.cursor = self.cursor.saturating_sub(1),
            super::Move::Right => {
                self.cursor = (self.cursor + 1).min(self.text.chars().count());
            }
            super::Move::Home => self.cursor = 0,
            super::Move::End => self.cursor = self.text.chars().count(),
            super::Move::Up | super::Move::Down => {}
        }
    }

    fn backspace(&mut self, _word: bool) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let from = self.byte_at(self.cursor - 1);
        let to = self.byte_at(self.cursor);
        self.text.replace_range(from..to, "");
        self.cursor -= 1;
    }

    fn delete(&mut self, _word: bool) {
        if self.delete_selection() || self.cursor >= self.text.chars().count() {
            return;
        }
        let from = self.byte_at(self.cursor);
        let to = self.byte_at(self.cursor + 1);
        self.text.replace_range(from..to, "");
    }

    fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.chars().count();
    }

    fn selected_text(&self) -> String {
        match self.selection_bounds() {
            Some((start, end)) => self.text.chars().skip(start).take(end - start).collect(),
            None => String::new(),
        }
    }

    fn selection(&self) -> Option<(usize, usize)> {
        self.selection_bounds()
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
    }

    fn place_caret(&mut self, cursor: usize) {
        self.anchor = None;
        self.cursor = cursor.min(self.text.chars().count());
    }
}

/// Markdown host: every text is a paragraph with a single plain span.
pub(crate) struct FakeHost;

impl ChatHost for FakeHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        vec![Block::Paragraph(Paragraph {
            spans: vec![Span::plain(text)],
        })]
    }

    fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

impl ViewHost for FakeHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        vec![Span::plain(text)]
    }
}

/// View with one user message, a dark palette and a 440×600 window.
pub(crate) fn sample_view() -> ChatView {
    let mut model = FakeModel {
        current: Some(1),
        ..FakeModel::default()
    };
    model.messages.push(ChatMessage::new(1, Role::User, "hola"));
    let mut view = ChatView::new(
        Box::new(model),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    view
}

/// View with the given messages and conversations.
pub(crate) fn view_with(messages: Vec<ChatMessage>, sessions: Vec<SessionInfo>) -> ChatView {
    let model = FakeModel {
        messages,
        sessions,
        current: Some(1),
        ..FakeModel::default()
    };
    let mut view = ChatView::new(
        Box::new(model),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    view
}
