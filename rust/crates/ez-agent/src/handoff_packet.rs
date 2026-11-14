//! Port of `src/ezagent/agent/handoff_packet.py` (F3-02).
//!
//! Qué se pasa al siguiente agente: respuesta, conclusión o índice + archivo.
//! Nunca se reescribe el texto. La conclusión se localiza con reglas por agente
//! y, si se da un `select`, con un modelo pequeño que **solo devuelve un número
//! de línea**.

use std::collections::HashSet;
use std::sync::OnceLock;

use ez_core::pyvalue::{
    char_len, collapse_spaces, head_chars, py_rstrip, py_splitlines, py_strip, tail_chars,
};
use regex::Regex;

pub const SHORT_CHARS: usize = 2500;
pub const DOCUMENT_FILE_CHARS: usize = 6000;
pub const MIN_LOG_CHARS: usize = 400;
pub const MAX_FALLBACK_CHARS: usize = 1500;
const SPOKEN_CHARS: usize = 360;
const OUTLINE_LINES: usize = 24;

fn strong() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?x)^(?:
        Ran\s\d+\s
      | [•×●]\s(?:Ran|Read|Wrote|Write|Edit|Edited|Added|Updated|Deleted
                 |Explored|Searched|Listed|Thought|Create|Created)\b
      | \#\s(?:Wrote|Read|Edit|Created|Deleted|Patched)\b
      | [←→↳]\s
      | \+\s(?:Show\sdetails|Thought)\b
      | Explored:
    )",
        )
        .expect("patrón fuerte válido")
    })
}

fn weak() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(?:\$\s\S|\d{1,4}\s{1,2}\S|[⎿└]\s)").expect("patrón débil válido")
    })
}

fn bullet() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[●•⏺]\s+").expect("patrón válido"))
}

fn heading() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(?:#{1,4}\s+\S.{0,80}|\d{1,2}[.)]\s+\S.{0,80}|[A-ZÁÉÍÓÚÑ][^.!?:]{3,80}:?)$")
            .expect("patrón de título válido")
    })
}

/// Lo que se pasa al siguiente agente.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Packet {
    /// `short` | `report` | `document`.
    pub kind: String,
    /// Lo que se inserta en la tarea del siguiente agente.
    pub text: String,
    /// Lo que se diría por voz.
    pub spoken: String,
    /// Texto completo original.
    pub full: String,
    pub saved_path: String,
}

impl Packet {
    fn new(kind: &str, text: &str, spoken: &str, full: &str, saved_path: &str) -> Self {
        Self {
            kind: kind.into(),
            text: text.into(),
            spoken: spoken.into(),
            full: full.into(),
            saved_path: saved_path.into(),
        }
    }
}

/// ¿Hubo trabajo de herramientas en la salida?
pub fn has_work(text: &str) -> bool {
    py_splitlines(text)
        .into_iter()
        .any(|line| strong().is_match(py_strip(line)))
}

/// Índice de la primera línea posterior al último registro de herramientas.
fn after_last_marker(lines: &[&str]) -> usize {
    let mut last: Option<usize> = None;
    for (index, raw) in lines.iter().enumerate() {
        let text = py_strip(raw);
        if strong().is_match(text) || (last.is_some() && weak().is_match(text)) {
            last = Some(index);
        }
    }
    last.map_or(0, |index| index + 1)
}

/// Índice breve: primer párrafo + títulos reconocibles (sin inventar nada).
pub fn outline(text: &str, limit: usize) -> String {
    let lines: Vec<&str> = py_splitlines(text).into_iter().map(py_strip).collect();
    let first = lines
        .iter()
        .find(|line| !line.is_empty())
        .copied()
        .unwrap_or("");
    let mut seen: HashSet<&str> = HashSet::new();
    let unique: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            !line.is_empty()
                && char_len(line) <= 90
                && !line.ends_with('.')
                && heading().is_match(line)
        })
        .filter(|line| seen.insert(line))
        .collect();
    let mut parts: Vec<String> = Vec::new();
    if !first.is_empty() {
        parts.push(head_chars(first, 300).to_string());
    }
    if !unique.is_empty() {
        let items: Vec<String> = unique
            .iter()
            .take(limit)
            .map(|h| format!("- {h}"))
            .collect();
        parts.push(format!("Contenido:\n{}", items.join("\n")));
    }
    parts.join("\n")
}

fn first_chars(text: &str, limit: usize) -> String {
    let flat = collapse_spaces(text);
    if char_len(&flat) <= limit {
        return flat;
    }
    format!("{}…", py_rstrip(head_chars(&flat, limit - 1)))
}

