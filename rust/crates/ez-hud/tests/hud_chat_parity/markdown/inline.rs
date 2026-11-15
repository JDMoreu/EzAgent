//! Análisis inline del markdown: énfasis, código y enlaces.

use super::{clean_target, Span};
use ez_core::pyvalue::py_strip;

const ESCAPABLE: &str = "\\`*_{}[]()#+-.!|>~";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Escape,
    Code(usize),
    Bold(usize),
    Italic(usize),
    Link,
}

/// Parte una línea en trozos con estilo; el énfasis sin cerrar queda literal.
pub fn parse_inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    merge(inline(&chars, false, false, None))
}

fn inline(chars: &[char], bold: bool, italic: bool, link: Option<&str>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut pos = 0usize;
    let mut plain_from = 0usize;

    while pos < chars.len() {
        let Some((start, token)) = next_token(chars, pos) else {
            break;
        };
        match token {
            Token::Escape => {
                flush(chars, plain_from, start, bold, italic, link, &mut out);
                out.push(Span {
                    text: chars[start + 1].to_string(),
                    bold,
                    italic,
                    code: false,
                    link: link.map(str::to_string),
                });
                pos = start + 2;
                plain_from = pos;
            }
            Token::Code(length) => {
                let Some(close) = find_run(chars, start + length, '`', length) else {
                    pos = start + length;
                    continue;
                };
                flush(chars, plain_from, start, bold, italic, link, &mut out);
                let inner: String = chars[start + length..close].iter().collect();
                let stripped = py_strip(&inner);
                let text = if stripped.is_empty() {
                    inner
                } else {
                    stripped.to_string()
                };
                out.push(Span {
                    text,
                    bold,
                    italic,
                    code: true,
                    link: link.map(str::to_string),
                });
                pos = close + length;
                plain_from = pos;
            }
            Token::Bold(length) | Token::Italic(length) => {
                let marker = chars[start];
                let Some(close) = find_close(chars, start + length, marker, length) else {
                    pos = start + length;
                    continue;
                };
                flush(chars, plain_from, start, bold, italic, link, &mut out);
                let is_bold = matches!(token, Token::Bold(_));
                out.extend(inline(
                    &chars[start + length..close],
                    bold || is_bold,
                    italic || !is_bold,
                    link,
                ));
                pos = close + length;
                plain_from = pos;
            }
            Token::Link => {
                let end = link_end(chars, start + 1);
                let tail = end.and_then(|end| link_tail(chars, end));
                let Some((target, tail_end)) = tail else {
                    pos = start + 1;
                    continue;
                };
                flush(chars, plain_from, start, bold, italic, link, &mut out);
                out.extend(inline(
                    &chars[start + 1..end.unwrap_or(start + 1)],
                    bold,
                    italic,
                    Some(&clean_target(&target)),
                ));
                pos = tail_end;
                plain_from = pos;
            }
        }
    }
    flush(chars, plain_from, chars.len(), bold, italic, link, &mut out);
    out
}

