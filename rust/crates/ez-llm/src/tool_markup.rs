//! Port of `src/ezagent/voice/tool_markup.py` (F3-01).
//!
//! Detecta el marcado interno de «llamar a una herramienta» que algunos
//! modelos escriben como texto. Eso no es una llamada real y nunca debe llegar
//! a la voz ni al chat: se corta en cuanto aparece, aunque llegue partido en
//! varios fragmentos del streaming.

/// En minúsculas: la comparación ignora mayúsculas.
const MARKERS: [&str; 13] = [
    "<｜dsml｜",
    "<|dsml|",
    "<dsml",
    "｜dsml｜",
    "<｜tool▁call",
    "<｜tool_call",
    "<|tool_call",
    "<tool_call>",
    "<tool_calls>",
    "<function_call",
    "<function_calls>",
    "[tool_calls]",
    "<|python_tag|>",
];

/// Lo dice el código, no el modelo: el modelo no llegó a ejecutar nada.
pub const TOOL_MARKUP_NOTICE: &str = "No pude ejecutar esa acción porque la herramienta no respondió. No se hizo nada; inténtalo de nuevo.";

/// Filtro por ronda: deja pasar el texto normal y descarta desde el primer marcador.
#[derive(Debug, Default, Clone)]
pub struct ToolMarkupGuard {
    held: String,
    pub tripped: bool,
}

fn longest_marker() -> usize {
    MARKERS
        .iter()
        .map(|marker| marker.chars().count())
        .max()
        .unwrap_or(0)
}

/// `str.lower()` carácter a carácter: conserva la longitud en caracteres
/// salvo en los pocos casos (`İ`) donde Python también expande.
fn lowered(text: &str) -> String {
    text.to_lowercase()
}

impl ToolMarkupGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Texto seguro para mostrar/hablar; guarda una cola que podría ser inicio de marcador.
    pub fn feed(&mut self, text: &str) -> String {
        if self.tripped || text.is_empty() {
            return String::new();
        }
        let buffer = format!("{}{text}", self.held);
        let lower = lowered(&buffer);
        let cut = MARKERS.iter().filter_map(|marker| lower.find(marker)).min();
        if let Some(cut) = cut {
            self.tripped = true;
            self.held.clear();
            return char_prefix(&buffer, &lower, cut);
        }
        let keep = partial_marker_len(&lower);
        let total = buffer.chars().count();
        let (safe, held) = split_chars(&buffer, total - keep);
        self.held = held;
        safe
    }

    /// Al terminar el stream: lo retenido no era un marcador, así que se libera.
    pub fn flush(&mut self) -> String {
        let held = if self.tripped {
            String::new()
        } else {
            std::mem::take(&mut self.held)
        };
        self.held.clear();
        held
    }
}

/// Tamaño (en caracteres) de la cola que podría ser el inicio de un marcador.
fn partial_marker_len(lower: &str) -> usize {
    let chars: Vec<char> = lower.chars().collect();
    let limit = chars.len().min(longest_marker().saturating_sub(1));
    for size in (1..=limit).rev() {
        let tail: String = chars[chars.len() - size..].iter().collect();
        if MARKERS.iter().any(|marker| marker.starts_with(&tail)) {
            return size;
        }
    }
    0
}

/// `buffer[:cut]` donde `cut` es un índice de bytes en la versión en minúsculas.
fn char_prefix(buffer: &str, lower: &str, cut: usize) -> String {
    let count = lower[..cut].chars().count();
    buffer.chars().take(count).collect()
}

fn split_chars(text: &str, count: usize) -> (String, String) {
    let head: String = text.chars().take(count).collect();
    let tail: String = text.chars().skip(count).collect();
    (head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_normal_text() {
        let mut guard = ToolMarkupGuard::new();
        assert_eq!(guard.feed("hola "), "hola ");
        assert_eq!(guard.feed("mundo"), "mundo");
        assert!(!guard.tripped);
    }

    #[test]
    fn cuts_at_marker_even_when_split() {
        let mut guard = ToolMarkupGuard::new();
        assert_eq!(guard.feed("ok <tool_"), "ok ");
        assert_eq!(guard.feed("call>{...}"), "");
        assert!(guard.tripped);
        assert_eq!(guard.feed("más"), "");
        assert_eq!(guard.flush(), "");
    }

    #[test]
    fn releases_false_alarms_on_flush() {
        let mut guard = ToolMarkupGuard::new();
        assert_eq!(guard.feed("a <to"), "a ");
        assert_eq!(guard.flush(), "<to");
    }
}
