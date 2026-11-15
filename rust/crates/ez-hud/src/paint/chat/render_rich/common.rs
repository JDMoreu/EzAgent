//! Shared helpers of the rich chat blocks (part of `render_rich.py`, F5-13).

use ez_core::pyvalue::py_strip;

pub(super) const DEFAULT_TILE: [u8; 3] = [120, 130, 150];

// ------------------------------------------------------------------ helpers
/// `1,4 MB` / `820 KB` / `12 B` (`media.human_size`).
pub fn human_size(size: u64) -> String {
    let mut value = size as f64;
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

const FAMILY: [(&str, [u8; 3]); 11] = [
    (".pdf", [214, 76, 70]),
    (".doc", [58, 112, 214]),
    (".docx", [58, 112, 214]),
    (".odt", [58, 112, 214]),
    (".xls", [40, 150, 96]),
    (".xlsx", [40, 150, 96]),
    (".ods", [40, 150, 96]),
    (".csv", [40, 150, 96]),
    (".ppt", [222, 120, 50]),
    (".pptx", [222, 120, 50]),
    (".odp", [222, 120, 50]),
];

pub fn family_color(ext: &str) -> [u8; 3] {
    FAMILY
        .iter()
        .find(|(suffix, _)| *suffix == ext)
        .map(|(_, rgb)| *rgb)
        .unwrap_or(DEFAULT_TILE)
}

/// `markdown.is_local_target`: does it look like a local file path?
pub fn is_local_target(target: &str) -> bool {
    let text = py_strip(target);
    let bytes = text.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }
    text.starts_with("\\\\")
        || text.starts_with("file:")
        || text.starts_with("~/")
        || text.starts_with("~\\")
        || text.starts_with("./")
        || text.starts_with(".\\")
        || text.starts_with("../")
        || text.starts_with("..\\")
        || text.starts_with('/')
}

/// `urlparse(src).netloc`, or `None` when it is empty (the caller falls back
/// to the whole source).
pub fn url_netloc(src: &str) -> Option<String> {
    let after_scheme = src
        .find("://")
        .map(|index| &src[index + 3..])
        .or_else(|| src.strip_prefix("//"))?;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    (!authority.is_empty()).then(|| authority.to_string())
}
