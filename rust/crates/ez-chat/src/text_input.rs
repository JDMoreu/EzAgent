//! Port of `src/ezagent/voice/chat/text_input.py` (F3-14).
//!
//! Campo de texto con cursor, selección y pegado. Índices por punto de código,
//! igual que Python.

use ez_core::pyvalue::{char_len, head_chars, py_strip};

/// `MAX_CHARS`.
pub const MAX_CHARS: usize = 20_000;

/// Tecla de movimiento del cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// `str(text or "")` normalizado: `\r\n`/`\r` a `\n`, tabuladores a cuatro
/// espacios y sin caracteres de control.
pub fn sanitize(text: &str) -> String {
    let clean = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ");
    clean
        .chars()
        .filter(|character| {
            !matches!(
                *character,
                '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'
            )
        })
        .collect()
}

/// Byte donde empieza el carácter `index` (o el final si se pasa).
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(offset, _)| offset)
}

/// `text[start:end]` contando puntos de código.
fn slice(text: &str, start: usize, end: usize) -> &str {
    &text[byte_at(text, start)..byte_at(text, end)]
}

/// `re.match(r"\w", ch)` de Python (Unicode, incluye `_`).
fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Texto editable con cursor y selección entre `anchor` y `cursor`.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}

impl TextInput {
    /// `TextInput(text)`.
    pub fn new(text: &str) -> Self {
        let mut input = Self::default();
        if !text.is_empty() {
            input.insert(text);
        }
        input
    }

    // ------------------------------------------------------------ selección
    /// `selection()`.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some((anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    /// `selected_text()`.
    pub fn selected_text(&self) -> String {
        match self.selection() {
            Some((start, end)) => slice(&self.text, start, end).to_string(),
            None => String::new(),
        }
    }

