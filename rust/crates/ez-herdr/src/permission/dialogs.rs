//! Lectura de la pantalla: tipo de diálogo, comando, ruta, carpeta y la
//! secuencia de teclas que aprueba una sola vez (§10.7, §10.8).

use std::sync::OnceLock;

use ez_core::pyvalue::{fold_accents, py_splitlines, py_strip};
use regex::Regex;

use super::{Dialog, DialogKind};

const BOX_GLYPHS: [char; 6] = ['│', '┃', '|', '║', '▌', '▏'];
const MAX_ARROW_OPTIONS: usize = 8;
const MAX_OPTION_CHARS: usize = 100;

fn compile(slot: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    slot.get_or_init(|| Regex::new(pattern).expect("patrón fijo válido"))
}

fn trust_marker() -> &'static Regex {
    static TRUST: OnceLock<Regex> = OnceLock::new();
    compile(
        &TRUST,
        r"(?:do you trust|trust this (?:folder|directory|project|workspace)|yes, i trust|quick safety check|confias en|confiar en (?:esta|este) (?:carpeta|directorio|proyecto|espacio))",
    )
}

fn edit_marker() -> &'static Regex {
    static EDIT: OnceLock<Regex> = OnceLock::new();
    compile(
        &EDIT,
        r"(?m)(?:would you like to make the following edits|do you want to (?:create|edit|write|modify|overwrite)\b|do you want to make this edit|apply (?:the following )?changes\?|make this edit|aplicar (?:los|estos) cambios|editar (?:el|este) archivo|modificar (?:el|este) archivo|crear (?:el|este) archivo|escribir (?:el|este) archivo|permiso de (?:edicion|escritura)|^[│┃|║\s]*→\s*(?:edit|write)\b)",
    )
}

fn exec_marker() -> &'static Regex {
    static EXEC: OnceLock<Regex> = OnceLock::new();
    compile(
        &EXEC,
        r"(?:would you like to run the following command|allow command\?|do you want to proceed|run (?:this|the following|shell) command|command requires approval|bash command|shell command|ejecutar (?:el|este) (?:siguiente )?comando|permitir ejecutar|permiso de ejecucion|deseas ejecutar|desea ejecutar)",
    )
}

fn permission_header() -> &'static Regex {
    static HEADER: OnceLock<Regex> = OnceLock::new();
    compile(
        &HEADER,
        r"(?:permission (?:required|needed)|permiso (?:requerido|necesario)|approval required)",
    )
}

fn permission_word() -> &'static Regex {
    static WORD: OnceLock<Regex> = OnceLock::new();
    compile(
        &WORD,
        r"(?:\bpermission\b|\bpermiso\b|\ballow\b|\bpermitir\b|\bapprove\b|\bapproval\b|\baprobar\b|\baprobacion\b|\bconfirm\b|\bconfirma\b|\btrust\b|\bconfiar\b)",
    )
}

fn numbered_option() -> &'static Regex {
    static NUMBERED: OnceLock<Regex> = OnceLock::new();
    compile(&NUMBERED, r"^(\d{1,2})[.)]\s+(.+)$")
}

fn change_header() -> &'static Regex {
    static CHANGE: OnceLock<Regex> = OnceLock::new();
    compile(&CHANGE, r"^(\S+)\s+\(\+\d+ -\d+\)$")
}

fn edit_target() -> &'static Regex {
    static TARGET: OnceLock<Regex> = OnceLock::new();
    compile(
        &TARGET,
        r"(?i)^(?:do you want to (?:create|edit|write|modify|overwrite)|would you like to (?:edit|write|create))\s+(.+)$",
    )
}

fn arrow_edit_target() -> &'static Regex {
    static ARROW: OnceLock<Regex> = OnceLock::new();
    compile(&ARROW, r"(?i)^→\s*(?:edit|write)\s+(.+)$")
}

fn positive_word() -> &'static Regex {
    static POSITIVE: OnceLock<Regex> = OnceLock::new();
    compile(
        &POSITIVE,
        r"\b(?:yes|si|allow|permitir|permite|permita|aprobar|aprueba|apruebo)\b",
    )
}

/// Concesión permanente del filtro de §10.7, por palabra completa.
fn permanent_word() -> &'static Regex {
    static PERMANENT: OnceLock<Regex> = OnceLock::new();
    compile(
        &PERMANENT,
        r"\b(?:always|siempre|tod[oa]s?|all|session|sesi[oó]n)\b|don'?t ask|no (?:vuelvas a |volver a )?preguntar",
    )
}

