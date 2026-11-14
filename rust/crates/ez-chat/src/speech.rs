//! Port of `src/ezagent/voice/chat/speech.py` (F3-14).
//!
//! La versión hablada de una respuesta: frases sin markdown y aviso de lo que
//! se queda en pantalla.

use ez_core::pyvalue::{char_len, collapse_spaces, head_chars, py_strip};

use crate::markdown::{parse, Block, Span};

pub const MAX_SPOKEN_CHARS: usize = 460;
pub const MAX_LIST_ITEMS: usize = 5;
pub const SCREEN_NOTE: &str = "El detalle está en el chat.";

fn text_of(spans: &[Span]) -> String {
    collapse_spaces(
        &spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
    )
}

fn sentence(text: &str) -> String {
    let mut text = py_strip(text).to_string();
    if !text.is_empty() && !matches!(text.chars().last(), Some('.' | '!' | '?' | '…' | ':')) {
        text.push('.');
    }
    text
}

/// Texto para decir en voz alta; vacío si no hay nada que decir.
pub fn spoken_version(markdown: &str, max_chars: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rich = false;
    let mut items = 0usize;
    for block in parse(markdown) {
        match block {
            Block::Heading(item) => parts.push(sentence(&text_of(&item.spans))),
            Block::Paragraph(item) => parts.push(sentence(&text_of(&item.spans))),
            Block::Quote(item) => parts.push(sentence(&text_of(&item.spans))),
            Block::ListItem(item) => {
                items += 1;
                if items <= MAX_LIST_ITEMS {
                    parts.push(sentence(&text_of(&item.spans)));
                } else {
                    rich = true;
                }
            }
            Block::Code(_) | Block::Table(_) | Block::Image(_) | Block::Document(_) => {
                rich = true;
            }
            Block::Rule(_) => {}
        }
    }
    let joined = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let (mut spoken, clipped) = fit(&joined, max_chars);
    if !spoken.is_empty() && (rich || clipped) {
        spoken.push(' ');
        spoken.push_str(SCREEN_NOTE);
    } else if spoken.is_empty() && rich {
        spoken = "Te lo dejé en el chat.".to_string();
    }
    spoken
}

/// `spoken_version(markdown)` con el tope por defecto.
pub fn spoken(markdown: &str) -> String {
    spoken_version(markdown, MAX_SPOKEN_CHARS)
}

/// Recorta en una frontera de frase para no cortar a media palabra.
fn fit(text: &str, limit: usize) -> (String, bool) {
    if char_len(text) <= limit {
        return (text.to_string(), false);
    }
    let mut kept = String::new();
    for sentence in split_sentences(text) {
        let candidate = py_strip(&format!("{kept} {sentence}")).to_string();
        if char_len(&candidate) > limit {
            break;
        }
        kept = candidate;
    }
    if kept.is_empty() {
        let head = head_chars(text, limit);
        let body = match head.rfind(' ') {
            Some(index) => &head[..index],
            None => head,
        };
        kept = format!("{}.", body.trim_end_matches([',', ';', ':']));
    }
    (kept, true)
}

/// `re.split(r"(?<=[.!?…])\s+", text)`.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut bytes = text.char_indices().peekable();
    while let Some((index, character)) = bytes.next() {
        if matches!(character, '.' | '!' | '?' | '…') {
            let mut end = index + character.len_utf8();
            let mut saw_space = false;
            while bytes.peek().is_some_and(|(_, next)| next.is_whitespace()) {
                saw_space = true;
                let (space_index, space) = bytes.next().expect("espacio");
                end = space_index + space.len_utf8();
            }
            if saw_space {
                parts.push(&text[start..index + character.len_utf8()]);
                start = end;
            }
        }
    }
    parts.push(&text[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prose_is_spoken_as_is() {
        assert_eq!(
            spoken_version("Hola, ¿cómo estás? Todo bien.", MAX_SPOKEN_CHARS),
            "Hola, ¿cómo estás? Todo bien."
        );
    }

    #[test]
    fn formatting_is_not_read_out() {
        let said = spoken_version(
            "# Resumen\n\nLas ventas **subieron** un `12 %` según [el informe](https://x.com).",
            MAX_SPOKEN_CHARS,
        );
        assert_eq!(
            said,
            "Resumen. Las ventas subieron un 12 % según el informe."
        );
    }

    #[test]
    fn long_lists_are_cut_and_announced() {
        let markdown = (1..=8)
            .map(|i| format!("- punto {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let said = spoken_version(&markdown, MAX_SPOKEN_CHARS);
        assert!(said.contains("punto 1.") && said.contains("punto 5."));
        assert!(!said.contains("punto 6"));
        assert!(said.ends_with(SCREEN_NOTE));
    }

    #[test]
    fn rich_content_stays_on_screen() {
        let said = spoken_version("Mira:\n\n| a | b |\n|---|---|\n| 1 | 2 |", MAX_SPOKEN_CHARS);
        assert_eq!(said, format!("Mira: {SCREEN_NOTE}"));
        assert_eq!(
            spoken_version("| a |\n|---|\n| 1 |", MAX_SPOKEN_CHARS),
            "Te lo dejé en el chat."
        );
    }

    #[test]
    fn long_answers_are_cut_at_a_sentence_boundary() {
        let long = (0..60)
            .map(|i| format!("Esta es la frase número {i} del resumen."))
            .collect::<Vec<_>>()
            .join(" ");
        let said = spoken_version(&long, MAX_SPOKEN_CHARS);
        assert!(char_len(&said) <= MAX_SPOKEN_CHARS + char_len(SCREEN_NOTE) + 1);
        assert!(said.ends_with(SCREEN_NOTE));
    }

    #[test]
    fn one_enormous_sentence_is_cut_at_a_word() {
        let said = spoken_version(&"palabra ".repeat(300), MAX_SPOKEN_CHARS);
        assert!(char_len(&said) < MAX_SPOKEN_CHARS + char_len(SCREEN_NOTE) + 5);
        assert!(said.ends_with(SCREEN_NOTE) && said.contains("palabra palabra"));
    }

    #[test]
    fn empty_input_says_nothing() {
        assert_eq!(spoken(""), "");
        assert_eq!(spoken("   \n\n"), "");
        assert_eq!(spoken("---"), "");
    }
}
