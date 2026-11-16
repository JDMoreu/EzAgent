//! Port of `src/ezagent/agent/paths.py` (F2-04).
//!
//! Toda herramienta resuelve, valida y opera sobre la ruta devuelta por
//! [`resolve_safe`] (nunca sobre la cadena original).

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};

/// Barra invertida, para evitar escapes en el código (`paths.BS`).
pub const BS: char = '\\';

/// Nombres de dispositivo reservados de Windows (`paths.RESERVED`).
pub const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// La ruta no cumple la política de acceso (`paths.PathDenied`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDenied {
    message: String,
}

impl PathDenied {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }

    /// Texto que ve el usuario y que compara la paridad.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PathDenied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PathDenied {}

/// Variables de entorno de `expandvars`/`expanduser` (en Python, `os.environ`).
#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: Vec<(String, String)>,
}

impl Env {
    /// Las del proceso actual.
    pub fn process() -> Self {
        Self {
            vars: std::env::vars_os()
                .map(|(key, value)| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
        }
    }

    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            vars: pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    /// `os.environ[name]`: en Windows la comparación ignora mayúsculas.
    fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(key, _)| same_env_name(key, name))
            .map(|(_, value)| value.as_str())
    }
}

fn same_env_name(key: &str, name: &str) -> bool {
    if cfg!(windows) {
        key.to_uppercase() == name.to_uppercase()
    } else {
        key == name
    }
}

/// `Path(os.path.expandvars(os.path.expanduser(value)))`.
pub fn expand(value: &str) -> PathBuf {
    PathBuf::from(expand_with(&Env::process(), value))
}

/// Igual que [`expand`], con un entorno dado.
pub fn expand_with(env: &Env, value: &str) -> String {
    expand_vars(env, &expand_user(env, value))
}

fn expand_user(env: &Env, value: &str) -> String {
    let Some(rest) = value.strip_prefix('~') else {
        return value.to_string();
    };
    if !(rest.is_empty() || rest.starts_with('/') || rest.starts_with(BS)) {
        // shortcut: `~usuario` y `~+` quedan sin expandir; ampliar si un perfil los usa.
        return value.to_string();
    }
    let Some(home) = user_home(env) else {
        return value.to_string();
    };
    format!("{home}{rest}")
}

#[cfg(windows)]
fn user_home(env: &Env) -> Option<String> {
    if let Some(home) = env.var("USERPROFILE").filter(|text| !text.is_empty()) {
        return Some(home.to_string());
    }
    match (env.var("HOMEDRIVE"), env.var("HOMEPATH")) {
        (Some(drive), Some(path)) => Some(format!("{drive}{path}")),
        _ => None,
    }
}

#[cfg(not(windows))]
fn user_home(env: &Env) -> Option<String> {
    env.var("HOME")
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn var_pattern() -> &'static Regex {
    static SLOT: OnceLock<Regex> = OnceLock::new();
    SLOT.get_or_init(|| {
        Regex::new(r"\$(\w+)|\$\{([^}]*)\}|%([^%]*)%").expect("patrón interno válido")
    })
}

fn expand_vars(env: &Env, value: &str) -> String {
    if !value.contains('$') && !value.contains('%') {
        return value.to_string();
    }
    var_pattern()
        .replace_all(value, |captures: &Captures| {
            let name = captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .map(|item| item.as_str())
                .unwrap_or_default();
            match env.var(name) {
                Some(replacement) => replacement.to_string(),
                None => captures
                    .get(0)
                    .map(|item| item.as_str().to_string())
                    .unwrap_or_default(),
            }
        })
        .into_owned()
}

/// `resolve_safe` con el entorno del proceso.
pub fn resolve_safe(
    raw: &str,
    base: &Path,
    roots: &[impl AsRef<str>],
    deny: &[impl AsRef<str>],
    deny_globs: &[impl AsRef<str>],
) -> Result<PathBuf, PathDenied> {
    resolve_safe_with(&Env::process(), raw, base, roots, deny, deny_globs)
}

/// `resolve_safe` con un entorno inyectable: las pruebas de paridad lo fijan.
pub fn resolve_safe_with(
    env: &Env,
    raw: &str,
    base: &Path,
    roots: &[impl AsRef<str>],
    deny: &[impl AsRef<str>],
    deny_globs: &[impl AsRef<str>],
) -> Result<PathBuf, PathDenied> {
    let trimmed = check_raw(raw)?;
    let expanded = expand_with(env, trimmed);
    let mut path = PathBuf::from(&expanded);
    if expanded.is_empty() {
        path = base.to_path_buf();
    } else if !path.is_absolute() {
        // DECISION: las rutas relativas se resuelven contra el workspace.
        path = base.join(&path);
    }
    let resolved = resolve(&path);
    check_reserved(&resolved)?;
    check_allowed(env, &resolved, roots)?;
    check_denied_folders(env, &resolved, deny)?;
    check_denied_globs(&resolved, deny_globs)?;
    if is_anchor(&resolved) {
        return Err(PathDenied::new("raíz de unidad no permitida"));
    }
    Ok(resolved)
}

