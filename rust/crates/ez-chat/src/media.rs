//! Port of `src/ezagent/voice/chat/media.py` (F3-14).
//!
//! Imágenes y documentos del chat: qué se puede mostrar y qué se puede abrir.
//! Nada se descarga, se borra ni se ejecuta por su cuenta.

mod csv_preview;
mod image_probe;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use ez_core::pyvalue::{char_len, head_chars, py_rstrip, py_strip};

pub const IMAGE_EXTENSIONS: [&str; 6] = [".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"];
pub const TEXT_EXTENSIONS: [&str; 10] = [
    ".md",
    ".markdown",
    ".txt",
    ".csv",
    ".tsv",
    ".json",
    ".log",
    ".toml",
    ".yaml",
    ".yml",
];
pub const OFFICE_EXTENSIONS: [&str; 11] = [
    ".pdf", ".docx", ".doc", ".xlsx", ".xls", ".pptx", ".ppt", ".odt", ".ods", ".odp", ".rtf",
];
/// Lo único que «Abrir» acepta: nunca .exe, .bat, .cmd, .ps1, .lnk, .js…
pub const OPENABLE_EXTENSIONS: [&str; 27] = [
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".bmp",
    ".webp",
    ".md",
    ".markdown",
    ".txt",
    ".csv",
    ".tsv",
    ".json",
    ".log",
    ".toml",
    ".yaml",
    ".yml",
    ".pdf",
    ".docx",
    ".doc",
    ".xlsx",
    ".xls",
    ".pptx",
    ".ppt",
    ".odt",
    ".ods",
    ".odp",
    ".rtf",
];
pub const MAX_IMAGE_BYTES: i64 = 25 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;
pub const MAX_PREVIEW_BYTES: usize = 1024 * 1024;
pub const PREVIEW_CHARS: usize = 1800;
pub const PREVIEW_ROWS: usize = 8;

/// Tipo de archivo para pintar su tarjeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
    Office,
    Other,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "text",
            Self::Office => "office",
            Self::Other => "other",
        }
    }
}

/// Datos de un archivo local para pintar su tarjeta (sin leer su contenido).
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub ext: String,
    pub size: i64,
    pub mtime: f64,
    pub kind: Kind,
    /// Dentro de las carpetas de lectura permitidas.
    pub allowed: bool,
    /// Tipo seguro de abrir con un clic.
    pub openable: bool,
}

/// Imagen validada: las dimensiones salen de la cabecera, sin decodificar píxeles.
///
/// DECISION: convertir a RGBA requiere el crate `image`, que `ez-chat` no
/// declara; el HUD (F5-13) decodifica el archivo con `LoadedImage::path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedImage {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
}

/// Ruta absoluta de un enlace local (`C:\x`, `file:///x`, `~/x`, relativo).
pub fn resolve_target(target: &str, base_dirs: &[PathBuf]) -> Option<PathBuf> {
    let text = py_strip(target).trim_matches(|character| matches!(character, '<' | '>'));
    if text.is_empty() {
        return None;
    }
    let mut text = text.to_string();
    if text.to_lowercase().starts_with("file:") {
        text = file_url_path(&text);
    }
    let path = expand_user(&text);
    if path.is_absolute() {
        return Some(path);
    }
    for base in base_dirs {
        let candidate = base.join(&path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn file_url_path(text: &str) -> String {
    let rest = text.get(5..).unwrap_or("");
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path) = match rest.strip_prefix("//") {
        Some(after) => match after.find('/') {
            Some(index) => (&after[..index], &after[index..]),
            None => (after, ""),
        },
        None => ("", rest),
    };
    let mut raw = unquote(path);
    if !host.is_empty() && host != "localhost" {
        raw = format!("//{host}{raw}");
    } else if raw.len() > 2 && raw.starts_with('/') && raw.as_bytes().get(2) == Some(&b':') {
        raw.remove(0);
    }
    raw
}

/// `unquote` de `urllib.parse`: `%XX` a bytes y decodificación UTF-8 con reemplazo.
fn unquote(text: &str) -> String {
    let mut bytes: Vec<u8> = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '%' {
            let mut hex = String::new();
            for _ in 0..2 {
                if let Some(next) = chars
                    .peek()
                    .copied()
                    .filter(|value| value.is_ascii_hexdigit())
                {
                    hex.push(next);
                    chars.next();
                }
            }
            if hex.len() == 2 {
                if let Ok(value) = u8::from_str_radix(&hex, 16) {
                    bytes.push(value);
                    continue;
                }
            }
            bytes.push(b'%');
            bytes.extend_from_slice(hex.as_bytes());
        } else {
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn expand_user(text: &str) -> PathBuf {
    let rest = text.strip_prefix('~');
    match rest {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
            let rest = rest.trim_start_matches(['/', '\\']);
            ez_core::dirs::home().join(rest)
        }
        _ => PathBuf::from(text),
    }
}

/// ¿Está `path` dentro de alguna de las carpetas `roots`? (resuelve enlaces y `..`).
pub fn is_within(path: &Path, roots: &[PathBuf]) -> bool {
    let resolved = ez_core::paths::resolve(path);
    roots.iter().any(|root| {
        let base = ez_core::paths::resolve(&expand_user(&root.to_string_lossy()));
        starts_with_path(&resolved, &base)
    })
}

fn starts_with_path(candidate: &Path, base: &Path) -> bool {
    let candidate: Vec<String> = candidate
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let base: Vec<String> = base
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    candidate.len() >= base.len() && candidate[..base.len()] == base[..]
}

/// Tipo de archivo según su extensión.
pub fn kind_of(ext: &str) -> Kind {
    let ext = ext.to_lowercase();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Image
    } else if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Text
    } else if OFFICE_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Office
    } else {
        Kind::Other
    }
}

