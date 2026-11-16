//! Clasificador de diálogos de permiso y lista de denegación (AUTO-A2, §9.4).
//!
//! Módulo puro: recibe la pantalla ya leída y los datos que el bucle ya
//! calculó (alcance de la terminal, carpeta de la tarea). No habla con Herdr
//! ni abre hilos.

use std::path::Path;
use std::sync::OnceLock;

use ez_core::pyvalue::{collapse_spaces, fold_accents, head_chars, py_splitlines, py_strip};
use ez_core::redact::redact;
use regex::Regex;

/// Lo que pide un diálogo de permiso.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DialogKind {
    /// Ejecutar un comando.
    Exec,
    /// Editar o escribir un archivo.
    Edit,
    /// Confiar en una carpeta (`_BLOCKING_DIALOG` del Python).
    Trust,
    /// Cualquier otro diálogo: se pregunta.
    Other,
    /// La pantalla no muestra un diálogo.
    #[default]
    None,
}

/// Diálogo leído de la pantalla de una terminal bloqueada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialog {
    pub kind: DialogKind,
    /// Comando pedido (`Exec`) o archivo destino (`Edit`).
    pub command: Option<String>,
    /// Carpeta del diálogo de confianza (`Trust`).
    pub folder: Option<String>,
    /// `(tecla, texto)` de las opciones numeradas o con flecha.
    pub options: Vec<(String, String)>,
    /// Tecla de la opción que aprueba **solo esta vez**, si es única.
    pub approve_once: Option<String>,
}

/// Qué hacer con el diálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Enviar esta tecla (`keys_agent(address, [tecla])`).
    Approve(String),
    /// Pausar el modo autónomo y preguntar al usuario, con este motivo.
    Ask(String),
    /// El diálogo ya no está en pantalla: no hay nada que hacer.
    Ignore,
}

/// Espera sin cambios de pantalla antes de aprobar (antirrebote de §9.4).
pub const APPROVE_SETTLE_S: f64 = 1.5;

/// Líneas de pantalla que forman la huella del antirrebote.
pub const FINGERPRINT_LINES: usize = 20;

/// Máximo de caracteres del extracto de pantalla que va a la auditoría.
pub const SCREEN_EXCERPT_CHARS: usize = 400;

const BOX_GLYPHS: [char; 6] = ['│', '┃', '|', '║', '▌', '▏'];

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

fn create_target() -> &'static Regex {
    static CREATE: OnceLock<Regex> = OnceLock::new();
    compile(
        &CREATE,
        r"(?i)(?:do you want to (?:create|edit|write|modify|overwrite)|would you like to edit)\s+(.+)$",
    )
}

fn arrow_edit_target() -> &'static Regex {
    static ARROW: OnceLock<Regex> = OnceLock::new();
    compile(&ARROW, r"(?i)^→\s*(?:edit|write)\s+(.+)$")
}

/// Línea sin espacios ni bordes de la caja del TUI.
fn clean_line(line: &str) -> &str {
    py_strip(line).trim_matches(|character: char| {
        character.is_whitespace() || BOX_GLYPHS.contains(&character)
    })
}

/// Tecla entre paréntesis al final de una opción (`Yes, proceed (y)`).
fn trailing_key(text: &str) -> Option<String> {
    let inner = text.strip_suffix(')')?;
    let start = inner.rfind('(')?;
    let key = py_strip(&inner[start + 1..]).to_lowercase();
    let single = key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
    let named = matches!(key.as_str(), "enter" | "esc" | "tab" | "space");
    (single || named).then_some(key)
}

/// Una opción numerada o una opción marcada con flecha.
///
/// DECISION: una opción solo con flecha no tiene tecla deducible (se elige
/// moviendo el cursor y pulsando Enter); su clave queda vacía y nunca se
/// aprueba con ella.
fn parse_option(line: &str) -> Option<(String, String)> {
    let line = clean_line(line);
    let (selected, rest) = match line
        .strip_prefix('❯')
        .or_else(|| line.strip_prefix('›'))
        .or_else(|| line.strip_prefix('>'))
    {
        Some(rest) => (true, py_strip(rest)),
        None => (false, line),
    };
    if let Some(captures) = numbered_option().captures(rest) {
        let number = captures.get(1)?.as_str().to_string();
        let text = py_strip(captures.get(2)?.as_str()).to_string();
        let key = trailing_key(&text).unwrap_or(number);
        return Some((key, text));
    }
    if selected && !rest.is_empty() {
        return Some((String::new(), rest.to_string()));
    }
    None
}

fn parse_options(lines: &[&str]) -> Vec<(String, String)> {
    lines.iter().filter_map(|line| parse_option(line)).collect()
}