/// Decide qué parte de la salida de un agente pasa al siguiente.
pub fn build_packet(
    answer: &str,
    select: Option<&dyn Fn(&[String]) -> Option<i64>>,
    save: Option<&dyn Fn(&str) -> Option<String>>,
) -> Packet {
    let answer = py_strip(answer);
    if answer.is_empty() {
        return Packet::new("short", "", "", "", "");
    }
    let lines = py_splitlines(answer);
    if !has_work(answer) {
        if char_len(answer) <= SHORT_CHARS {
            return Packet::new(
                "short",
                answer,
                &first_chars(answer, SPOKEN_CHARS),
                answer,
                "",
            );
        }
        // Texto largo sin herramientas (un plan): hasta ~6000 caracteres se pasa
        // entero; por encima, índice breve + archivo con el texto completo.
        let path = match save {
            Some(save) if char_len(answer) > DOCUMENT_FILE_CHARS => save(answer),
            _ => None,
        };
        if let Some(path) = path.filter(|path| !path.is_empty()) {
            let digest = outline(answer, OUTLINE_LINES);
            let spoken = first_chars(&digest, SPOKEN_CHARS);
            return Packet::new("document", &digest, &spoken, answer, &path);
        }
        return Packet::new(
            "document",
            answer,
            &first_chars(answer, SPOKEN_CHARS),
            answer,
            "",
        );
    }
    report_packet(answer, &lines, select, save)
}

fn report_packet(
    answer: &str,
    lines: &[&str],
    select: Option<&dyn Fn(&[String]) -> Option<i64>>,
    save: Option<&dyn Fn(&str) -> Option<String>>,
) -> Packet {
    let mut start: Option<i64> = None;
    if let Some(select) = select.filter(|_| lines.len() > 12) {
        let owned: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
        start = select(&owned);
    }
    let by_model = start.is_some_and(|value| value > 0 && value as usize <= lines.len());
    let first = match start.filter(|_| by_model) {
        Some(value) => value as usize,
        None => after_last_marker(lines) + 1,
    };
    let mut conclusion = py_strip(&lines[(first - 1).min(lines.len())..].join("\n")).to_string();
    if conclusion.is_empty() {
        let from = lines.len().saturating_sub(12);
        conclusion = py_strip(&lines[from..].join("\n")).to_string();
    }
    if !by_model && char_len(&conclusion) > MAX_FALLBACK_CHARS {
        // Tras el último comando puede haber mucha salida pegada a la
        // conclusión: sin el modelo se queda solo el final, por líneas enteras.
        let tail = tail_chars(&conclusion, MAX_FALLBACK_CHARS);
        conclusion = match tail.find('\n') {
            Some(position) => tail[position + 1..].to_string(),
            None => tail.to_string(),
        };
    }
    let conclusion = bullet().replace(&conclusion, "").into_owned();
    let mut path = String::new();
    if let Some(save) = save {
        if char_len(answer).saturating_sub(char_len(&conclusion)) > MIN_LOG_CHARS {
            path = save(answer).unwrap_or_default();
        }
    }
    let spoken = first_chars(&conclusion, SPOKEN_CHARS);
    Packet::new("report", &conclusion, &spoken, answer, &path)
}

/// Texto que se inserta en la tarea, con la ruta del archivo si la hay.
pub fn packet_text(packet: &Packet) -> String {
    if packet.saved_path.is_empty() {
        return packet.text.clone();
    }
    let note = if packet.kind == "document" {
        format!(
            "[Texto completo, léelo antes de empezar: {}]",
            packet.saved_path
        )
    } else {
        format!(
            "[Registro completo del trabajo, solo si lo necesitas: {}]",
            packet.saved_path
        )
    };
    format!("{}\n\n{note}", packet.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_answers_pass_through() {
        let packet = build_packet(" hecho ", None, None);
        assert_eq!(packet, Packet::new("short", "hecho", "hecho", "hecho", ""));
        assert_eq!(build_packet("  ", None, None).kind, "short");
    }

    #[test]
    fn work_logs_become_reports() {
        let mut log = String::from("● Ran 3 shell commands\n");
        for index in 0..60 {
            log.push_str(&format!(
                "$ comando {index} con bastante salida de relleno aqui\n"
            ));
        }
        log.push_str("● Creé a.txt en /tmp para probar.");
        let save = |text: &str| Some(format!("/tmp/log-{}.md", text.len()));
        let packet = build_packet(&log, None, Some(&save));
        assert_eq!(packet.kind, "report");
        assert_eq!(packet.text, "Creé a.txt en /tmp para probar.");
        assert!(packet.saved_path.starts_with("/tmp/log-"));
        assert!(packet_text(&packet).contains("[Registro completo del trabajo"));
    }

    #[test]
    fn long_plans_become_documents() {
        let plan = format!("Plan\n\n1. Primero\n{}", "detalle largo. ".repeat(500));
        let save = |_: &str| Some("/tmp/plan.md".to_string());
        let packet = build_packet(&plan, None, Some(&save));
        assert_eq!(packet.kind, "document");
        assert!(packet
            .text
            .starts_with("Plan\nContenido:\n- Plan\n- 1. Primero"));
        assert!(packet_text(&packet)
            .ends_with("[Texto completo, léelo antes de empezar: /tmp/plan.md]"));
    }
}
