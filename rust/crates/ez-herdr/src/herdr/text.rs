//! Texto de las pantallas y de la línea de comandos que se escribe en la shell.
//!
//! `strip_ansi` y `visible_name` son de `herdr.py`; `command_line` y
//! `valid_keys` vienen de `agent_session.py` y viven aquí porque `run_agent` y
//! `keys_agent` los necesitan ya (F4-11 los reutilizará desde `session`).

use std::path::Path;
use std::sync::OnceLock;

use ez_core::pyvalue::{collapse_spaces, py_splitlines, py_strip, text_or_empty};
use regex::Regex;
use serde_json::{Map, Value};

#[cfg(test)]
mod tests;

fn compiled(slot: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    slot.get_or_init(|| Regex::new(pattern).expect("patrón fijo válido"))
}

pub fn strip_ansi(text: &str) -> String {
    static OSC: OnceLock<Regex> = OnceLock::new();
    static ANSI: OnceLock<Regex> = OnceLock::new();
    static BOX: OnceLock<Regex> = OnceLock::new();
    static SPACES: OnceLock<Regex> = OnceLock::new();
    let text = compiled(&OSC, r"(?s)\x1b\].*?(?:\x07|\x1b\\)").replace_all(text, "");
    let text = compiled(&ANSI, r"\x1b\[[0-9;?]*[ -/]*[@-~]").replace_all(&text, "");
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = compiled(&BOX, r"[─━│┃┌┐└┘├┤┬┴┼╭╮╰╯╔╗╚╝║═]+").replace_all(&text, " ");
    let text = compiled(&SPACES, r"[ \t]+").replace_all(&text, " ");
    let lines: Vec<&str> = py_splitlines(&text)
        .into_iter()
        .map(py_strip)
        .filter(|line| !line.is_empty())
        .collect();
    py_strip(&lines.join("\n")).to_string()
}

const AGENT_NAMES: [(&str, &str); 5] = [
    ("claude-code", "Claude Code"),
    ("claude", "Claude Code"),
    ("opencode", "OpenCode"),
    ("codex", "Codex"),
    ("gemini", "Gemini"),
];

/// Nombre que el usuario ve en Herdr, nunca la dirección interna («t51»).
///
/// `where` es «espacio / pestaña» («EzAgent / Claude Code» → «Claude Code de
/// EzAgent»); una pestaña numerada se nombra «pestaña 2 de EzAgent».
pub fn visible_name(item: &Map<String, Value>) -> String {
    let where_text = collapse_spaces(&text_or_empty(item.get("where")));
    let (mut space, mut tab) = (String::new(), String::new());
    if let Some((left, right)) = where_text.split_once(" / ") {
        space = py_strip(left).to_string();
        tab = py_strip(right).to_string();
    } else if !where_text.is_empty() {
        tab = where_text;
    }
    if space.is_empty() {
        space = collapse_spaces(&text_or_empty(item.get("space")));
    }
    if tab.is_empty() {
        tab = collapse_spaces(&text_or_empty(item.get("name")));
    }
    if !tab.is_empty() && tab.chars().all(char::is_numeric) {
        tab = format!("pestaña {tab}");
    }
    if !tab.is_empty() && !space.is_empty() {
        return format!("{tab} de {space}");
    }
    if !tab.is_empty() {
        return tab;
    }
    let mut agent = py_strip(&text_or_empty(item.get("agent"))).to_string();
    if let Some((_, label)) = AGENT_NAMES
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&agent))
    {
        agent = (*label).to_string();
    }
    let cwd = text_or_empty(item.get("cwd"));
    let folder = Path::new(&cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !agent.is_empty() && !folder.is_empty() {
        return format!("{agent} en {folder}");
    }
    if agent.is_empty() {
        "el agente".to_string()
    } else {
        agent
    }
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn plain_argument() -> &'static Regex {
    static PLAIN: OnceLock<Regex> = OnceLock::new();
    // DECISION: Python deja pasar un salto de línea final por el `$`; aquí se
    // cita de más, que solo agrega comillas seguras.
    compiled(&PLAIN, r"^[A-Za-z0-9_./:=@%+,\-]+$")
}

/// Línea que se escribe en PowerShell; cita lo que no sea un argumento simple.
///
/// `env` se antepone como `$env:NOMBRE='valor'; ` (solo vive en esa shell).
/// `path_prepend` añade carpetas al principio del PATH de esa shell.
///
/// DECISION: cuando el programa va citado se antepone el operador `&`:
/// en PowerShell `'C:\ruta\programa.exe'` a secas es una cadena, no una
/// orden, y el hijo muere sin arrancar (verificado en vivo el 19-nov).
pub(crate) fn command_line(
    argv: &[String],
    env: &Map<String, Value>,
    path_prepend: Option<&[String]>,
) -> String {
    let mut prefix = String::new();
    for (name, value) in env {
        prefix.push_str(&format!(
            "$env:{name}={}; ",
            ps_quote(&ez_core::pyvalue::python_str(value))
        ));
    }
    for folder in path_prepend.unwrap_or(&[]) {
        prefix.push_str(&format!(
            "$env:PATH={}+$env:PATH; ",
            ps_quote(&format!("{folder};"))
        ));
    }
    let parts: Vec<String> = argv
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if plain_argument().is_match(part) {
                part.clone()
            } else if index == 0 {
                // El programa citado necesita el operador de llamada.
                format!("& {}", ps_quote(part))
            } else {
                ps_quote(part)
            }
        })
        .collect();
    prefix + &parts.join(" ")
}

/// Teclas nombradas (enter, esc, ctrl+t...) o None si alguna no es válida.
pub(crate) fn valid_keys(keys: &[String]) -> Option<Vec<String>> {
    static KEY: OnceLock<Regex> = OnceLock::new();
    let pattern = compiled(
        &KEY,
        r"^(?:(?:ctrl|alt|shift)\+)*(?:[a-z0-9]|enter|esc|tab|up|down|left|right|home|end|pageup|pagedown|backspace|space|f(?:[1-9]|1[0-2]))$",
    );
    if keys.is_empty() || keys.len() > 12 {
        return None;
    }
    let cleaned: Vec<String> = keys
        .iter()
        .map(|key| py_strip(key).to_lowercase())
        .collect();
    if cleaned.iter().all(|key| pattern.is_match(key)) {
        Some(cleaned)
    } else {
        None
    }
}