fn check_raw(raw: &str) -> Result<&str, PathDenied> {
    if raw.is_empty() || raw.contains('\0') {
        return Err(PathDenied::new("ruta vacía o inválida"));
    }
    let trimmed = raw.trim_matches(is_python_space);
    if trimmed.starts_with(&format!("{BS}{BS}")) || trimmed.starts_with("//") {
        return Err(PathDenied::new("rutas UNC o de dispositivo no permitidas"));
    }
    if trimmed.chars().skip(2).any(|character| character == ':') {
        return Err(PathDenied::new(
            "flujos de datos alternativos no permitidos",
        ));
    }
    Ok(trimmed)
}

/// `str.strip()` de Python: también cuenta `\x1c`..`\x1f`, que Rust no.
fn is_python_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

fn check_reserved(path: &Path) -> Result<(), PathDenied> {
    for part in path.components() {
        let Component::Normal(name) = part else {
            continue;
        };
        if is_reserved(&name.to_string_lossy()) {
            return Err(PathDenied::new("nombre reservado de Windows"));
        }
    }
    Ok(())
}

fn is_reserved(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_uppercase();
    RESERVED.contains(&stem.as_str())
}

fn check_allowed(env: &Env, path: &Path, roots: &[impl AsRef<str>]) -> Result<(), PathDenied> {
    let allowed = roots
        .iter()
        .any(|root| is_within(path, &expanded_path(env, root.as_ref())));
    if allowed {
        Ok(())
    } else {
        Err(PathDenied::new("fuera de las carpetas permitidas"))
    }
}

fn check_denied_folders(
    env: &Env,
    path: &Path,
    deny: &[impl AsRef<str>],
) -> Result<(), PathDenied> {
    for entry in deny {
        if is_within(path, &expanded_path(env, entry.as_ref())) {
            return Err(PathDenied::new("carpeta protegida"));
        }
    }
    Ok(())
}

fn check_denied_globs(path: &Path, patterns: &[impl AsRef<str>]) -> Result<(), PathDenied> {
    let posix = path.to_string_lossy().replace(BS, "/");
    let name = path
        .file_name()
        .map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_default();
    for pattern in patterns {
        let pattern = pattern.as_ref();
        let last = pattern.rsplit('/').next().unwrap_or(pattern);
        if fnmatch(&posix, pattern) || fnmatch(&name, last) {
            // DECISION: fnmatch simple, como Python.
            return Err(PathDenied::new("archivo protegido"));
        }
    }
    Ok(())
}

fn expanded_path(env: &Env, value: &str) -> PathBuf {
    resolve(Path::new(&expand_with(env, value)))
}

/// `Path.resolve(strict=False)`: resuelve contra el disco la parte que existe
/// (enlaces y junctions incluidos) y conserva tal cual la que no existe. Es lo
/// que impide que `C:\Users\All Users` pase por `C:\Users` camino de `C:\ProgramData`.
pub fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut resolved = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::Prefix(_) | Component::RootDir => resolved.push(part.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
                canonicalize_current(&mut resolved);
            }
            Component::Normal(name) => {
                resolved.push(name);
                canonicalize_current(&mut resolved);
            }
        }
    }
    resolved
}

/// Sustituye la ruta por su destino real cuando el componente ya existe.
fn canonicalize_current(path: &mut PathBuf) {
    if let Ok(real) = std::fs::canonicalize(&*path) {
        *path = without_verbatim_prefix(&real);
    }
}

/// `\\?\C:\x` → `C:\x`; `\\?\UNC\server\share` → `\\server\share`. Los
/// fixtures comparan contra la forma normal, no contra la verbatim de Windows.
fn without_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

/// `rp == base or rp.is_relative_to(base)`; en Windows sin distinguir mayúsculas.
fn is_within(candidate: &Path, base: &Path) -> bool {
    let mut rest = candidate.components();
    for part in base.components() {
        match rest.next() {
            Some(other) if same_part(&other, &part) => {}
            _ => return false,
        }
    }
    true
}

fn same_part(left: &Component<'_>, right: &Component<'_>) -> bool {
    normcase(&left.as_os_str().to_string_lossy()) == normcase(&right.as_os_str().to_string_lossy())
}

/// `os.path.normcase`: minúsculas y `/` a `\` en Windows.
fn normcase(text: &str) -> String {
    if cfg!(windows) {
        text.replace('/', &BS.to_string()).to_lowercase()
    } else {
        text.to_string()
    }
}

fn is_anchor(path: &Path) -> bool {
    let mut parts = path.components();
    match parts.next() {
        Some(Component::Prefix(_)) | Some(Component::RootDir) => {
            !parts.any(|part| matches!(part, Component::Normal(_)))
        }
        _ => false,
    }
}

