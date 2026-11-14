//! Análisis de bloques de markdown: líneas, listas, tablas, citas y código.

use super::inline::{rest_matches, skip_space};
use super::{
    clean_target, parse_inline, Align, Block, Code, Document, Heading, Image, ListItem, Quote,
    Table,
};
use ez_core::pyvalue::py_strip;

/// Una valla de código abierta.
pub(super) struct Fence {
    marker: char,
    length: usize,
    lang: String,
}

pub(super) fn single_line(line: &str) -> Option<Block> {
    if let Some((level, text)) = match_heading(line) {
        return Some(Block::Heading(Heading {
            level,
            spans: parse_inline(&text),
        }));
    }
    if match_rule(line) {
        return Some(Block::Rule(super::Rule));
    }
    if let Some((alt, target)) = match_link_line(line, true) {
        return Some(Block::Image(Image {
            alt: py_strip(&alt).to_string(),
            src: clean_target(&target),
        }));
    }
    if let Some((label, target)) = match_link_line(line, false) {
        let target = clean_target(&target);
        if is_local_target(&target) {
            return Some(Block::Document(Document {
                label: py_strip(&label).to_string(),
                target,
            }));
        }
    }
    None
}

fn chars_of(line: &str) -> Vec<char> {
    line.chars().collect()
}

fn match_heading(line: &str) -> Option<(usize, String)> {
    let chars = chars_of(line);
    let mut index = 0usize;
    while index < 3 && chars.get(index) == Some(&' ') {
        index += 1;
    }
    let mut level = 0usize;
    while level < 6 && chars.get(index + level) == Some(&'#') {
        level += 1;
    }
    if level == 0
        || !chars
            .get(index + level)
            .is_some_and(|value| value.is_whitespace())
    {
        return None;
    }
    index += level;
    index = skip_space(&chars, index);
    let rest: String = chars[index..].iter().collect();
    let rest = py_strip(&rest);
    let rest = rest.trim_end_matches('#');
    Some((level, py_strip(rest).to_string()))
}

fn match_rule(line: &str) -> bool {
    let chars = chars_of(line);
    let mut index = 0usize;
    while index < 3 && chars.get(index) == Some(&' ') {
        index += 1;
    }
    let Some(character) = chars.get(index).copied() else {
        return false;
    };
    if !matches!(character, '-' | '*' | '_') {
        return false;
    }
    index += 1;
    let mut count = 1usize;
    loop {
        let mut next = index;
        while chars.get(next).is_some_and(|value| value.is_whitespace()) {
            next += 1;
        }
        if chars.get(next) == Some(&character) {
            count += 1;
            index = next + 1;
        } else {
            break;
        }
    }
    while chars.get(index).is_some_and(|value| value.is_whitespace()) {
        index += 1;
    }
    count >= 3 && index == chars.len()
}

pub(super) fn match_fence(line: &str) -> Option<Fence> {
    let chars = chars_of(line);
    let mut index = 0usize;
    while index < 3 && chars.get(index) == Some(&' ') {
        index += 1;
    }
    let marker = chars.get(index).copied()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let mut length = 0usize;
    while chars.get(index + length) == Some(&marker) {
        length += 1;
    }
    if length < 3 {
        return None;
    }
    index += length;
    index = skip_space(&chars, index);
    let start = index;
    while chars.get(index).is_some_and(|value| {
        value.is_alphanumeric() || matches!(value, '_' | '+' | '#' | '.' | '-')
    }) {
        index += 1;
    }
    let lang: String = chars[start..index].iter().collect();
    index = skip_space(&chars, index);
    (index == chars.len()).then_some(Fence {
        marker,
        length,
        lang,
    })
}

pub(super) fn read_code(lines: &[&str], index: usize, fence: &Fence) -> (usize, Block) {
    let mut body: Vec<&str> = Vec::new();
    let mut index = index + 1;
    while index < lines.len() {
        let closing = py_strip(lines[index]);
        let closes = !closing.is_empty()
            && closing.chars().all(|value| value == fence.marker)
            && closing.chars().count() >= fence.length;
        if closes {
            index += 1;
            break;
        }
        body.push(lines[index]);
        index += 1;
    }
    (
        index,
        Block::Code(Code {
            lang: fence.lang.clone(),
            text: body.join("\n"),
        }),
    )
}