fn auto_mode_word() -> &'static Regex {
    static AUTO: OnceLock<Regex> = OnceLock::new();
    compile(&AUTO, r"auto mode|modo automatico")
}

fn asks_enter_hint() -> &'static Regex {
    static HINT: OnceLock<Regex> = OnceLock::new();
    compile(
        &HINT,
        r"(?:press(?:ing)?\s+enter\s+(?:to|para)\s+confirm|enter\s+to\s+confirm|enter\s+confirm|pulsa(?:r)?\s+enter|presiona(?:r)?\s+enter)",
    )
}

fn arrows() -> &'static Regex {
    static ARROWS: OnceLock<Regex> = OnceLock::new();
    compile(&ARROWS, r"^(?:\s*[>›❯])")
}

/// Línea sin espacios ni bordes de la caja del TUI.
pub(super) fn clean_line(line: &str) -> &str {
    py_strip(line).trim_matches(|character: char| {
        character.is_whitespace() || BOX_GLYPHS.contains(&character)
    })
}

fn folded(text: &str) -> String {
    fold_accents(text)
}

/// Una opción leída del menú, con lo necesario para deducir su secuencia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedOption {
    pub(super) number: Option<String>,
    pub(super) shortcut: Option<String>,
    pub(super) text: String,
    pub(super) selected: bool,
}

impl ParsedOption {
    /// Tecla directa de la opción (atajo o número); vacía si solo se llega con flechas.
    pub(super) fn key(&self) -> String {
        self.shortcut
            .clone()
            .or_else(|| self.number.clone())
            .unwrap_or_default()
    }
}

/// Tecla entre paréntesis al final de una opción (`Yes, proceed (y)`).
pub(super) fn trailing_key(text: &str) -> Option<String> {
    let inner = text.strip_suffix(')')?;
    let start = inner.rfind('(')?;
    let key = py_strip(&inner[start + 1..]).to_lowercase();
    let single = key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
    let named = matches!(
        key.as_str(),
        "enter" | "esc" | "tab" | "space" | "up" | "down"
    );
    (single || named).then_some(key)
}

/// Marcador de la opción resaltada (`❯`, `›` o `>`).
fn strip_marker(line: &str) -> (bool, &str) {
    match line
        .strip_prefix('❯')
        .or_else(|| line.strip_prefix('›'))
        .or_else(|| line.strip_prefix('>'))
    {
        Some(rest) => (true, py_strip(rest)),
        None => (false, line),
    }
}

/// Una opción numerada o marcada con flecha.
pub(super) fn parse_option(line: &str) -> Option<ParsedOption> {
    let line = clean_line(line);
    let (selected, rest) = strip_marker(line);
    if let Some(captures) = numbered_option().captures(rest) {
        let number = captures.get(1)?.as_str().to_string();
        let text = py_strip(captures.get(2)?.as_str()).to_string();
        return Some(ParsedOption {
            number: Some(number),
            shortcut: trailing_key(&text),
            text,
            selected,
        });
    }
    if selected && !rest.is_empty() {
        return Some(ParsedOption {
            number: None,
            shortcut: None,
            text: rest.to_string(),
            selected: true,
        });
    }
    None
}

fn is_hint(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.starts_with("esc to")
        || lower.starts_with("press ")
        || lower.starts_with("pulsa")
        || lower.starts_with("presiona")
        || lower.starts_with("tip:")
        || lower.starts_with("ctrl+")
        || lower.starts_with("tab to")
        || lower.starts_with("shift+")
        || lower.starts_with('⇆')
        || lower.starts_with('←')
        || lower.contains("enter confirm")
}

/// ¿Parece una opción sin número ni flecha que sigue a la resaltada?
fn is_plain_option(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_alphanumeric() {
        return false;
    }
    if text.chars().count() > MAX_OPTION_CHARS {
        return false;
    }
    if text.ends_with([':', '?', '.']) {
        return false;
    }
    let (marked, _) = strip_marker(text);
    !marked && numbered_option().captures(text).is_none()
}

