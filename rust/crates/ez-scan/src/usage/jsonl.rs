//! Utilidades compartidas por los lectores: fechas, archivos `.jsonl` y colas
//! de registros grandes.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, TimeZone};

/// Milisegundos desde la época.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// La fecha de modificación de un archivo, en milisegundos.
pub(crate) fn mtime_ms(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let millis = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    u64::try_from(millis).ok()
}

/// Recorre `root` y devuelve sus `.jsonl` (hasta `depth` carpetas).
pub(crate) fn collect_jsonl(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_into(root, depth, &mut found);
    found
}

fn collect_into(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_into(&path, depth - 1, found);
            }
        } else if path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
}

/// Los `.jsonl` de `root` modificados desde `start_ms`, del más nuevo al más viejo.
pub(crate) fn files_since(root: &Path, start_ms: u64) -> Vec<PathBuf> {
    let mut files: Vec<(u64, PathBuf)> = collect_jsonl(root, 6)
        .into_iter()
        .filter_map(|path| mtime_ms(&path).map(|mtime| (mtime, path)))
        .filter(|(mtime, _)| *mtime >= start_ms)
        .collect();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    files.into_iter().map(|(_, path)| path).collect()
}

/// Todos los `.jsonl` de `root`, del más nuevo al más viejo (aunque sean viejos).
pub(crate) fn files_newest_first(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(u64, PathBuf)> = collect_jsonl(root, 6)
        .into_iter()
        .filter_map(|path| mtime_ms(&path).map(|mtime| (mtime, path)))
        .collect();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    files.into_iter().map(|(_, path)| path).collect()
}

/// Llama a `visit` con cada línea de `path`; las líneas ilegibles se saltan.
pub(crate) fn read_lines(path: &Path, mut visit: impl FnMut(&str)) {
    let Ok(file) = File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        visit(&line);
    }
}

/// El final de un archivo grande (para no cargar 20 MB de registro).
pub(crate) fn read_tail(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Interpreta una fecha RFC 3339 (`2026-11-17T17:25:59.123Z`) en milisegundos.
pub(crate) fn parse_time_ms(text: &str) -> Option<u64> {
    let parsed = DateTime::parse_from_rfc3339(text.trim()).ok()?;
    u64::try_from(parsed.timestamp_millis()).ok()
}

/// La fecha en hora local, para mostrar.
pub(crate) fn format_local(ms: u64) -> String {
    let Ok(ms) = i64::try_from(ms) else {
        return ms.to_string();
    };
    match Local.timestamp_millis_opt(ms).single() {
        Some(date) => date.format("%Y-%m-%d %H:%M").to_string(),
        None => ms.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_fecha_rfc3339_se_vuelve_milisegundos() {
        let millis = parse_time_ms("2026-01-02T03:04:05.000Z").expect("fecha");
        assert_eq!(format_local(millis).len(), 16);
        assert!(parse_time_ms("no es una fecha").is_none());
    }

    #[test]
    fn solo_junta_jsonl_y_respeta_la_antiguedad() {
        let root = std::env::temp_dir().join(format!("ez-scan-jsonl-{}", std::process::id()));
        let folder = root.join("a").join("b");
        std::fs::create_dir_all(&folder).expect("carpeta");
        std::fs::write(folder.join("uno.jsonl"), "{}\n").expect("uno");
        std::fs::write(folder.join("dos.txt"), "{}\n").expect("dos");
        let now = now_ms();
        assert_eq!(files_since(&root, now.saturating_sub(60_000)).len(), 1);
        assert!(files_since(&root, now + 60_000).is_empty());
        assert_eq!(collect_jsonl(&root, 6).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
