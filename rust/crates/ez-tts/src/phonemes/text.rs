//! The `phonemizer` pipeline used by Kokoro: punctuation, chunks and restore.
//!
//! `phonemizer.phonemize(text, lang, preserve_punctuation=True,
//! with_stress=True)` splits the text at punctuation marks, phonemizes every
//! chunk (and every group separated by quotes or brackets as a whole) and puts
//! the marks back. Only the parts Kokoro needs are ported: the default
//! separator (`word = " "`, empty phone/syllable), `keep-flags` and
//! `words_mismatch="ignore"`, which are the values `kokoro_onnx` passes.

use std::collections::{HashMap, HashSet};

use ez_core::pyvalue::py_strip;

use super::PhonemeError;

/// `Punctuation.default_marks()`.
const DEFAULT_MARKS: &str = ";:,.!?¡¿—…\"«»“”(){}[]";
/// `_INLINE_MARKS`: quotes and brackets do not end the utterance.
const INLINE_MARKS: &str = "\"«»“”‘’„‹›()[]{}";

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Part {
    Chunk(usize),
    Mark(String),
}

fn is_space(character: char) -> bool {
    ez_core::pyvalue::is_py_space(character)
}

fn is_mark_at(chars: &[(usize, char)], index: usize) -> bool {
    let character = chars[index].1;
    if !DEFAULT_MARKS.contains(character) {
        return false;
    }
    if character == ',' || character == '.' {
        let previous = index
            .checked_sub(1)
            .and_then(|previous| chars.get(previous))
            .is_some_and(|(_, value)| value.is_ascii_digit());
        let next = chars
            .get(index + 1)
            .is_some_and(|(_, value)| value.is_ascii_digit());
        // `(?<![0-9])[.,]|[.,](?![0-9])`: a decimal separator is not a mark.
        return !(previous && next);
    }
    true
}

/// Spans of `line` that hold whitespace plus at least one punctuation mark.
fn mark_spans(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let starts_run = is_space(chars[index].1) || is_mark_at(&chars, index);
        if !starts_run {
            index += 1;
            continue;
        }
        let mut end = index;
        let mut has_mark = false;
        while end < chars.len() && (is_space(chars[end].1) || is_mark_at(&chars, end)) {
            has_mark |= is_mark_at(&chars, end);
            end += 1;
        }
        if has_mark {
            let start_byte = chars[index].0;
            let end_byte = chars.get(end).map_or(line.len(), |(offset, _)| *offset);
            spans.push((start_byte, end_byte));
        }
        index = end;
    }
    spans
}

fn preserve_line(line: &str, chunks: &mut Vec<String>) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    for (start, end) in mark_spans(line) {
        if start > cursor {
            parts.push(Part::Chunk(chunks.len()));
            chunks.push(line[cursor..start].to_string());
        }
        parts.push(Part::Mark(line[start..end].to_string()));
        cursor = end;
    }
    if cursor < line.len() {
        parts.push(Part::Chunk(chunks.len()));
        chunks.push(line[cursor..].to_string());
    }
    parts
}

fn is_inline(mark: &str) -> bool {
    let cleaned: String = mark
        .chars()
        .filter(|character| !is_space(*character))
        .collect();
    !cleaned.is_empty()
        && cleaned
            .chars()
            .all(|character| INLINE_MARKS.contains(character))
}

fn inline_groups(lines: &[Vec<Part>]) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    for parts in lines {
        let mut group: Vec<usize> = Vec::new();
        for (number, part) in parts.iter().enumerate() {
            let Part::Chunk(index) = *part else {
                continue;
            };
            let follows_mark = number >= 2
                && group.last().is_some_and(|last| {
                    parts
                        .get(number - 2)
                        .and_then(|part| match part {
                            Part::Chunk(previous) => Some(*previous),
                            Part::Mark(_) => None,
                        })
                        .is_some_and(|previous| previous == *last)
                })
                && parts.get(number - 1).is_some_and(|mark| match mark {
                    Part::Mark(text) => is_inline(text),
                    Part::Chunk(_) => false,
                });
            if follows_mark {
                group.push(index);
                continue;
            }
            if group.len() > 1 {
                groups.push(group.clone());
            }
            group = vec![index];
        }
        if group.len() > 1 {
            groups.push(group);
        }
    }
    groups
}

fn words(text: &str) -> Vec<&str> {
    text.split(' ').filter(|word| !word.is_empty()).collect()
}