/// `fnmatch.fnmatch`: normaliza mayúsculas y compara (`fnmatchcase`).
pub fn fnmatch(name: &str, pattern: &str) -> bool {
    let name = normcase(name);
    let pattern = normcase(pattern);
    if !pattern
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
    {
        return name == pattern;
    }
    // shortcut: un patrón que el motor no acepte no coincide; `re.error` de
    // Python no tiene caso en los fixtures.
    Regex::new(&format!(r"\A{}", translate(&pattern)))
        .is_ok_and(|compiled| compiled.is_match(&name))
}

/// `fnmatch.translate` de CPython 3.12 línea por línea: `(?s:…)`, `[!…]`, `]`
/// como primer carácter, escapes dentro de la clase y rangos invertidos.
///
/// El `\A` (el `match` de Python) lo pone [`fnmatch`].
fn translate(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    let mut previous_star = false;
    while let Some(&character) = chars.get(index) {
        index += 1;
        if character == '*' {
            // Los `*` consecutivos se comprimen en uno; Python los une con
            // `.*?`, que solo cambia el trabajo del motor, no si encaja.
            if !previous_star {
                out.push_str(".*");
            }
        } else if character == '?' {
            out.push('.');
        } else if character == '[' {
            if let Some(close) = class_close(&chars, index) {
                out.push_str(&class_regex(&chars, index, close));
                index = close + 1;
            } else {
                out.push_str(r"\[");
            }
        } else {
            out.push_str(&regex::escape(&character.to_string()));
        }
        previous_star = character == '*';
    }
    format!(r"(?s:{out})\z")
}

/// Índice del `]` que cierra la clase; `None` si el patrón la deja abierta.
fn class_close(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start;
    if chars.get(index) == Some(&'!') {
        index += 1;
    }
    if chars.get(index) == Some(&']') {
        index += 1;
    }
    while index < chars.len() && chars[index] != ']' {
        index += 1;
    }
    (index < chars.len()).then_some(index)
}

/// Cuerpo de la clase (`[!a-c]` → `!a-c`) con los rangos invertidos recortados.
fn class_body(chars: &[char], start: usize, close: usize) -> String {
    let raw: String = chars[start..close].iter().collect();
    if !raw.contains('-') {
        return raw.replace('\\', r"\\");
    }
    let mut chunks: Vec<Vec<char>> = Vec::new();
    // Python busca `-` a partir del segundo carácter si la clase niega.
    let (mut from, mut search) = (start, start + 1 + usize::from(chars[start] == '!'));
    while search < close {
        if chars[search] != '-' {
            search += 1;
            continue;
        }
        chunks.push(chars[from..search].to_vec());
        from = search + 1;
        search += 3;
    }
    let tail: String = chars[from..close].iter().collect();
    match (tail.is_empty(), chunks.last_mut()) {
        (true, Some(last)) => last.push('-'),
        _ => chunks.push(tail.chars().collect()),
    }
    // Un rango invertido (`[z-a]`) no es válido en una expresión regular.
    let mut index = chunks.len();
    while index > 1 {
        index -= 1;
        if chunks[index - 1].last() > chunks[index].first() {
            let mut merged = std::mem::take(&mut chunks[index - 1]);
            merged.pop();
            merged.extend(chunks[index].iter().skip(1));
            chunks[index - 1] = merged;
            chunks.remove(index);
        }
    }
    chunks
        .iter()
        .map(|chunk| {
            chunk
                .iter()
                .collect::<String>()
                .replace('\\', r"\\")
                .replace('-', r"\-")
        })
        .collect::<Vec<String>>()
        .join("-")
}

/// Clase `[...]` lista para el motor: negación, `]` inicial y escapes.
fn class_regex(chars: &[char], start: usize, close: usize) -> String {
    let mut stuff = class_body(chars, start, close);
    for character in ['&', '~', '|'] {
        stuff = stuff.replace(character, &format!("\\{character}"));
    }
    if stuff.is_empty() {
        // Python emite `(?!)`, un lookahead; `\b\B` tampoco coincide nunca.
        return r"\b\B".to_string();
    }
    if stuff == "!" {
        // Rango negado vacío: cualquier carácter.
        return ".".to_string();
    }
    let (negated, body) = match stuff.strip_prefix('!') {
        Some(rest) => (true, rest.to_string()),
        None if stuff.starts_with('^') || stuff.starts_with('[') => (false, format!("\\{stuff}")),
        None => (false, stuff),
    };
    // Rust no acepta `]` como primer carácter de la clase; Python sí (`[]a]`).
    let body = if body.starts_with(']') {
        format!("\\{body}")
    } else {
        body
    };
    if negated {
        format!("[^{body}]")
    } else {
        format!("[{body}]")
    }
}

#[cfg(test)]
mod tests;
