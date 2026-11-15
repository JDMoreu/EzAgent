//! Port of `kokoro_onnx.chunker` (F4-04): batch splitting and pauses.

use ez_core::pyvalue::{char_len, is_py_space, py_rstrip, py_strip};

pub const SENTENCE_MARKS: &str = ".!?…";
pub const CLAUSE_MARKS: &str = ",;:";

/// `re.split(r"(?<=[marks])\s+", text)`: cut after a mark and its spaces.
fn split_after(text: &str, marks: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        current.push(character);
        if !marks.contains(character) {
            continue;
        }
        let mut spaced = false;
        while characters.peek().is_some_and(|next| is_py_space(*next)) {
            characters.next();
            spaced = true;
        }
        if spaced {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

fn split_spaces(text: &str) -> Vec<String> {
    text.split(is_py_space)
        .filter(|piece| !piece.is_empty())
        .map(str::to_string)
        .collect()
}

fn atoms_into(text: &str, max_length: usize, level: usize, out: &mut Vec<String>) {
    if char_len(text) <= max_length {
        if !text.is_empty() {
            out.push(text.to_string());
        }
        return;
    }
    for index in level..3 {
        let pieces = match index {
            0 => split_after(text, SENTENCE_MARKS),
            1 => split_after(text, CLAUSE_MARKS),
            _ => split_spaces(text),
        };
        if pieces.len() > 1 {
            for piece in pieces {
                atoms_into(py_strip(&piece), max_length, index + 1, out);
            }
            return;
        }
    }
    // A single unbroken run: slice it rather than dropping the tail.
    let characters: Vec<char> = text.chars().collect();
    for start in (0..characters.len()).step_by(max_length) {
        let end = (start + max_length).min(characters.len());
        out.push(characters[start..end].iter().collect());
    }
}

/// `_pack`: group consecutive atoms into batch index ranges.
fn pack(lengths: &[usize], limit: usize) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut size = 0;
    for (index, length) in lengths.iter().enumerate() {
        let candidate = if index == start {
            *length
        } else {
            size + 1 + *length
        };
        if candidate > limit && index > start {
            batches.push((start, index));
            start = index;
            size = *length;
        } else {
            size = candidate;
        }
    }
    if !lengths.is_empty() {
        batches.push((start, lengths.len()));
    }
    batches
}

/// `split_phonemes`: balanced batches of at most `max_length` phonemes.
///
/// `max_length == 0` con texto no vacío reproduce el `range() arg 3 must not be
/// zero` del Python.
pub fn split_phonemes(phonemes: &str, max_length: usize) -> Result<Vec<String>, String> {
    if max_length == 0 && !py_strip(phonemes).is_empty() {
        return Err("range() arg 3 must not be zero".to_string());
    }
    let mut atoms = Vec::new();
    atoms_into(py_strip(phonemes), max_length, 0, &mut atoms);
    if atoms.is_empty() {
        return Ok(Vec::new());
    }
    let lengths: Vec<usize> = atoms.iter().map(|atom| char_len(atom)).collect();
    let fewest = pack(&lengths, max_length).len();
    let mut low = *lengths.iter().max().unwrap_or(&0);
    let mut high = max_length;
    while low < high {
        let middle = (low + high) / 2;
        if pack(&lengths, middle).len() <= fewest {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(pack(&lengths, low)
        .into_iter()
        .map(|(start, end)| atoms[start..end].join(" "))
        .collect())
}

/// `pause_after`: silence a batch ending with this text asks for.
pub fn pause_after(phonemes: &str, sentence: f64, clause: f64) -> f64 {
    let trimmed = py_rstrip(phonemes);
    let Some(mark) = trimmed.chars().last() else {
        // `"" in SENTENCE_MARKS` is true in Python.
        return sentence;
    };
    if SENTENCE_MARKS.contains(mark) {
        return sentence;
    }
    if CLAUSE_MARKS.contains(mark) {
        return clause;
    }
    0.0
}