fn match_link_line(line: &str, image: bool) -> Option<(String, String)> {
    let chars = chars_of(line);
    let mut index = skip_space(&chars, 0);
    if !image
        && chars
            .get(index)
            .is_some_and(|value| matches!(value, '-' | '*' | '+'))
        && chars
            .get(index + 1)
            .is_some_and(|value| value.is_whitespace())
    {
        index += 2;
        index = skip_space(&chars, index);
    }
    if image {
        if chars.get(index) != Some(&'!') || chars.get(index + 1) != Some(&'[') {
            return None;
        }
        index += 1;
    }
    if chars.get(index) != Some(&'[') {
        return None;
    }
    let label_start = index + 1;
    let label_end = chars[label_start..]
        .iter()
        .position(|value| *value == ']')
        .map(|offset| label_start + offset)?;
    if !image && label_end == label_start {
        return None;
    }
    let label: String = chars[label_start..label_end].iter().collect();
    index = label_end + 1;
    if chars.get(index) != Some(&'(') {
        return None;
    }
    index = skip_space(&chars, index + 1);
    let start = index;
    let (end, raw) = if chars.get(index) == Some(&'<') {
        let mut close = index + 1;
        while close < chars.len() && chars[close] != '>' && chars[close] != '\n' {
            close += 1;
        }
        if close >= chars.len() || chars[close] != '>' || close == index + 1 {
            return None;
        }
        (close + 1, chars[start..=close].iter().collect::<String>())
    } else if let Some(prefix) = local_prefix(&chars, index) {
        let mut stop = index + prefix;
        while stop < chars.len() && chars[stop] != ')' && chars[stop] != '\n' {
            stop += 1;
        }
        let mut end = index + prefix;
        while end <= stop && !rest_matches(&chars, end) {
            end += 1;
        }
        if end > stop {
            return None;
        }
        (end, chars[start..end].iter().collect::<String>())
    } else {
        let mut stop = index;
        while stop < chars.len() && chars[stop] != ')' && !chars[stop].is_whitespace() {
            stop += 1;
        }
        if stop == start || !rest_matches(&chars, stop) {
            return None;
        }
        (stop, chars[start..stop].iter().collect::<String>())
    };
    if !rest_matches(&chars, end) {
        return None;
    }
    Some((label, raw))
}

pub(super) fn is_local_target(target: &str) -> bool {
    let chars = chars_of(target);
    local_prefix(&chars, 0).is_some()
}

fn local_prefix(chars: &[char], index: usize) -> Option<usize> {
    let get = |offset: usize| chars.get(index + offset).copied();
    if get(0).is_some_and(|value| value.is_ascii_alphabetic())
        && get(1) == Some(':')
        && matches!(get(2), Some('/') | Some('\\'))
    {
        return Some(3);
    }
    if get(0) == Some('\\') && get(1) == Some('\\') {
        return Some(2);
    }
    if chars[index..].starts_with(&['f', 'i', 'l', 'e', ':']) {
        return Some(5);
    }
    if get(0) == Some('~') && matches!(get(1), Some('/') | Some('\\')) {
        return Some(2);
    }
    if get(0) == Some('.') {
        if matches!(get(1), Some('/') | Some('\\')) {
            return Some(2);
        }
        if get(1) == Some('.') && matches!(get(2), Some('/') | Some('\\')) {
            return Some(3);
        }
    }
    (get(0) == Some('/')).then_some(1)
}

pub(super) fn match_quote(line: &str) -> Option<String> {
    let chars = chars_of(line);
    let mut index = 0usize;
    while index < 3 && chars.get(index) == Some(&' ') {
        index += 1;
    }
    if chars.get(index) != Some(&'>') {
        return None;
    }
    index += 1;
    if chars.get(index).is_some_and(|value| value.is_whitespace()) {
        index += 1;
    }
    Some(chars[index..].iter().collect())
}

/// `(nivel, marcador, texto)` si la línea abre un elemento de lista.
pub(super) fn list_item(line: &str) -> Option<(usize, String, String)> {
    let chars = chars_of(line);
    let mut index = 0usize;
    while chars.get(index).is_some_and(|value| value.is_whitespace()) {
        index += 1;
    }
    let indent: String = chars[..index].iter().collect();
    let level = expand_tabs_len(&indent) / 2;
    if !match_rule(line)
        && matches!(chars.get(index).copied(), Some('-' | '*' | '+'))
        && chars
            .get(index + 1)
            .is_some_and(|value| value.is_whitespace())
    {
        let text_start = skip_space(&chars, index + 2);
        let text: String = chars[text_start..].iter().collect();
        return Some((level, "•".to_string(), text));
    }
    let digit_start = index;
    let mut digits = 0usize;
    while digits < 9 && chars.get(index).is_some_and(|value| value.is_numeric()) {
        index += 1;
        digits += 1;
    }
    if digits == 0 || !matches!(chars.get(index), Some('.') | Some(')')) {
        return None;
    }
    if !chars
        .get(index + 1)
        .is_some_and(|value| value.is_whitespace())
    {
        return None;
    }
    let number: String = chars[digit_start..index].iter().collect();
    let text_start = skip_space(&chars, index + 2);
    let text: String = chars[text_start..].iter().collect();
    Some((level, format!("{number}."), text))
}

fn expand_tabs_len(text: &str) -> usize {
    let mut column = 0usize;
    for character in text.chars() {
        if character == '\t' {
            column += 4 - column % 4;
        } else {
            column += 1;
        }
    }
    column
}