/// Normaliza el texto de una opción para comparar palabras.
fn folded(text: &str) -> String {
    fold_accents(text)
}

fn positive_option(text: &str) -> bool {
    static POSITIVE: OnceLock<Regex> = OnceLock::new();
    let pattern = compile(
        &POSITIVE,
        r"(?:\byes\b|\bsi\b|\ballow\b|\bpermitir\b|\baprobar\b|\bpermite\b)",
    );
    pattern.is_match(&folded(text))
}

fn excluded_option(text: &str) -> bool {
    static EXCLUDED: OnceLock<Regex> = OnceLock::new();
    // DECISION: además de la lista de §9.4 se excluye «auto mode» / «modo
    // automático»: no es una aprobación de una sola vez y saltaría la lista
    // de denegación y la auditoría (§5).
    let pattern = compile(
        &EXCLUDED,
        r"(?:always|siempre|don'?t ask|dont ask|no volver|for this session|auto mode|modo automatico|\ball\b)",
    );
    pattern.is_match(&folded(text))
}

/// La única opción que aprueba solo esta vez, si la hay.
fn approve_once(options: &[(String, String)]) -> Option<String> {
    let mut found: Option<String> = None;
    for (key, text) in options {
        if key.is_empty() || !positive_option(text) || excluded_option(text) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(key.clone());
    }
    found
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
    let starter = first.is_ascii_lowercase()
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
        let option = numbered_option().captures(text).is_some()
            || text.starts_with('❯')
            || text.starts_with('›');
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
            if let Some(dangerous) = candidates.iter().find(|line| denied(line).is_some()) {
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

fn extract_edit_target(lines: &[&str]) -> Option<String> {
    for line in lines {
        let text = clean_line(line);
        if let Some(captures) = arrow_edit_target().captures(text) {
            return Some(py_strip(captures.get(1)?.as_str()).to_string());
        }
        if let Some(captures) = change_header().captures(text) {
            return Some(captures.get(1)?.as_str().to_string());
        }
        if let Some(captures) = create_target().captures(&folded(text)) {
            let target = py_strip(captures.get(1)?.as_str());
            let target = target.trim_end_matches(['?', '¿', '.']);
            if !target.is_empty() {
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
    let options = parse_options(&lines);
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
    let command = match kind {
        DialogKind::Exec => extract_exec_command(&lines),
        DialogKind::Edit => extract_edit_target(&lines),
        _ => None,
    };
    let folder = if kind == DialogKind::Trust {
        extract_folder(&lines)
    } else {
        None
    };
    Dialog {
        kind,
        command,
        folder,
        approve_once: approve_once(&options),
        options,
    }
}

fn deny_rules() -> &'static Vec<(Regex, &'static str)> {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    // shortcut: la lista compara texto literal; una ruta que resuelve a la
    // raíz con «..» (`rm -rf /tmp/..`) no se detecta. Ampliar si aparece.
    RULES.get_or_init(|| {
        [
            (
                r"(?i)\brm\s+(?:[^\n]*?\s)?(?:-{1,2}[a-z-]*r[a-z-]*)\b[^\n]*\s(?:/|/\*|~|~/|~/\*|\$HOME/?\*?|\$\{HOME\}/?\*?|%userprofile%(?:[\\/]\*)?|[a-z]:[\\/]\*?)\s*(?:$|[;&|])",
                "borra la raíz, el perfil del usuario o una unidad",
            ),
            (
                r"(?i)\b(?:remove-item|ri|rd|rmdir|del)\b[^\n]*(?:-recurse|/s\b|-r\b)[^\n]*\s(?:[a-z]:[\\/]\*?|%userprofile%(?:[\\/]\*)?)\s*(?:$|[;&|])",
                "borra la raíz de una unidad",
            ),
            (
                r"(?i)(?:\bformat-volume\b|(?:^|[;&|]\s*)(?:sudo\s+)?format(?:\.com|\.exe)?(?:\s|$))",
                "formatea un disco",
            ),
            (r"(?i)\bdiskpart(?:\.exe)?\b", "reparte o formatea discos"),
            (r"(?i)\bmkfs\b", "formatea un sistema de archivos"),
            (
                r"(?i)\bdd\b[^\n]*\bof=/dev/",
                "escribe directamente en un dispositivo",
            ),
            (
                r"(?i)\bgit\s+push\b(?:[^\n]*(?:--force|(?:^|\s)-f(?:\s|$))[^\n]*\b(?:main|master|release)\b|[^\n]*\b(?:main|master|release)\b[^\n]*(?:--force|(?:^|\s)-f(?:\s|$)))",
                "reescribe una rama principal con git push --force",
            ),
            (
                r"(?i)\|\s*(?:sudo\s+)?(?:sh|bash|iex)\b",
                "ejecuta un script descargado de internet",
            ),
            (
                r"(?i)\b(?:invoke-expression|iex)\b[^\n]*(?:https?://|\b(?:iwr|irm|curl|wget)\b)",
                "ejecuta un script descargado de internet",
            ),
            (
                r"(?i)\b(?:shutdown(?:\.exe)?|restart-computer|stop-computer)\b",
                "apaga o reinicia el equipo",
            ),
            (
                r#"(?i)\breg(?:\.exe)?\s+add\s+["']?hklm\b"#,
                "modifica el registro del sistema",
            ),
            (
                r#"(?:^|[\s"'/\\])(?:\.ssh\b|\.aws\b|\.env\b|credential)"#,
                "toca credenciales o secretos",
            ),
        ]
        .into_iter()
        .map(|(pattern, reason)| (Regex::new(pattern).expect("patrón fijo válido"), reason))
        .collect()
    })
}

/// Motivo por el que un comando está en la lista de denegación, si lo está.
pub fn denied(command: &str) -> Option<&'static str> {
    deny_rules()
        .iter()
        .find(|(pattern, _)| pattern.is_match(command))
        .map(|(_, reason)| *reason)
}

fn normalize_folder(path: &str) -> String {
    let text = path.trim().replace('\\', "/");
    let text = text.trim_end_matches('/').to_string();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// `folder` es la de la tarea o una carpeta dentro de ella.
fn folder_within(folder: &str, task_folder: &str) -> bool {
    let folder = normalize_folder(folder);
    let task = normalize_folder(task_folder);
    !task.is_empty() && (folder == task || folder.starts_with(&format!("{task}/")))
}

fn ask(motivo: &str) -> Decision {
    Decision::Ask(motivo.to_string())
}

fn decide_exec(dialog: &Dialog) -> Decision {
    if let Some(command) = dialog.command.as_deref() {
        if let Some(reason) = denied(command) {
            return ask(reason);
        }
    } else if dialog.kind == DialogKind::Exec {
        return ask("no pude leer el comando del diálogo");
    }
    match dialog.approve_once.as_deref() {
        Some(key) if !key.is_empty() => Decision::Approve(key.to_string()),
        _ => ask("no pude deducir una opción de permiso para esta vez"),
    }
}

fn decide_trust(dialog: &Dialog, task_folder: Option<&Path>) -> Decision {
    let Some(folder) = dialog.folder.as_deref() else {
        return ask("no pude leer la carpeta del diálogo");
    };
    let Some(task) = task_folder.and_then(Path::to_str) else {
        return ask("no hay carpeta de la tarea para comparar la confianza");
    };
    if !folder_within(folder, task) {
        return ask("la carpeta del diálogo no es la de la tarea");
    }
    match dialog.approve_once.as_deref() {
        Some(key) if !key.is_empty() => Decision::Approve(key.to_string()),
        _ => ask("no pude deducir una opción de permiso para esta vez"),
    }
}

/// Aplica las reglas de «Aprobar» de §9.4 a un diálogo ya clasificado.
///
/// `in_scope` dice si la terminal está en el alcance de la tarea; el
/// antirrebote lo resuelve el bucle con [`screen_fingerprint`] y
/// [`APPROVE_SETTLE_S`].
pub fn decide(dialog: &Dialog, in_scope: bool, task_folder: Option<&Path>) -> Decision {
    match dialog.kind {
        DialogKind::None => Decision::Ignore,
        _ if !in_scope => ask("la terminal no está en el alcance de la tarea"),
        DialogKind::Exec | DialogKind::Edit => decide_exec(dialog),
        DialogKind::Trust => decide_trust(dialog, task_folder),
        DialogKind::Other => ask("diálogo de permiso no reconocido"),
    }
}

/// Huella de las últimas líneas de pantalla para no aprobar dos veces.
///
/// DECISION: se ignoran las líneas vacías para que el relleno del TUI no
/// cambie la huella.
pub fn screen_fingerprint(screen: &str) -> String {
    let lines: Vec<String> = py_splitlines(screen)
        .into_iter()
        .map(py_strip)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let start = lines.len().saturating_sub(FINGERPRINT_LINES);
    lines[start..].join("\n")
}

/// Extracto de la pantalla para la auditoría: una línea, sin secretos y
/// como máximo [`SCREEN_EXCERPT_CHARS`] caracteres.
pub fn screen_excerpt(screen: &str) -> String {
    let flat = collapse_spaces(&redact(screen));
    head_chars(&flat, SCREEN_EXCERPT_CHARS).to_string()
}

#[cfg(test)]
mod tests;
