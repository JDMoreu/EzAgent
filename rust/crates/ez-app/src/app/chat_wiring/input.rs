//! Adapter from `ez_chat::text_input::TextInput` to the view field (F6-08).

use ez_chat::text_input::{Move as InputMove, TextInput};
use ez_core::pyvalue::char_len;
use ez_hud::model::chat::view::{Move, TextField};

/// `ChatView.input` (`text_input.TextInput`) behind the `ez-hud` seam.
pub(super) struct HudInput {
    input: TextInput,
}

impl HudInput {
    pub(super) fn new() -> Self {
        Self {
            input: TextInput::new(""),
        }
    }
}

fn input_move(key: Move) -> InputMove {
    match key {
        Move::Left => InputMove::Left,
        Move::Right => InputMove::Right,
        Move::Up => InputMove::Up,
        Move::Down => InputMove::Down,
        Move::Home => InputMove::Home,
        Move::End => InputMove::End,
    }
}

impl TextField for HudInput {
    fn text(&self) -> &str {
        &self.input.text
    }

    fn cursor(&self) -> usize {
        self.input.cursor
    }

    fn empty(&self) -> bool {
        self.input.empty()
    }

    fn insert(&mut self, text: &str) {
        self.input.insert(text);
    }

    fn newline(&mut self) {
        self.input.newline();
    }

    fn take(&mut self) -> String {
        self.input.take()
    }

    fn move_caret(&mut self, key: Move, select: bool, word: bool) {
        self.input.move_cursor(input_move(key), select, word);
    }

    fn backspace(&mut self, word: bool) {
        self.input.backspace(word);
    }

    fn delete(&mut self, word: bool) {
        self.input.delete(word);
    }

    fn select_all(&mut self) {
        self.input.select_all();
    }

    fn selected_text(&self) -> String {
        self.input.selected_text()
    }

    fn selection(&self) -> Option<(usize, usize)> {
        self.input.selection()
    }

    fn clear(&mut self) {
        self.input.clear();
    }

    fn place_caret(&mut self, cursor: usize) {
        self.input.anchor = None;
        self.input.cursor = cursor.min(char_len(&self.input.text));
    }
}