/// Opciones del diálogo. Tras una opción solo con flecha se absorben las
/// líneas siguientes: los menús sin números resaltan una sola y listan el resto.
pub(super) fn parse_options(lines: &[&str]) -> Vec<ParsedOption> {
    let mut options = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(option) = parse_option(lines[index]) else {
            index += 1;
            continue;
        };
        let arrow_only = option.number.is_none();
        options.push(option);
        index += 1;
        if !arrow_only {
            continue;
        }
        let mut absorbed = 0;
        while index < lines.len() && absorbed < MAX_ARROW_OPTIONS {
            let text = clean_line(lines[index]);
            if text.is_empty() || is_hint(text) || !is_plain_option(text) {
                break;
            }
            options.push(ParsedOption {
                number: None,
                shortcut: None,
                text: text.to_string(),
                selected: false,
            });
            absorbed += 1;
            index += 1;
        }
    }
    options
}

pub(super) fn positive_option(text: &str) -> bool {
    positive_word().is_match(&folded(text))
}

/// Opción que concede permiso más allá de esta vez (o salta la auditoría).
pub(super) fn permanent_filter(text: &str) -> bool {
    let text = folded(text);
    permanent_word().is_match(&text) || auto_mode_word().is_match(&text)
}

/// ¿Hay alguna opción permanente en el menú? (para el motivo de §10.7)
pub(super) fn has_permanent(options: &[(String, String)]) -> bool {
    options
        .iter()
        .any(|(_, text)| permanent_word().is_match(&folded(text)))
}

fn asks_enter(screen: &str) -> bool {
    asks_enter_hint().is_match(&folded(screen))
}

fn sequence_for(parsed: &[ParsedOption], target: usize, extra_enter: bool) -> Vec<String> {
    let option = &parsed[target];
    if let Some(shortcut) = &option.shortcut {
        return vec![shortcut.clone()];
    }
    if let Some(number) = &option.number {
        let mut sequence = vec![number.clone()];
        if extra_enter {
            sequence.push("enter".to_string());
        }
        return sequence;
    }
    // shortcut: solo se cuentan las opciones visibles; un menú sin números que
    // resalta una opción de la mitad de la lista podría necesitar una flecha más.
    // El bucle lo detecta en S3 (el diálogo seguiría en pantalla) y pausa.
    let from = parsed
        .iter()
        .enumerate()
        .filter(|(_, option)| option.selected)
        .map(|(index, _)| index)
        .min_by_key(|index| index.abs_diff(target))
        .unwrap_or(0);
    let mut sequence = Vec::new();
    if target >= from {
        sequence.extend(std::iter::repeat_n("down".to_string(), target - from));
    } else {
        sequence.extend(std::iter::repeat_n("up".to_string(), from - target));
    }
    sequence.push("enter".to_string());
    sequence
}

/// Secuencia que aprueba solo esta vez, si hay una única opción válida.
pub(super) fn approve_sequence(
    parsed: &[ParsedOption],
    screen: &str,
    allow_permanent: bool,
) -> Option<Vec<String>> {
    let candidates: Vec<usize> = parsed
        .iter()
        .enumerate()
        .filter(|(_, option)| positive_option(&option.text))
        .filter(|(_, option)| allow_permanent || !permanent_filter(&option.text))
        .map(|(index, _)| index)
        .collect();
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(sequence_for(parsed, *target, asks_enter(screen)))
}

/// Primera línea con forma de ruta absoluta (carpeta del diálogo de confianza).
fn extract_folder(lines: &[&str]) -> Option<String> {
    for line in lines {
        let text = clean_line(line);
        let candidate = text.trim_matches(|character: char| {
            character == '"'
                || character == '\''
                || character == '`'
                || character == '«'
                || character == '»'
        });
        let drive = candidate.len() > 2
            && candidate.as_bytes()[0].is_ascii_alphabetic()
            && candidate.as_bytes()[1] == b':'
            && matches!(candidate.as_bytes()[2], b'\\' | b'/');
        let unix = candidate.starts_with('/') && !candidate.starts_with("//");
        let home = candidate.starts_with("~/") || candidate == "~";
        if (drive || unix || home) && !candidate.contains('?') {
            return Some(candidate.trim_end_matches(['/', '\\']).to_string());
        }
    }
    None
}

fn looks_like_command(text: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    let drive = text.len() > 1 && text.as_bytes()[1] == b':';
    let starter = first.is_ascii_lowercase()
        || drive
        || matches!(first, '.' | '/' | '~' | '$' | '"' | '\'' | '(' | '\\');
    starter && !text.ends_with('.')
}

