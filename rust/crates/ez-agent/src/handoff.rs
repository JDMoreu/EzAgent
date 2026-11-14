//! Port of `src/ezagent/agent/handoff.py` (F3-02).
//!
//! Traspaso de resultados entre agentes: solo la respuesta, sin la pantalla.
//! La salida de un agente es un dato no confiable: aquí solo se recorta y se
//! entrega; nunca se interpreta.

use std::sync::OnceLock;

use ez_core::pyvalue::{
    char_len, collapse_spaces, head_chars, py_rstrip, py_splitlines, py_strip, tail_chars,
};
use regex::Regex;

pub const MAX_ANSWER_CHARS: usize = 60_000;
const TAIL_LINES: usize = 30;
const ECHO_END: &str = "</hc>";

fn bullet_start() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[●•⏺]\s+").expect("patrón válido"))
}

fn bullet_line() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^\s*[●•⏺]\s+").expect("patrón válido"))
}

fn footer() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?x)^(?:
        [╹╰╯╭╮┌┐└┘├┤─━│┃▀▄█]{2,}
      | (?:└\s)?(?:Worked\ for|Completed\ in)\b
      | [✻✽✶✢✳]\s.*\bfor\s\d
      | [›❯]\s?(?:Ask\b|$)
      | ⏵ | Message\s· | Tip: | \[hc\] | \?\sfor\sshortcuts | esc\sto\s
      | Suggested\snext\smessage | .*\s↗$
      | (?:Build|Plan|Default|Ask)\s·\s
      | .*\s·\s\d+(?:\.\d+)?\s?(?:s|ms|m)\b
      | GPT-[\w.]+\s(?:low|medium|high|xhigh|max|ultra|default)\b
    )",
        )
        .expect("patrón de pie válido")
    })
}

fn thinking() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(?:└\s*)?(?:[•●+▸▾]\s*)?(?:Thought|Thinking)\b").expect("patrón válido")
    })
}

fn screen_lines(screen: &str) -> Vec<String> {
    py_splitlines(screen)
        .into_iter()
        .map(|line| py_rstrip(line).to_string())
        .collect()
}

/// Línea donde termina `tail`, aunque la pantalla lo parta en varias.
fn tail_line(lines: &[String], tail: &str) -> Option<usize> {
    let mut joined = String::new();
    let mut ends = Vec::with_capacity(lines.len());
    let squeezed: Vec<String> = lines.iter().map(|line| collapse_spaces(line)).collect();
    for text in &squeezed {
        if !text.is_empty() {
            if !joined.is_empty() {
                joined.push(' ');
            }
            joined.push_str(text);
        }
        ends.push(joined.len());
    }
    let position = joined.rfind(tail)?;
    let end = position + tail.len();
    ends.iter()
        .enumerate()
        .find(|(index, line_end)| **line_end >= end && !squeezed[*index].is_empty())
        .map(|(index, _)| index)
}

/// Índice de la línea donde termina el eco del mensaje enviado (-1 es «antes de la primera»).
fn anchor(lines: &[String], sent_task: &str) -> Option<isize> {
    // 1) Si el programa marca el origen (<hc from=..>), el cierre es exacto.
    if let Some(index) = lines.iter().rposition(|line| line.contains(ECHO_END)) {
        return Some(index as isize);
    }
    // 2) El texto que EzAgent pegó: su final, aunque se haya partido en líneas.
    let flat = collapse_spaces(sent_task);
    let tail = tail_chars(&flat, 60);
    if char_len(tail) >= 8 {
        if let Some(found) = tail_line(lines, tail) {
            return Some(found as isize);
        }
    }
    // Sin eco reconocible: la última viñeta de respuesta marca el inicio.
    lines
        .iter()
        .rposition(|line| bullet_line().is_match(line))
        .map(|index| index as isize - 1)
}

/// La última respuesta del agente, o `""` si no la reconoce.
pub fn extract_answer(screen: &str, sent_task: &str) -> String {
    let lines = screen_lines(screen);
    let Some(start) = anchor(&lines, sent_task) else {
        return String::new();
    };
    let from = usize::try_from(start + 1).unwrap_or(0).min(lines.len());
    let mut collected: Vec<String> = Vec::new();
    for raw in &lines[from..] {
        let text = py_strip(raw);
        if text.is_empty() {
            if collected.last().is_some_and(|last| !last.is_empty()) {
                collected.push(String::new());
            }
            continue;
        }
        if thinking().is_match(text) {
            continue;
        }
        if footer().is_match(text) {
            break;
        }
        collected.push(text.to_string());
    }
    while collected.last().is_some_and(String::is_empty) {
        collected.pop();
    }
    if let Some(first) = collected.first_mut() {
        *first = bullet_start().replace(first, "").into_owned();
    }
    let answer = collected.join("\n");
    head_chars(py_strip(&answer), MAX_ANSWER_CHARS).to_string()
}

/// Respaldo: las últimas líneas útiles de la pantalla, sin marcos ni pie.
pub fn clean_tail(screen: &str) -> String {
    let kept: Vec<&str> = screen_lines_ref(screen)
        .into_iter()
        .map(py_strip)
        .filter(|line| !line.is_empty() && !footer().is_match(line) && !thinking().is_match(line))
        .collect();
    let start = kept.len().saturating_sub(TAIL_LINES);
    tail_chars(&kept[start..].join("\n"), MAX_ANSWER_CHARS).to_string()
}

fn screen_lines_ref(screen: &str) -> Vec<&str> {
    py_splitlines(screen).into_iter().map(py_rstrip).collect()
}

/// La respuesta limpia si se reconoce; si no, la cola de la pantalla.
pub fn answer_or_tail(screen: &str, sent_task: &str) -> String {
    let answer = extract_answer(screen, sent_task);
    if answer.is_empty() {
        clean_tail(screen)
    } else {
        answer
    }
}

/// Instrucción + resultado previo, como si se copiara y pegara.
///
/// Con `{output}` en la tarea, la respuesta va en ese lugar; sin él, se añade
/// debajo entre comillas triples para delimitarla como dato.
pub fn compose_handoff(task: &str, answer: &str) -> String {
    let task = py_strip(task);
    let answer = py_strip(answer);
    if answer.is_empty() {
        return task.to_string();
    }
    if task.contains("{output}") {
        return task.replace("{output}", answer);
    }
    if task.is_empty() {
        answer.to_string()
    } else {
        format!("{task}\n\n\"\"\"\n{answer}\n\"\"\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_answer_after_echo() {
        let screen =
            "<hc from=\"x\">hazlo</hc>\n● Listo, creé a.txt\n\nUsa a.txt\nWorked for 3s\n› ";
        assert_eq!(extract_answer(screen, ""), "Listo, creé a.txt\n\nUsa a.txt");
    }

    #[test]
    fn falls_back_to_last_bullet_and_tail() {
        assert_eq!(extract_answer("● hola\nfin", ""), "hola\nfin");
        assert_eq!(extract_answer("sin eco", ""), "");
        assert_eq!(
            answer_or_tail("sin eco\nBuild · modelo · 2.4s", ""),
            "sin eco"
        );
    }

    #[test]
    fn composes_handoffs() {
        assert_eq!(compose_handoff("Revisa:\n{output}", " ok "), "Revisa:\nok");
        assert_eq!(
            compose_handoff("Revisa", "ok"),
            "Revisa\n\n\"\"\"\nok\n\"\"\""
        );
        assert_eq!(compose_handoff("", "ok"), "ok");
        assert_eq!(compose_handoff(" t ", ""), "t");
    }
}
