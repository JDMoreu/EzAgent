//! Screen classification helpers of `incidents.py`: tail, patterns and reset
//! times. Hand-written instead of the Python regexes because this crate does
//! not declare the `regex` dependency.

use chrono::{Duration, Local, NaiveDateTime, TimeZone};

use ez_core::pyvalue::{collapse_spaces, head_chars, is_py_space, py_splitlines};

use super::{Signal, TAIL_LINES};

pub(super) fn tail(screen: &str) -> Vec<String> {
    let lines: Vec<String> = py_splitlines(screen)
        .into_iter()
        .map(collapse_spaces)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..].to_vec()
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Reset time mentioned in `text` (epoch), or 0.
pub fn parse_reset(text: &str, now: NaiveDateTime) -> f64 {
    let lowered = text.to_lowercase();
    let clock = find_clock(&lowered, clock12).or_else(|| find_clock(&lowered, clock24));
    if let Some((hour, minute)) = clock {
        // DECISION: Python raises ValueError for a minute like 7:60pm and the
        // supervisor tick dies; the port treats it as «no reset time».
        let Some(moment) = now.date().and_hms_opt(hour, minute, 0) else {
            return 0.0;
        };
        let moment = if moment <= now {
            moment + Duration::days(1)
        } else {
            moment
        };
        return to_timestamp(moment);
    }
    in_span(&lowered).map_or(0.0, |span| {
        to_timestamp(now + Duration::seconds(span as i64))
    })
}

fn to_timestamp(moment: NaiveDateTime) -> f64 {
    Local
        .from_local_datetime(&moment)
        .earliest()
        .map_or(0.0, |local| local.timestamp() as f64)
}

/// First successful clock match in the text, trying every trigger occurrence.
fn find_clock(text: &str, parse: fn(&str, usize) -> Option<(u32, u32)>) -> Option<(u32, u32)> {
    for (at, _) in text.char_indices() {
        for trigger in [
            "resets",
            "reset",
            "try again",
            "available",
            "disponible",
            "vuelve",
        ] {
            if text[at..].starts_with(trigger) {
                if let Some(clock) = parse(text, at + trigger.len()) {
                    return Some(clock);
                }
            }
        }
    }
    None
}

fn clock12(text: &str, start: usize) -> Option<(u32, u32)> {
    let mut skipped = 0usize;
    for (offset, character) in text[start..].char_indices() {
        if character.is_ascii_digit() {
            return clock12_at(text, start + offset);
        }
        skipped += 1;
        if skipped > 12 {
            return None;
        }
    }
    None
}

fn clock12_at(text: &str, at: usize) -> Option<(u32, u32)> {
    let characters: Vec<char> = text[at..].chars().collect();
    for take in [2usize, 1] {
        if characters.len() < take {
            continue;
        }
        if !characters[..take].iter().all(char::is_ascii_digit) {
            continue;
        }
        let hour: u32 = characters[..take].iter().collect::<String>().parse().ok()?;
        let mut index = take;
        let mut minute = 0u32;
        if characters.get(index) == Some(&':') {
            let first = characters.get(index + 1)?;
            let second = characters.get(index + 2)?;
            if !first.is_ascii_digit() || !second.is_ascii_digit() {
                continue;
            }
            minute = format!("{first}{second}").parse().ok()?;
            index += 3;
        }
        while characters.get(index).is_some_and(|c| is_py_space(*c)) {
            index += 1;
        }
        let meridiem = match characters.get(index) {
            Some('a') => false,
            Some('p') => true,
            _ => continue,
        };
        index += 1;
        if characters.get(index) == Some(&'.') {
            index += 1;
        }
        while characters.get(index).is_some_and(|c| is_py_space(*c)) {
            index += 1;
        }
        if characters.get(index) != Some(&'m') {
            continue;
        }
        index += 1;
        if characters.get(index).is_some_and(|c| is_word(*c)) {
            continue;
        }
        let hour = hour % 12 + if meridiem { 12 } else { 0 };
        return Some((hour, minute));
    }
    None
}

fn clock24(text: &str, start: usize) -> Option<(u32, u32)> {
    let mut skipped = 0usize;
    for (offset, character) in text[start..].char_indices() {
        if character.is_ascii_digit() {
            let at = start + offset;
            let boundary = text[..at].chars().next_back().is_none_or(|c| !is_word(c));
            if boundary {
                return clock24_at(text, at);
            }
            return None;
        }
        skipped += 1;
        if skipped > 12 {
            return None;
        }
    }
    None
}