/// Líneas que pueden ser el comando del diálogo, tras la cabecera.
fn command_candidates<'a>(lines: &[&'a str], start: usize) -> Vec<&'a str> {
    let mut candidates = Vec::new();
    for line in &lines[start..] {
        let text = clean_line(line);
        if text.is_empty() || text.to_lowercase().starts_with("tip:") {
            continue;
        }
        let option = numbered_option().captures(text).is_some() || arrows().is_match(text);
        if option || candidates.len() >= 10 {
            break;
        }
        candidates.push(text);
    }
    candidates
}

fn extract_exec_command(lines: &[&str]) -> Option<String> {
    for (index, line) in lines.iter().enumerate() {
        let text = clean_line(line);
        if let Some(command) = text.strip_prefix("$ ") {
            let command = py_strip(command);
            if !command.is_empty() {
                return Some(command.to_string());
            }
            continue;
        }
        let header = text.to_lowercase();
        let header = header.trim_end_matches(':');
        if matches!(
            header,
            "bash command" | "shell command" | "powershell command" | "cmd command"
        ) {
            let candidates = command_candidates(lines, index + 1);
            // Se prefiere la línea que la lista de denegación reconoce para
            // que un formato distinto del TUI no oculte un comando peligroso.
            if let Some(dangerous) = candidates.iter().find(|line| super::denied(line).is_some()) {
                return Some((*dangerous).to_string());
            }
            return candidates
                .iter()
                .find(|line| looks_like_command(line))
                .map(|line| (*line).to_string());
        }
    }
    None
}

fn plausible_path(text: &str) -> bool {
    !text.is_empty()
        && (!text.contains(char::is_whitespace)
            || text.contains(['/', '\\'])
            || text.ends_with(['.', ')']))
}

fn extract_edit_path(lines: &[&str]) -> Option<String> {
    for line in lines {
        let text = clean_line(line);
        if let Some(captures) = arrow_edit_target().captures(text) {
            let target = py_strip(captures.get(1)?.as_str());
            if plausible_path(target) {
                return Some(target.to_string());
            }
        }
        if let Some(captures) = change_header().captures(text) {
            return Some(captures.get(1)?.as_str().to_string());
        }
        if let Some(captures) = edit_target().captures(text) {
            let target = py_strip(captures.get(1)?.as_str());
            let target = target.trim_end_matches(['?', '¿', '.']);
            if plausible_path(target) {
                return Some(target.to_string());
            }
        }
    }
    None
}

fn has_dollar_command(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let text = clean_line(line);
        text.strip_prefix("$ ")
            .is_some_and(|rest| !py_strip(rest).is_empty())
    })
}

/// Clasifica el diálogo que muestra la pantalla.
pub fn classify(screen: &str) -> Dialog {
    let lines: Vec<&str> = py_splitlines(screen).into_iter().collect();
    let text = folded(screen);
    let parsed = parse_options(&lines);
    let options: Vec<(String, String)> = parsed
        .iter()
        .map(|option| (option.key(), option.text.clone()))
        .collect();
    let kind = if trust_marker().is_match(&text) {
        DialogKind::Trust
    } else if edit_marker().is_match(&text) {
        DialogKind::Edit
    } else if exec_marker().is_match(&text)
        || (permission_header().is_match(&text) && has_dollar_command(&lines))
    {
        DialogKind::Exec
    } else if permission_header().is_match(&text)
        || (!options.is_empty() && permission_word().is_match(&text))
    {
        DialogKind::Other
    } else {
        DialogKind::None
    };
    let command = if kind == DialogKind::Exec {
        extract_exec_command(&lines)
    } else {
        None
    };
    let path = if kind == DialogKind::Edit {
        extract_edit_path(&lines)
    } else {
        None
    };
    let folder = if kind == DialogKind::Trust {
        extract_folder(&lines)
    } else {
        None
    };
    let approve_once = if matches!(
        kind,
        DialogKind::Exec | DialogKind::Edit | DialogKind::Trust
    ) {
        // En confiar en una carpeta la concesión es permanente por naturaleza;
        // §10.2 la limita a la carpeta de trabajo, así que no se filtra aquí.
        approve_sequence(&parsed, screen, kind == DialogKind::Trust)
    } else {
        None
    };
    Dialog {
        kind,
        command,
        path,
        folder,
        options,
        approve_once,
    }
}