/// `Punctuation._context`: split an in-context result at its inline marks.
fn split_context(chunks: &[String], group: &[usize], phonemized: &str) -> Option<Vec<String>> {
    if group.len() < 2 {
        return None;
    }
    let mut counts = Vec::with_capacity(group.len());
    for index in group {
        counts.push(words(chunks.get(*index)?).len());
    }
    if counts.contains(&0) {
        return None;
    }
    let pieces: Vec<&str> = phonemized.split(' ').collect();
    if pieces.iter().filter(|piece| !piece.is_empty()).count() != counts.iter().sum::<usize>() {
        return None;
    }
    let starts: HashSet<usize> = counts[..counts.len() - 1]
        .iter()
        .scan(0_usize, |total, count| {
            *total += *count;
            Some(*total)
        })
        .collect();
    let mut cuts = vec![0];
    let mut seen = 0;
    for (number, piece) in pieces.iter().enumerate() {
        if piece.is_empty() {
            continue;
        }
        if starts.contains(&seen) {
            cuts.push(number);
        }
        seen += 1;
    }
    cuts.push(pieces.len());
    Some(
        cuts.windows(2)
            .map(|pair| pieces[pair[0]..pair[1]].join(" "))
            .collect(),
    )
}

fn restore(chunks: &[String], in_context: &HashMap<usize, String>, parts: &[Part]) -> String {
    let mut out = String::new();
    let mut mark: Option<String> = None;
    for part in parts {
        match part {
            Part::Mark(text) => {
                if out.ends_with(' ') {
                    out.pop();
                }
                mark = Some(text.clone());
                out.push_str(text);
            }
            Part::Chunk(index) => {
                let text = in_context.get(index).map_or_else(
                    || chunks.get(*index).map_or("", String::as_str),
                    String::as_str,
                );
                if !text.is_empty() {
                    out.push_str(text);
                    mark = None;
                }
            }
        }
    }
    if let Some(mark) = mark {
        if !mark.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}

/// `phonemizer._phonemize`: string in, string out, empty lines dropped.
pub(super) fn split_lines(text: &str) -> Vec<String> {
    text.trim_matches(['\r', '\n'])
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .filter(|line| !py_strip(line).is_empty())
        .collect()
}

/// `phonemizer.phonemize(text, lang, preserve_punctuation=True, with_stress=True)`.
pub(super) fn phonemize_text<F>(text: &str, mut phonemize_chunk: F) -> Result<String, PhonemeError>
where
    F: FnMut(&str) -> Result<String, PhonemeError>,
{
    let lines = split_lines(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut per_line: Vec<Vec<Part>> = Vec::new();
    for line in &lines {
        per_line.push(preserve_line(line, &mut chunks));
    }

    let mut phonemized = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        phonemized.push(phonemize_chunk(chunk)?);
    }
    let mut in_context: HashMap<usize, String> = HashMap::new();
    for group in inline_groups(&per_line) {
        let joined = group
            .iter()
            .filter_map(|index| chunks.get(*index).map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        let answer = phonemize_chunk(&joined)?;
        if let Some(parts) = split_context(&phonemized, &group, &answer) {
            for (index, text) in group.iter().zip(parts) {
                in_context.insert(*index, text);
            }
        }
    }

    Ok(per_line
        .iter()
        .map(|parts| restore(&phonemized, &in_context, parts))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn collapse_underscores(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_underscore = false;
    for character in text.chars() {
        if character == '_' {
            if !previous_underscore {
                out.push(character);
            }
            previous_underscore = true;
        } else {
            out.push(character);
            previous_underscore = false;
        }
    }
    out
}

/// `EspeakBackend._postprocess_line` for the options Kokoro uses.
pub(super) fn postprocess(line: &str) -> String {
    let mut text: String = py_strip(line)
        .chars()
        .map(|character| if character == '\n' { ' ' } else { character })
        .collect();
    text = text.replace("  ", " ");
    if text.contains("__") {
        text = collapse_underscores(&text);
    }
    text = text.replace("_ ", " ");
    if text.is_empty() {
        return String::new();
    }
    // `strip=False` appends one word separator after the last word.
    let mut out = text
        .split(' ')
        .map(|word| py_strip(word).replace('_', ""))
        .collect::<Vec<_>>()
        .join(" ");
    out.push(' ');
    out
}