fn clock24_at(text: &str, at: usize) -> Option<(u32, u32)> {
    let characters: Vec<char> = text[at..].chars().collect();
    let first = *characters.first()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let second = characters.get(1).copied().unwrap_or(' ');
    let (hour, index) = if ['0', '1'].contains(&first) && second.is_ascii_digit() {
        (first.to_digit(10)? * 10 + second.to_digit(10)?, 2)
    } else if characters.get(1) == Some(&':') {
        (first.to_digit(10)?, 1)
    } else if first == '2' && ('0'..='3').contains(&second) {
        (2 * 10 + second.to_digit(10)?, 2)
    } else {
        return None;
    };
    if hour > 23 || characters.get(index) != Some(&':') {
        return None;
    }
    let ten = *characters.get(index + 1)?;
    let one = *characters.get(index + 2)?;
    if !('0'..='5').contains(&ten) || !one.is_ascii_digit() {
        return None;
    }
    let minute = ten.to_digit(10)? * 10 + one.to_digit(10)?;
    if characters
        .get(index + 3)
        .is_some_and(|character| is_word(*character))
    {
        return None;
    }
    Some((hour, minute))
}

fn in_span(text: &str) -> Option<f64> {
    for (offset, _) in text.char_indices() {
        for trigger in ["resets", "reset", "try again", "in", "en"] {
            if text[offset..].starts_with(trigger) {
                if let Some(span) = in_at(text, offset + trigger.len()) {
                    return Some(span);
                }
            }
        }
    }
    None
}