pub(super) fn read_list(lines: &[&str], index: usize) -> (usize, Vec<Block>) {
    let mut pending: Vec<(usize, String, String)> = Vec::new();
    let mut index = index;
    while index < lines.len() {
        let line = lines[index];
        if let Some(item) = list_item(line) {
            pending.push(item);
        } else if !py_strip(line).is_empty()
            && (line.starts_with(' ') || line.starts_with('\t'))
            && !pending.is_empty()
        {
            let last = pending.len() - 1;
            let (level, marker, text) = pending[last].clone();
            pending[last] = (level, marker, format!("{} {}", text, py_strip(line)));
        } else {
            break;
        }
        index += 1;
    }
    let base = pending
        .iter()
        .map(|(level, _, _)| *level)
        .min()
        .unwrap_or(0);
    let mut blocks = Vec::with_capacity(pending.len());
    for (level, marker, text) in pending {
        let mut text = text;
        let mut checked = None;
        if let Some((value, rest)) = match_task(&text) {
            checked = Some(value);
            text = rest;
        }
        blocks.push(Block::ListItem(ListItem {
            level: (level - base).min(4),
            marker,
            spans: parse_inline(&text),
            checked,
        }));
    }
    (index, blocks)
}

fn match_task(text: &str) -> Option<(bool, String)> {
    let chars = chars_of(text);
    if chars.first() != Some(&'[') {
        return None;
    }
    let mark = *chars.get(1)?;
    if !matches!(mark, ' ' | 'x' | 'X') || chars.get(2) != Some(&']') {
        return None;
    }
    if !chars.get(3).is_some_and(|value| value.is_whitespace()) {
        return None;
    }
    let rest: String = chars[skip_space(&chars, 3)..].iter().collect();
    Some((mark.eq_ignore_ascii_case(&'x'), rest))
}

pub(super) fn is_table_start(lines: &[&str], index: usize) -> bool {
    let Some(header) = lines.get(index) else {
        return false;
    };
    let Some(separator) = lines.get(index + 1) else {
        return false;
    };
    header.contains('|')
        && table_separator(separator)
        && separator.contains('-')
        && separator.contains('|')
}

fn table_separator(line: &str) -> bool {
    let mut text = py_strip(line);
    if let Some(rest) = text.strip_prefix('|') {
        text = rest;
    }
    if let Some(rest) = text.strip_suffix('|') {
        text = rest;
    }
    if text.is_empty() {
        return false;
    }
    text.split('|').all(|cell| {
        let mut cell = py_strip(cell);
        if let Some(rest) = cell.strip_prefix(':') {
            cell = rest;
        }
        if let Some(rest) = cell.strip_suffix(':') {
            cell = rest;
        }
        !cell.is_empty() && cell.chars().all(|value| value == '-')
    })
}

fn split_row(line: &str) -> Vec<String> {
    let mut text = py_strip(line);
    if let Some(rest) = text.strip_prefix('|') {
        text = rest;
    }
    if text.ends_with('|') && !text.ends_with("\\|") {
        text = &text[..text.len() - 1];
    }
    let mut cells: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
            current.push(character);
        } else if character == '|' {
            cells.push(py_strip(&current).to_string());
            current.clear();
        } else {
            current.push(character);
        }
    }
    cells.push(py_strip(&current).to_string());
    cells
}

fn align_of(cell: &str) -> Align {
    let cell = py_strip(cell);
    if cell.starts_with(':') && cell.ends_with(':') && cell.chars().count() > 1 {
        return Align::Center;
    }
    if cell.ends_with(':') {
        Align::Right
    } else {
        Align::Left
    }
}

pub(super) fn read_table(lines: &[&str], index: usize) -> (usize, Block) {
    let header = split_row(lines[index]);
    let mut aligns: Vec<Align> = split_row(lines[index + 1])
        .iter()
        .map(|cell| align_of(cell))
        .collect();
    let width = header.len();
    let mut rows: Vec<Vec<Vec<super::Span>>> = Vec::new();
    let mut index = index + 2;
    while index < lines.len() && !py_strip(lines[index]).is_empty() && lines[index].contains('|') {
        let mut cells = split_row(lines[index]);
        cells.truncate(width);
        cells.resize(width, String::new());
        rows.push(cells.iter().map(|cell| parse_inline(cell)).collect());
        index += 1;
    }
    aligns.resize(width, Align::Left);
    aligns.truncate(width);
    let header = header.iter().map(|cell| parse_inline(cell)).collect();
    (
        index,
        Block::Table(Table {
            header,
            rows,
            aligns,
        }),
    )
}

pub(super) fn read_quote(lines: &[&str], index: usize) -> (usize, Block) {
    let mut parts: Vec<String> = Vec::new();
    let mut index = index;
    while index < lines.len() {
        let Some(part) = match_quote(lines[index]) else {
            break;
        };
        parts.push(py_strip(&part).to_string());
        index += 1;
    }
    let text = parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    (
        index,
        Block::Quote(Quote {
            spans: parse_inline(&text),
        }),
    )
}
