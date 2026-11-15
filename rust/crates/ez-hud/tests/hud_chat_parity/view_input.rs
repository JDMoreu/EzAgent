//! `text_input.TextInput` del generador, índice por carácter.

use ez_hud::model::chat::view::{Move, TextField};

/// Campo de texto del generador (`text_input.TextInput`), índice por carácter.
#[derive(Default)]
pub(super) struct FakeInput {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) anchor: Option<usize>,
}

impl FakeInput {
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
        self.delete_selection();
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

    fn move_caret(&mut self, key: Move, select: bool, word: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else if let Some((start, end)) = self.selection_bounds() {
            if key == Move::Left || key == Move::Right {
                self.cursor = if key == Move::Left { start } else { end };
                self.anchor = None;
                return;
            }
        } else {
            self.anchor = None;
        }
        self.cursor = self.target(key, word);
    }

    fn backspace(&mut self, word: bool) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let start = if word {
            self.word_left(self.cursor)
        } else {
            self.cursor - 1
        };
        let from = self.byte_at(start);
        let to = self.byte_at(self.cursor);
        self.text.replace_range(from..to, "");
        self.cursor = start;
    }

    fn delete(&mut self, word: bool) {
        if self.delete_selection() || self.cursor >= self.text.chars().count() {
            return;
        }
        let end = if word {
            self.word_right(self.cursor)
        } else {
            self.cursor + 1
        };
        let from = self.byte_at(self.cursor);
        let to = self.byte_at(end);
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

impl FakeInput {
    fn target(&self, key: Move, word: bool) -> usize {
        let text = &self.text;
        let pos = self.cursor;
        let chars: Vec<char> = text.chars().collect();
        match key {
            Move::Left => {
                if word {
                    self.word_left(pos)
                } else {
                    pos.saturating_sub(1)
                }
            }
            Move::Right => {
                if word {
                    self.word_right(pos)
                } else {
                    (pos + 1).min(chars.len())
                }
            }
            Move::Home => {
                let mut start = pos;
                while start > 0 && chars[start - 1] != '\n' {
                    start -= 1;
                }
                start
            }
            Move::End => {
                let mut end = pos;
                while end < chars.len() && chars[end] != '\n' {
                    end += 1;
                }
                end
            }
            Move::Up => {
                let start = line_start(&chars, pos);
                if start == 0 {
                    return 0;
                }
                let previous = line_start(&chars, start - 1);
                (previous + (pos - start)).min(start - 1)
            }
            Move::Down => {
                let start = line_start(&chars, pos);
                let mut next = pos;
                while next < chars.len() && chars[next] != '\n' {
                    next += 1;
                }
                if next >= chars.len() {
                    return chars.len();
                }
                let mut end = next + 1;
                while end < chars.len() && chars[end] != '\n' {
                    end += 1;
                }
                (next + 1 + (pos - start)).min(end)
            }
        }
    }

    fn word_left(&self, position: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut pos = position;
        while pos > 0 && !is_word(chars[pos - 1]) {
            pos -= 1;
        }
        while pos > 0 && is_word(chars[pos - 1]) {
            pos -= 1;
        }
        pos
    }

    fn word_right(&self, position: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut pos = position;
        while pos < chars.len() && !is_word(chars[pos]) {
            pos += 1;
        }
        while pos < chars.len() && is_word(chars[pos]) {
            pos += 1;
        }
        pos
    }
}

fn line_start(chars: &[char], position: usize) -> usize {
    let mut start = position;
    while start > 0 && chars[start - 1] != '\n' {
        start -= 1;
    }
    start
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