fn digits_at(text: &str, from: usize) -> Option<(u64, usize)> {
    let count: usize = text[from..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    if count == 0 {
        return None;
    }
    let value = text[from..from + count].parse().ok()?;
    Some((value, from + count))
}

fn skip_whitespace(text: &str, from: usize) -> usize {
    let count: usize = text[from..]
        .chars()
        .take_while(|character| is_py_space(*character))
        .map(char::len_utf8)
        .sum();
    from + count
}

fn in_at(text: &str, after_trigger: usize) -> Option<f64> {
    let space = skip_whitespace(text, after_trigger);
    if space == after_trigger {
        return None;
    }
    let mut rest = space;
    if text[rest..].starts_with("in") {
        let separator = skip_whitespace(text, rest + 2);
        if separator > rest + 2 {
            rest = separator;
        }
    }
    let (hours, next) = parse_hours(text, rest);
    let minutes_at = skip_whitespace(text, next);
    let (minutes, _) = parse_minutes(text, minutes_at);
    if hours.is_none() && minutes.is_none() {
        return None;
    }
    Some((hours.unwrap_or(0) * 3600 + minutes.unwrap_or(0) * 60) as f64)
}

fn parse_hours(text: &str, from: usize) -> (Option<u64>, usize) {
    let Some((value, after)) = digits_at(text, from) else {
        return (None, from);
    };
    let after = skip_whitespace(text, after);
    if !text[after..].starts_with('h') {
        return (None, from);
    }
    let mut end = after + 1;
    for suffix in ["ours", "our", "oras", "ora"] {
        if text[end..].starts_with(suffix) {
            end += suffix.len();
            break;
        }
    }
    (Some(value), end)
}

fn parse_minutes(text: &str, from: usize) -> (Option<u64>, usize) {
    let Some((value, after)) = digits_at(text, from) else {
        return (None, from);
    };
    let after = skip_whitespace(text, after);
    if !text[after..].starts_with('m') {
        return (None, from);
    }
    let mut end = after + 1;
    if text[end..].starts_with("in") {
        end += 2;
        for suffix in ["utes", "ute", "utos", "uto"] {
            if text[end..].starts_with(suffix) {
                end += suffix.len();
                break;
            }
        }
    }
    if text[end..].chars().next().is_some_and(is_word) {
        return (None, from);
    }
    (Some(value), end)
}

/// Labels what the last lines of the screen show, or `None`.
pub fn classify(screen: &str, now: NaiveDateTime) -> Option<Signal> {
    let lines = tail(screen);
    for kind in ["auth", "quota", "context", "compacting"] {
        for line in lines.iter().rev() {
            if matches(kind, &line.to_lowercase()) {
                let resets = if kind == "quota" {
                    parse_reset(&lines.join(" "), now)
                } else {
                    0.0
                };
                return Some(Signal {
                    kind: kind.to_string(),
                    detail: head_chars(line, 200).to_string(),
                    resets,
                });
            }
        }
    }
    None
}

pub(super) fn matches(kind: &str, line: &str) -> bool {
    match kind {
        "auth" => is_auth(line),
        "quota" => is_quota(line),
        "context" => is_context(line),
        "compacting" => is_compacting(line),
        _ => false,
    }
}

fn is_auth(line: &str) -> bool {
    line.contains("not logged in")
        || has_please_login(line)
        || line.contains("invalid api key")
        || line.contains("authentication failed")
        || line.contains("authentication error")
        || has_token_expired(line)
        || line.contains("401 unauthorized")
        || line.contains("sesión caducada")
        || line.contains("sesión expirada")
        || line.contains("sesion caducada")
        || line.contains("sesion expirada")
}

fn has_please_login(text: &str) -> bool {
    let mut rest = text;
    while let Some(position) = rest.find("please ") {
        let mut after = &rest[position + 7..];
        if let Some(stripped) = after.strip_prefix("run ") {
            after = stripped;
        }
        if let Some(stripped) = after.strip_prefix('/') {
            after = stripped;
        }
        if let Some(stripped) = after.strip_prefix("log") {
            let tail = stripped.strip_prefix(' ').unwrap_or(stripped);
            if tail.starts_with("in") {
                return true;
            }
        }
        rest = &rest[position + 7..];
    }
    false
}

fn has_token_expired(text: &str) -> bool {
    let mut rest = text;
    while let Some(position) = rest.find("token ") {
        let after = &rest[position + 6..];
        if after.starts_with("expired")
            || after
                .strip_prefix("has ")
                .is_some_and(|tail| tail.starts_with("expired"))
        {
            return true;
        }
        rest = after;
    }
    false
}

fn is_quota(line: &str) -> bool {
    has_hit_your_limit(line)
        || line.contains("usage limit")
        || line.contains("rate limit")
        || line.contains("rate-limit")
        || line.contains("quota exceeded")
        || line.contains("quota exhausted")
        || line.contains("insufficient balance")
        || line.contains("insufficient credit")
        || line.contains("insufficient quota")
        || line.contains("credit balance is too low")
        || line.contains("out of credits")
        || line.contains("too many requests")
        || has_429(line)
        || line.contains("límite de uso")
        || line.contains("limite de uso")
        || line.contains("sin créditos")
        || line.contains("sin creditos")
}

fn has_hit_your_limit(text: &str) -> bool {
    let mut rest = text;
    while let Some(position) = rest.find("hit your ") {
        let after = &rest[position + 9..];
        if after.starts_with("limit") {
            return true;
        }
        let word_len: usize = after
            .chars()
            .take_while(|character| is_word(*character) || *character == '-')
            .map(char::len_utf8)
            .sum();
        if word_len > 0 && after[word_len..].starts_with(' ') {
            let tail = &after[word_len + 1..];
            let tail = tail.strip_prefix("usage ").unwrap_or(tail);
            if tail.starts_with("limit") {
                return true;
            }
        }
        rest = after;
    }
    false
}

fn has_429(text: &str) -> bool {
    let mut rest = text;
    while let Some(position) = rest.find("429") {
        let before = rest[..position].chars().next_back();
        let after = rest[position + 3..].chars().next();
        if before.is_none_or(|c| !is_word(c)) && after.is_none_or(|c| !is_word(c)) {
            return true;
        }
        rest = &rest[position + 3..];
    }
    false
}

fn is_context(line: &str) -> bool {
    line.contains("prompt is too long")
        || has_context_phrase(line)
        || has_exceeds_context(line)
        || line.contains("maximum context length")
        || line.contains("ran out of room in the model's context")
        || line.contains("ran out of room in the models context")
        || line.contains("context window exceeded")
        || line.contains("context window is full")
}

fn has_context_phrase(text: &str) -> bool {
    for word in ["window", "length", "limit"] {
        let needle = format!("context {word} ");
        let mut rest = text;
        while let Some(position) = rest.find(&needle) {
            let tail = &rest[position + needle.len()..];
            if tail.starts_with("exceeded")
                || tail.starts_with("reached")
                || tail.starts_with("is full")
            {
                return true;
            }
            rest = tail;
        }
    }
    false
}

fn has_exceeds_context(text: &str) -> bool {
    let mut rest = text;
    while let Some(position) = rest.find("exceed") {
        let mut tail = &rest[position + 6..];
        if let Some(stripped) = tail.strip_prefix('s') {
            tail = stripped;
        }
        if let Some(tail) = tail.strip_prefix(" the ") {
            let tail = tail
                .strip_prefix("model's ")
                .or_else(|| tail.strip_prefix("models "))
                .unwrap_or(tail);
            if tail.starts_with("context window") {
                return true;
            }
        }
        rest = &rest[position + 6..];
    }
    false
}

fn is_compacting(line: &str) -> bool {
    line.contains("compacting conversation")
        || line.contains("compacting the conversation")
        || line.contains("compacting context")
        || line.contains("compacting the context")
        || has_auto_compact(line)
        || line.contains("summarizing conversation")
        || line.contains("summarizing the conversation")
        || line.contains("summarising conversation")
        || line.contains("summarising the conversation")
}

fn has_auto_compact(text: &str) -> bool {
    for trigger in ["autocompact", "auto-compact"] {
        let mut rest = text;
        while let Some(position) = rest.find(trigger) {
            let mut tail = &rest[position + trigger.len()..];
            if let Some(stripped) = tail.strip_prefix("ing") {
                tail = stripped;
            }
            if tail.chars().next().is_none_or(|c| !is_word(c)) {
                return true;
            }
            rest = tail;
        }
    }
    false
}
