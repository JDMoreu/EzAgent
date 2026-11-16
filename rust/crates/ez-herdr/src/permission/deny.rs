//! Lista de denegación de comandos y rutas, y comprobación de carpetas
//! (§9.4 ampliada en §10.8).

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use ez_core::paths;
use ez_core::pyvalue::py_split;
use regex::Regex;

const CREDENTIALS: &str = "toca credenciales o secretos";

fn rules() -> &'static [(Regex, &'static str)] {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    // shortcut: la lista compara texto literal; una ruta que resuelve a la
    // raíz con «..» o entre comillas no se detecta aquí. Los borrados
    // recursivos se revisan además contra las carpetas permitidas.
    RULES.get_or_init(|| {
        let table: [(&str, &str); 24] = [
            (
                r#"(?i)\brm\s+(?:[^\n]*?\s)?(?:-{1,2}[a-z-]*r[a-z-]*)\b[^\n]*\s["']?(?:/|/\*|~|~/|~/\*|\$HOME/?\*?|\$\{HOME\}/?\*?|%userprofile%(?:[\\/]\*)?|[a-z]:(?:[\\/]\*?)?)["']?\s*(?:$|[;&|])"#,
                "borra la raíz, el perfil del usuario o una unidad",
            ),
            (
                r#"(?i)\b(?:remove-item|ri|rd|rmdir|del)\b[^\n]*(?:-recurse|/s\b|-r\b)[^\n]*\s["']?(?:[a-z]:(?:[\\/]\*?)?|%userprofile%(?:[\\/]\*)?)["']?\s*(?:$|[;&|])"#,
                "borra la raíz de una unidad",
            ),
            (
                r#"(?i)\b(?:rm|rd|rmdir|del|remove-item|ri)\b[^\n]*\.git(?:[\\/][^\s"']*)?\s*(?:$|[;&|])"#,
                "borra el historial de git",
            ),
            (
                r"(?i)(?:\bformat-volume\b|(?:^|[;&|]\s*)(?:sudo\s+)?format(?:\.com|\.exe)?(?:\s|$))",
                "formatea un disco",
            ),
            (
                r"(?i)\bdiskpart(?:\.exe)?\b",
                "reparte o formatea discos",
            ),
            (r"(?i)\bmkfs\b", "formatea un sistema de archivos"),
            (
                r"(?i)\bdd\b[^\n]*\bof=/dev/",
                "escribe directamente en un dispositivo",
            ),
            (
                r"(?i)\bgit\b[^\n]*\bpush\b(?:[^\n]*(?:--force|(?:^|\s)-f(?:\s|$))[^\n]*\b(?:main|master|release)\b|[^\n]*\b(?:main|master|release)\b[^\n]*(?:--force|(?:^|\s)-f(?:\s|$)))",
                "reescribe una rama principal con git push --force",
            ),
            (
                r"(?i)\bgit\b[^\n]*\breset\b[^\n]*--hard\b",
                "descarta cambios sin confirmar",
            ),
            (
                r"(?i)\bgit\b[^\n]*\bclean\b[^\n]*(?:\s-[a-z]*f[a-z]*\b|\s--force\b)",
                "borra archivos sin seguimiento",
            ),
            (
                r"(?i)\bgit\b[^\n]*\bcheckout\b[^\n]*\s(?:--\s+)?\.(?:\s|$|[;&|])",
                "descarta cambios sin confirmar",
            ),
            (
                r"(?i)\bgit\b[^\n]*\brestore\b[^\n]*\s(?:--\s+)?\.(?:\s|$|[;&|])",
                "descarta cambios sin confirmar",
            ),
            (
                r"(?i)\bgit\b[^\n]*\bstash\b[^\n]*\b(?:drop|clear)\b",
                "borra cambios guardados en el stash",
            ),
            (
                r"(?i:\bgit\b[^\n]*\bbranch\b[^\n]*)\s(?:-D\b|--delete\s+--force\b)",
                "borra una rama sin fusionar",
            ),
            (r"(?i)\bnpm\s+publish\b", "publica un paquete"),
            (r"(?i)\bcargo\s+publish\b", "publica un paquete"),
            (r"(?i)\btwine\s+upload\b", "publica un paquete"),
            (
                r"(?i)\bdocker\s+system\s+prune\b",
                "borra imágenes y volúmenes de Docker",
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
                r#"(?i)(?:^|[\s"'/\\])(?:\.ssh(?:[\\/]|\b)|\.aws(?:[\\/]|\b)|\.gnupg(?:[\\/]|\b)|\.git[\\/]|\.env|id_rsa|credential)"#,
                CREDENTIALS,
            ),
            (
                r#"(?i)(?:^|[\s"'/\\])\.config[\\/]gh(?:[\\/]|\b)"#,
                CREDENTIALS,
            ),
        ];
        table
            .into_iter()
            .map(|(pattern, reason)| (Regex::new(pattern).expect("patrón fijo válido"), reason))
            .collect()
    })
}

/// Motivo por el que un comando está en la lista de denegación, si lo está.
pub fn denied(command: &str) -> Option<&'static str> {
    rules()
        .iter()
        .find(|(pattern, _)| pattern.is_match(command))
        .map(|(_, reason)| *reason)
}