    /// `select_all()`.
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = char_len(&self.text);
    }

    /// `_delete_selection()`.
    pub fn delete_selection(&mut self) -> bool {
        let span = self.selection();
        self.anchor = None;
        let Some((start, end)) = span else {
            return false;
        };
        self.text = format!(
            "{}{}",
            slice(&self.text, 0, start),
            slice(&self.text, end, char_len(&self.text))
        );
        self.cursor = start;
        true
    }

    // -------------------------------------------------------------- edición
    /// `insert(text)`: escribe o pega en el cursor (sustituye la selección).
    pub fn insert(&mut self, text: &str) {
        let clean = sanitize(text);
        if clean.is_empty() {
            return;
        }
        self.delete_selection();
        let room = MAX_CHARS.saturating_sub(char_len(&self.text));
        if room == 0 {
            return;
        }
        let clean = head_chars(&clean, room);
        let total = char_len(&self.text);
        self.text = format!(
            "{}{}{}",
            slice(&self.text, 0, self.cursor),
            clean,
            slice(&self.text, self.cursor, total)
        );
        self.cursor += char_len(clean);
    }

    /// `newline()`.
    pub fn newline(&mut self) {
        self.insert("\n");
    }

    /// `backspace(word)`.
    pub fn backspace(&mut self, word: bool) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let start = if word {
            self.word_left(self.cursor)
        } else {
            self.cursor - 1
        };
        let end = self.cursor;
        self.text = format!(
            "{}{}",
            slice(&self.text, 0, start),
            slice(&self.text, end, char_len(&self.text))
        );
        self.cursor = start;
    }

    /// `delete(word)`.
    pub fn delete(&mut self, word: bool) {
        let total = char_len(&self.text);
        if self.delete_selection() || self.cursor >= total {
            return;
        }
        let end = if word {
            self.word_right(self.cursor)
        } else {
            self.cursor + 1
        };
        self.text = format!(
            "{}{}",
            slice(&self.text, 0, self.cursor),
            slice(&self.text, end, total)
        );
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
    }

    /// `take()`: el texto para enviar, sin espacios sobrantes; vacía el campo.
    pub fn take(&mut self) -> String {
        let value = py_strip(&self.text).to_string();
        self.clear();
        value
    }

    /// `set_text(text)`.
    pub fn set_text(&mut self, text: &str) {
        self.clear();
        self.insert(text);
    }

    // ----------------------------------------------------------- movimiento
    /// `move(key, select, word)`.
    // DECISION: `move` es palabra reservada en Rust; se llama `move_cursor`.
    pub fn move_cursor(&mut self, key: Move, select: bool, word: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else if let Some((start, end)) = self.selection() {
            if matches!(key, Move::Left | Move::Right) {
                self.cursor = if key == Move::Left { start } else { end };
                self.anchor = None;
                return;
            }
            self.anchor = None;
        } else {
            self.anchor = None;
        }
        self.cursor = self.target(key, word);
    }

    fn target(&self, key: Move, word: bool) -> usize {
        let total = char_len(&self.text);
        let pos = self.cursor;
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
                    (pos + 1).min(total)
                }
            }
            Move::Home => self.line_start(pos),
            Move::End => self.line_end(pos),
            Move::Up => self.line_up(pos),
            Move::Down => self.line_down(pos),
        }
    }

    /// `text.rfind("\n", 0, pos) + 1`.
    fn line_start(&self, pos: usize) -> usize {
        let before = slice(&self.text, 0, pos);
        before
            .rfind('\n')
            .map_or(0, |index| char_len(&before[..index]) + 1)
    }

    /// `text.find("\n", pos)`, o el final.
    fn line_end(&self, pos: usize) -> usize {
        let after = slice(&self.text, pos, char_len(&self.text));
        after.find('\n').map_or_else(
            || char_len(&self.text),
            |index| pos + char_len(&after[..index]),
        )
    }

    fn line_up(&self, pos: usize) -> usize {
        let start = self.line_start(pos);
        if start == 0 {
            return 0;
        }
        let col = pos - start;
        let previous_start = self.line_start(start - 1);
        (previous_start + col).min(start - 1)
    }

    fn line_down(&self, pos: usize) -> usize {
        let start = self.line_start(pos);
        let col = pos - start;
        let total = char_len(&self.text);
        let after = slice(&self.text, pos, total);
        let Some(first_break) = after.find('\n') else {
            return total;
        };
        let next_start = pos + char_len(&after[..first_break]) + 1;
        let rest = slice(&self.text, next_start, total);
        let end = rest
            .find('\n')
            .map_or(total, |index| next_start + char_len(&rest[..index]));
        (next_start + col).min(end)
    }

    fn word_left(&self, mut pos: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        while pos > 0 && !is_word(chars[pos - 1]) {
            pos -= 1;
        }
        while pos > 0 && is_word(chars[pos - 1]) {
            pos -= 1;
        }
        pos
    }

    fn word_right(&self, mut pos: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let total = chars.len();
        while pos < total && !is_word(chars[pos]) {
            pos += 1;
        }
        while pos < total && is_word(chars[pos]) {
            pos += 1;
        }
        pos
    }

    // ------------------------------------------------------------- consulta
    /// `line_col()`: `(línea, columna)` del cursor.
    pub fn line_col(&self) -> (usize, usize) {
        let before = slice(&self.text, 0, self.cursor);
        let line = before.matches('\n').count();
        let column = match before.rfind('\n') {
            Some(index) => char_len(&before[index..]) - 1,
            None => char_len(before),
        };
        (line, column)
    }

    /// `empty`: el texto no tiene nada aparte de espacios.
    pub fn empty(&self) -> bool {
        py_strip(&self.text).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_cursor() {
        let mut field = TextInput::default();
        field.insert("hola");
        field.move_cursor(Move::Left, false, false);
        field.insert("X");
        assert_eq!(field.text, "holXa");
        assert_eq!(field.cursor, 4);
    }

    #[test]
    fn word_movement_and_deletion() {
        let mut field = TextInput::new("resume el informe, por favor");
        field.move_cursor(Move::Left, false, true);
        assert_eq!(
            slice(&field.text, field.cursor, char_len(&field.text)),
            "favor"
        );
        field.move_cursor(Move::Left, false, true);
        assert_eq!(
            slice(&field.text, field.cursor, char_len(&field.text)),
            "por favor"
        );
        field.move_cursor(Move::Right, false, true);
        assert!(slice(&field.text, 0, field.cursor).ends_with("por"));
        field.backspace(true);
        assert_eq!(field.text, "resume el informe,  favor");
        let mut field = TextInput::new("uno dos tres");
        field.move_cursor(Move::Home, false, false);
        field.delete(true);
        assert_eq!(field.text, " dos tres");
    }

    #[test]
    fn selection_is_replaced_and_deleted() {
        let mut field = TextInput::new("hola mundo");
        field.select_all();
        assert_eq!(field.selected_text(), "hola mundo");
        field.insert("adiós");
        assert_eq!(field.text, "adiós");
        assert!(field.selection().is_none());
        field.select_all();
        field.backspace(false);
        assert!(field.text.is_empty());
        assert_eq!(field.cursor, 0);
    }

    #[test]
    fn backspace_and_delete_at_the_edges() {
        let mut field = TextInput::new("abcd");
        field.move_cursor(Move::Left, false, false);
        field.backspace(false);
        assert_eq!(field.text, "abd");
        assert_eq!(field.cursor, 2);
        field.delete(false);
        assert_eq!(field.text, "ab");
        // Al final no hace nada; al principio tampoco.
        field.delete(false);
        assert_eq!(field.text, "ab");
        let mut empty = TextInput::default();
        empty.backspace(false);
        assert!(empty.text.is_empty() && empty.cursor == 0);
    }

    #[test]
    fn shift_movement_extends_the_selection() {
        let mut field = TextInput::new("abcdef");
        field.move_cursor(Move::Home, false, false);
        field.move_cursor(Move::Right, true, false);
        field.move_cursor(Move::Right, true, false);
        assert_eq!(field.selected_text(), "ab");
        field.move_cursor(Move::Left, true, false);
        assert_eq!(field.selected_text(), "a");
        // Sin Mayús, el cursor se colapsa al borde y la selección desaparece.
        field.move_cursor(Move::Right, false, false);
        assert!(field.selection().is_none());
        assert_eq!(field.cursor, 1);
    }

    #[test]
    fn enter_and_set_text() {
        let mut field = TextInput::new("a");
        field.newline();
        field.insert("b");
        assert_eq!(field.text, "a\nb");
        let mut field = TextInput::new("viejo");
        field.set_text("nuevo");
        assert_eq!(field.text, "nuevo");
        assert_eq!(field.cursor, 5);
    }

    #[test]
    fn plain_arrow_collapses_selection_to_its_edge() {
        let mut field = TextInput::new("abcdef");
        field.move_cursor(Move::Home, false, false);
        for _ in 0..3 {
            field.move_cursor(Move::Right, true, false);
        }
        field.move_cursor(Move::Left, false, false);
        assert_eq!(field.cursor, 0);
        assert!(field.selection().is_none());
    }

    #[test]
    fn multiline_navigation() {
        let mut field = TextInput::new("primera\nsegunda línea\ntercera");
        assert_eq!(field.line_col(), (2, 7));
        field.move_cursor(Move::Up, false, false);
        assert_eq!(field.line_col(), (1, 7));
        field.move_cursor(Move::Home, false, false);
        field.move_cursor(Move::End, false, false);
        assert_eq!(field.line_col(), (1, 13));
        field.move_cursor(Move::Up, false, false);
        assert_eq!(field.line_col(), (0, 7));
        field.move_cursor(Move::Up, false, false);
        assert_eq!(field.cursor, 0);
    }

    #[test]
    fn sanitize_removes_controls_and_expands_tabs() {
        assert_eq!(
            sanitize("a\r\nb\rc\td\u{0}e\u{1b}[31m"),
            "a\nb\nc    de[31m"
        );
    }

    #[test]
    fn take_trims_and_empties() {
        let mut field = TextInput::new("  hola  \n");
        assert_eq!(field.take(), "hola");
        assert!(field.text.is_empty() && field.empty());
        assert!(TextInput::new("   \n  ").empty());
    }

    #[test]
    fn the_text_has_a_cap() {
        let mut field = TextInput::default();
        field.insert(&"x".repeat(MAX_CHARS + 5000));
        assert_eq!(char_len(&field.text), MAX_CHARS);
        field.move_cursor(Move::Home, false, false);
        field.delete(false);
        field.insert("yz");
        assert_eq!(char_len(&field.text), MAX_CHARS);
    }

    #[test]
    fn unicode_is_counted_per_code_point() {
        let mut field = TextInput::new("añadir café ☕ y ñandú");
        field.backspace(false);
        assert_eq!(field.text, "añadir café ☕ y ñand");
        assert_eq!(field.cursor, char_len(&field.text));
    }
}