/// Ficha del archivo, o `None` si no existe o no es un archivo normal.
pub fn file_info(path: Option<&Path>, roots: Option<&[PathBuf]>) -> Option<FileInfo> {
    let path = path?;
    if !path.is_file() {
        return None;
    }
    let metadata = path.metadata().ok()?;
    let name = path
        .file_name()
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
    let ext = py_suffix(&name).to_lowercase();
    let modified = metadata.modified().ok()?;
    let mtime = match modified.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    let allowed = roots.is_none_or(|roots| is_within(path, roots));
    Some(FileInfo {
        path: path.to_path_buf(),
        name,
        ext: ext.clone(),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime,
        kind: kind_of(&ext),
        allowed,
        openable: is_openable(&ext),
    })
}

/// `Path(name).suffix.lower()`.
fn py_suffix(name: &str) -> String {
    let Some(index) = name.rfind('.') else {
        return String::new();
    };
    if index == 0 || index + 1 >= name.len() {
        return String::new();
    }
    name[index..].to_string()
}

fn is_openable(ext: &str) -> bool {
    OPENABLE_EXTENSIONS.contains(&ext)
}

/// ``1,4 MB`` / ``820 KB`` / ``12 B``.
pub fn human_size(size: i64) -> String {
    let mut value = size.max(0) as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if value < 1024.0 || unit == "GB" {
            let text = if unit == "B" || value >= 100.0 {
                format!("{value:.0}")
            } else {
                format!("{value:.1}").replace('.', ",")
            };
            return format!("{text} {unit}");
        }
        value /= 1024.0;
    }
    format!("{size} B")
}

/// Imagen validada para dibujar, o `None` si no se puede o no se debe mostrar.
pub fn load_image(info: Option<&FileInfo>) -> Option<LoadedImage> {
    let info = info?;
    if info.kind != Kind::Image || !info.allowed || info.size > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = match fs::read(&info.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("No pude abrir la imagen {}: {error}", info.name);
            return None;
        }
    };
    let Some((width, height, format)) = image_probe::probe(&bytes) else {
        tracing::warn!("No pude abrir la imagen {}: formato no válido", info.name);
        return None;
    };
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS || width < 1 || height < 1 {
        return None;
    }
    Some(LoadedImage {
        path: info.path.clone(),
        width,
        height,
        format,
    })
}

/// `(texto, hay_más)` con el principio de un archivo de texto; `None` si no aplica.
pub fn text_preview(info: Option<&FileInfo>) -> Option<(String, bool)> {
    let info = info?;
    if info.kind != Kind::Text || !info.allowed || info.size > (MAX_PREVIEW_BYTES * 8) as i64 {
        return None;
    }
    let file = match fs::File::open(&info.path) {
        Ok(file) => file,
        Err(_) => return None,
    };
    let mut raw: Vec<u8> = Vec::new();
    if file
        .take(MAX_PREVIEW_BYTES as u64)
        .read_to_end(&mut raw)
        .is_err()
    {
        return None;
    }
    let text = String::from_utf8_lossy(&raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let text: String = text
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || *character >= '\u{20}')
        .collect();
    if info.ext == ".csv" || info.ext == ".tsv" {
        let delimiter = if info.ext == ".tsv" { Some('\t') } else { None };
        return Some(csv_preview::table_preview(
            &text,
            delimiter,
            (raw.len() as i64) < info.size,
        ));
    }
    let more = char_len(&text) > PREVIEW_CHARS || (raw.len() as i64) < info.size;
    Some((
        py_rstrip(head_chars(&text, PREVIEW_CHARS)).to_string(),
        more,
    ))
}

/// Abre un documento con la aplicación predeterminada. Solo tipos seguros.
pub fn open_file(info: Option<&FileInfo>) -> bool {
    let Some(info) = info else {
        return false;
    };
    if !info.openable || !info.allowed {
        return false;
    }
    // DECISION: Python usa `os.startfile`; sin `windows` en este crate se abre
    // con el Explorador, que delega en la aplicación asociada y no usa shell.
    match Command::new("explorer").arg(&info.path).spawn() {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!("No pude abrir {}: {error}", info.name);
            false
        }
    }
}

/// Muestra el archivo en el Explorador (lista de argumentos, sin shell).
pub fn reveal_file(info: Option<&FileInfo>) -> bool {
    let Some(info) = info else {
        return false;
    };
    if !info.allowed {
        return false;
    }
    match Command::new("explorer")
        .arg(format!("/select,{}", info.path.display()))
        .spawn()
    {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!("No pude mostrar {}: {error}", info.name);
            false
        }
    }
}

#[cfg(test)]
mod tests;
