//! Port of `src/ezagent/voice/stt_dictionary.py` (F2-06).

use std::collections::HashSet;
use std::fmt;

use fancy_regex::Regex;

use crate::casefold::casefold;

/// Mensajes literales de los `ValueError` del Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryError {
    TooLong,
    Malformed,
    Duplicate,
    TooManyRules,
}

impl fmt::Display for DictionaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooLong => "El diccionario admite hasta 12000 caracteres.",
            Self::Malformed => "Usa origen → corrección; separa las reglas con punto y coma.",
            Self::Duplicate => "Cada expresión de origen debe aparecer una sola vez.",
            Self::TooManyRules => "El diccionario admite hasta 100 correcciones.",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DictionaryError {}

const MAX_CHARS: usize = 12_000;
const MAX_RULES: usize = 100;

/// Whisper condiciona el prompt como si fuera la frase anterior: una línea
/// natural en español con tildes, puntuación y jerga guía mejor que una lista.
const HINT_LEAD: &str =
    "Dile a Claude Code que revise el bug: el pop-up del HUD se quedó bugueado \
                         después del commit. Agentes y herramientas: ";
const HINT_LIMIT: usize = 512;
const HINT_AGENTS: [&str; 10] = [
    "EzAgent",
    "Herdr",
    "OpenCode",
    "Claude Code",
    "Codex",
    "Gemini",
    "GPT",
    "Qwen",
    "DeepSeek",
    "MiniMax",
];

/// Colapsa espacios como `" ".join(text.split())`.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Primer separador de regla (`→` o `=>`).
fn split_arrow(line: &str) -> Option<(&str, &str)> {
    let arrow = line.find('→');
    let fat = line.find("=>");
    let (index, width) = match (arrow, fat) {
        (Some(arrow), Some(fat)) if arrow < fat => (arrow, '→'.len_utf8()),
        (Some(_), Some(fat)) => (fat, 2),
        (Some(arrow), None) => (arrow, '→'.len_utf8()),
        (None, Some(fat)) => (fat, 2),
        (None, None) => return None,
    };
    Some((&line[..index], &line[index + width..]))
}

/// Pares `oido → escrito` separados por punto y coma o salto de línea.
pub fn parse_rules(value: &str) -> Result<Vec<(String, String)>, DictionaryError> {
    if value.chars().count() > MAX_CHARS {
        return Err(DictionaryError::TooLong);
    }
    let mut rules: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in value.split([';', '\r', '\n']) {
        if line.trim().is_empty() {
            continue;
        }
        let Some((source, target)) = split_arrow(line) else {
            return Err(DictionaryError::Malformed);
        };
        if source.trim().is_empty() || target.trim().is_empty() {
            return Err(DictionaryError::Malformed);
        }
        let source = collapse(source);
        let target = collapse(target);
        if !seen.insert(casefold(&source)) {
            return Err(DictionaryError::Duplicate);
        }
        rules.push((source, target));
        if rules.len() > MAX_RULES {
            return Err(DictionaryError::TooManyRules);
        }
    }
    Ok(rules)
}

/// Valida las reglas y devuelve la representación editable de una línea.
pub fn normalize_rules(value: &str) -> Result<String, DictionaryError> {
    let rules = parse_rules(value)?;
    Ok(rules
        .iter()
        .map(|(source, target)| format!("{source} → {target}"))
        .collect::<Vec<_>>()
        .join("; "))
}

/// Sugiere grafías a Whisper sin reescribir nombres ambiguos.
pub fn transcription_hint(
    prompt: &str,
    dictionary: &str,
    agents: &[String],
) -> Result<String, DictionaryError> {
    if !prompt.trim().is_empty() {
        return Ok(prompt.to_string());
    }
    let rules = parse_rules(dictionary)?;
    let mut candidates: Vec<String> = rules.into_iter().map(|(_, target)| target).collect();
    candidates.extend(HINT_AGENTS.iter().map(|name| name.to_string()));
    candidates.extend(agents.iter().map(|name| name.replace('_', " ")));
    let mut words: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for candidate in candidates {
        let word: String = collapse(&candidate).chars().take(64).collect();
        if word.is_empty() || !seen.insert(casefold(&word)) {
            continue;
        }
        let mut joined = words.clone();
        joined.push(word.clone());
        let length = HINT_LEAD.chars().count() + joined.join(", ").chars().count() + 1;
        if length > HINT_LIMIT || words.len() >= 24 {
            break;
        }
        words.push(word);
    }
    // DECISION: son solo pistas de grafía; un nombre dudoso nunca se reemplaza
    // salvo que el usuario configure una regla.
    Ok(format!("{HINT_LEAD}{}.", words.join(", ")))
}

/// `re.IGNORECASE` del Python iguala «i», «I», «İ» (U+0130) y «ı» (U+0131);
/// el plegado de mayúsculas de `regex` no incluye las dos turcas.
fn python_ignorecase(escaped: &str) -> String {
    escaped
        .chars()
        .map(|character| match character {
            'i' | 'I' | '\u{130}' | '\u{131}' => "[iI\u{130}\u{131}]".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Aplica las reglas una vez, de la frase más larga a la más corta.
pub fn correct_transcription(text: &str, dictionary: &str) -> Result<String, DictionaryError> {
    let mut rules = parse_rules(dictionary)?;
    if rules.is_empty() {
        return Ok(text.to_string());
    }
    rules.sort_by_key(|rule| std::cmp::Reverse(rule.0.chars().count()));
    let alternatives: Vec<String> = rules
        .iter()
        .enumerate()
        .map(|(index, (source, _))| {
            let words: Vec<String> = source
                .split_whitespace()
                .map(|word| python_ignorecase(&fancy_regex::escape(word)))
                .collect();
            format!("(?P<rule{index}>{})", words.join(r"\s+"))
        })
        .collect();
    let pattern = format!(r"(?i)(?<!\w)(?:{})(?!\w)", alternatives.join("|"));
    // DECISION: el patrón solo contiene texto escapado; si no compila es un
    // error del programa y se informa como diccionario mal formado.
    let regex = Regex::new(&pattern).map_err(|_| DictionaryError::Malformed)?;
    let mut produced = String::with_capacity(text.len());
    let mut last = 0;
    for caps in regex.captures_iter(text).flatten() {
        let Some(whole) = caps.get(0) else {
            continue;
        };
        produced.push_str(&text[last..whole.start()]);
        let matched = (1..caps.len()).find(|index| caps.get(*index).is_some());
        match matched.and_then(|index| rules.get(index - 1)) {
            Some((_, target)) => produced.push_str(target),
            None => produced.push_str(whole.as_str()),
        }
        last = whole.end();
    }
    produced.push_str(&text[last..]);
    Ok(produced)
}