/// Motivo por el que una ruta editada está siempre denegada, si lo está.
pub fn denied_path(path: &str) -> Option<&'static str> {
    let text = path.replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = text.split('/').filter(|part| !part.is_empty()).collect();
    for (index, part) in parts.iter().enumerate() {
        if part.ends_with(".pem") || part.ends_with(".key") {
            return Some(CREDENTIALS);
        }
        if part.starts_with(".env") || part.starts_with("id_rsa") {
            return Some(CREDENTIALS);
        }
        if matches!(*part, ".ssh" | ".aws" | ".gnupg") {
            return Some(CREDENTIALS);
        }
        if *part == ".git" {
            return Some("toca el historial de git");
        }
        if *part == "gh" && index > 0 && parts[index - 1] == ".config" {
            return Some(CREDENTIALS);
        }
        if *part == "credential" || part.starts_with("credential") {
            return Some(CREDENTIALS);
        }
    }
    None
}

/// Resuelve una ruta de pantalla contra la carpeta de trabajo, como haría el
/// shell; `None` si es relativa y no hay carpeta.
pub(crate) fn resolve_in(path: &str, base: Option<&Path>) -> Option<PathBuf> {
    let raw = Path::new(path);
    let rooted = raw.is_absolute()
        || raw
            .components()
            .next()
            .is_some_and(|part| matches!(part, Component::RootDir));
    let absolute = if rooted {
        raw.to_path_buf()
    } else {
        base?.join(raw)
    };
    Some(paths::resolve(&absolute))
}

fn keys(path: &Path) -> Vec<String> {
    path.components()
        .map(|part| {
            let text = part.as_os_str().to_string_lossy().to_string();
            if cfg!(windows) {
                text.to_lowercase()
            } else {
                text
            }
        })
        .collect()
}

/// `candidate` es igual a `root` o está dentro (por componentes). Las dos se
/// resuelven contra el disco: un enlace dentro de la carpeta no escapa.
pub(crate) fn within(candidate: &Path, root: &Path) -> bool {
    let candidate = keys(&paths::resolve(candidate));
    let root = keys(&paths::resolve(root));
    !root.is_empty() && candidate.len() >= root.len() && candidate[..root.len()] == root[..]
}

/// La ruta está dentro de alguna de las carpetas permitidas al papel.
pub(crate) fn path_allowed(path: &str, edit_dirs: &[PathBuf], workdir: Option<&Path>) -> bool {
    let Some(resolved) = resolve_in(path, workdir) else {
        return false;
    };
    edit_dirs.iter().any(|dir| within(&resolved, dir))
}

/// La ruta resuelta es `root` o está dentro.
pub(crate) fn resolves_inside(path: &str, root: &Path, workdir: Option<&Path>) -> bool {
    resolve_in(path, workdir).is_some_and(|candidate| within(&candidate, root))
}

fn is_rm_recursive(token: &str) -> bool {
    if token == "--recursive" {
        return true;
    }
    if !token.starts_with('-') || token.starts_with("--") {
        return false;
    }
    token[1..].chars().any(|part| part == 'r' || part == 'R')
}

fn is_flag(verb: &str, token: &str) -> bool {
    token.starts_with('-') || (matches!(verb, "rd" | "rmdir" | "del") && token.starts_with('/'))
}

fn is_recursive(verb: &str, rest: &[&str]) -> bool {
    match verb {
        "rm" => rest.iter().any(|token| is_rm_recursive(token)),
        "rd" | "rmdir" | "del" => rest.iter().any(|token| token.eq_ignore_ascii_case("/s")),
        "remove-item" | "ri" => rest
            .iter()
            .any(|token| token.eq_ignore_ascii_case("-recurse")),
        _ => false,
    }
}

/// Rutas que borra un comando recursivo; `None` si no es un borrado recursivo.
fn recursive_delete_targets(command: &str) -> Option<Vec<&str>> {
    let mut tokens: Vec<&str> = py_split(command).collect();
    if tokens
        .first()
        .is_some_and(|token| token.eq_ignore_ascii_case("sudo"))
    {
        tokens.remove(0);
    }
    let first = tokens.first()?;
    let name = first.rsplit(['\\', '/']).next()?.to_lowercase();
    let verb = name.strip_suffix(".exe").unwrap_or(&name).to_string();
    let rest = &tokens[1..];
    if !is_recursive(&verb, rest) {
        return None;
    }
    Some(
        rest.iter()
            .copied()
            .filter(|token| !is_flag(&verb, token))
            .map(|token| token.trim_matches(['"', '\'']))
            .filter(|token| !token.is_empty())
            .collect(),
    )
}

/// Rutas con `~` o variables que el módulo no puede expandir sin shell.
fn needs_expansion(path: &str) -> bool {
    path.starts_with('~') || path.starts_with('%') || path.contains('$')
}

/// Motivo por el que un borrado recursivo toca fuera de las carpetas del papel.
pub(crate) fn deletes_outside(
    command: &str,
    edit_dirs: &[PathBuf],
    workdir: Option<&Path>,
) -> Option<&'static str> {
    let targets = recursive_delete_targets(command)?;
    if targets.is_empty() {
        return Some("no pude comprobar qué borra el comando");
    }
    for target in targets {
        if needs_expansion(target) {
            return Some("no pude comprobar dónde borra el comando");
        }
        let Some(resolved) = resolve_in(target, workdir) else {
            return Some("no pude comprobar dónde borra el comando");
        };
        if edit_dirs.iter().any(|dir| within(dir, &resolved)) {
            return Some("borra la carpeta de trabajo completa");
        }
        if !edit_dirs.iter().any(|dir| within(&resolved, dir)) {
            return Some("borra archivos fuera de la carpeta de trabajo");
        }
    }
    None
}