fn flush(
    chars: &[char],
    from: usize,
    to: usize,
    bold: bool,
    italic: bool,
    link: Option<&str>,
    out: &mut Vec<Span>,
) {
    if to <= from {
        return;
    }
    out.push(Span {
        text: chars[from..to].iter().collect(),
        bold,
        italic,
        code: false,
        link: link.map(str::to_string),
    });
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn next_token(chars: &[char], from: usize) -> Option<(usize, Token)> {
    let mut index = from;
    while index < chars.len() {
        let character = chars[index];
        let next = chars.get(index + 1).copied();
        let after = chars.get(index + 2).copied();
        if character == '\\' && next.is_some_and(|value| ESCAPABLE.contains(value)) {
            return Some((index, Token::Escape));
        }
        if character == '`' {
            let mut length = 1;
            while chars.get(index + length) == Some(&'`') {
                length += 1;
            }
            return Some((index, Token::Code(length)));
        }
        if (character == '*' || character == '_')
            && next == Some(character)
            && after.is_some_and(|value| !value.is_whitespace())
        {
            return Some((index, Token::Bold(2)));
        }
        if character == '*' && next.is_some_and(|value| !value.is_whitespace() && value != '*') {
            return Some((index, Token::Italic(1)));
        }
        let previous_is_word = index > 0 && is_word(chars[index - 1]);
        if character == '_'
            && !previous_is_word
            && next.is_some_and(|value| !value.is_whitespace() && value != '_')
        {
            return Some((index, Token::Italic(1)));
        }
        if character == '[' {
            return Some((index, Token::Link));
        }
        index += 1;
    }
    None
}

fn find_run(chars: &[char], from: usize, character: char, length: usize) -> Option<usize> {
    let mut index = from;
    while index + length <= chars.len() {
        if chars[index..index + length]
            .iter()
            .all(|value| *value == character)
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Posición del cierre de un énfasis: el mismo marcador, sin espacio antes.
fn find_close(chars: &[char], start: usize, marker: char, length: usize) -> Option<usize> {
    let mut pos = start;
    loop {
        let close = find_run(chars, pos, marker, length)?;
        let before = if close > start {
            Some(chars[close - 1])
        } else {
            None
        };
        let after = chars.get(close + length).copied();
        let escaped = close > 0 && chars[close - 1] == '\\';
        let word_underscore = marker == '_' && after.is_some_and(|value| value.is_alphanumeric());
        if close > start
            && before.is_some_and(|value| !value.is_whitespace())
            && !escaped
            && !word_underscore
        {
            return Some(close);
        }
        pos = close + 1;
    }
}

/// Índice del «]» que cierra el texto de un enlace (admite corchetes anidados).
fn link_end(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (index, character) in chars.iter().enumerate().skip(start) {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Destino y final de `](destino "título")` a partir del «]».
fn link_tail(chars: &[char], end: usize) -> Option<(String, usize)> {
    if chars.get(end) != Some(&']') || chars.get(end + 1) != Some(&'(') {
        return None;
    }
    let mut index = end + 2;
    index = skip_space(chars, index);
    let (target, next) = if chars.get(index) == Some(&'<') {
        let mut close = index + 1;
        while close < chars.len() && chars[close] != '>' && chars[close] != '\n' {
            close += 1;
        }
        if close >= chars.len() || chars[close] != '>' || close == index + 1 {
            return None;
        }
        (chars[index..=close].iter().collect::<String>(), close + 1)
    } else {
        let start = index;
        while index < chars.len() && chars[index] != ')' && !chars[index].is_whitespace() {
            index += 1;
        }
        if index == start {
            return None;
        }
        (chars[start..index].iter().collect::<String>(), index)
    };
    let tail_end = rest_prefix(chars, next)?;
    Some((target, tail_end))
}

/// Final del `(?:\s+"[^"]*")?\s*\)` que empieza en `from`, sin exigir fin de texto.
fn rest_prefix(chars: &[char], from: usize) -> Option<usize> {
    let after_space = skip_space(chars, from);
    let had_space = after_space > from;
    if had_space && chars.get(after_space) == Some(&'"') {
        let mut close = after_space + 1;
        while close < chars.len() && chars[close] != '"' {
            close += 1;
        }
        if close < chars.len() {
            let index = skip_space(chars, close + 1);
            if chars.get(index) == Some(&')') {
                return Some(index + 1);
            }
        }
    }
    let mut index = skip_space(chars, from);
    if chars.get(index) == Some(&')') {
        index += 1;
        return Some(index);
    }
    None
}

/// `(?:\s+"[^"]*")?\s*\)\s*$` desde `from`.
pub(super) fn rest_matches(chars: &[char], from: usize) -> bool {
    rest_prefix(chars, from).is_some_and(|end| skip_space(chars, end) == chars.len())
}

pub(super) fn skip_space(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    index
}

fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if !span.code
                && !last.code
                && (last.bold, last.italic, &last.link) == (span.bold, span.italic, &span.link)
            {
                last.text.push_str(&span.text);
                continue;
            }
        }
        out.push(span);
    }
    out
}
