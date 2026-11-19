//! Fotos de git para atribuir cambios (R4, R7, R9, R11).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::texts;

/// Foto del repositorio: HEAD y la línea de estado de cada ruta.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSnapshot {
    pub head: Option<String>,
    /// Ruta (relativa a la raíz del repositorio) → código de estado.
    pub status: BTreeMap<String, String>,
}

/// `git -C <folder> <args>` y devuelve stdout; error con stderr redactado.
fn git(folder: &Path, args: &[&str]) -> Result<String, String> {
    let output = crate::exec::command("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .map_err(|error| texts::git_failed(&error.to_string()))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(texts::git_failed(detail.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// ¿La carpeta está dentro de un árbol de trabajo git?
pub fn is_repo(folder: &Path) -> bool {
    git(folder, &["rev-parse", "--is-inside-work-tree"])
        .map(|text| text.trim() == "true")
        .unwrap_or(false)
}

/// Foto actual del repositorio.
pub fn snapshot(folder: &Path) -> Result<GitSnapshot, String> {
    let head = git(folder, &["rev-parse", "HEAD"])
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let raw = git(
        folder,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(GitSnapshot {
        head,
        status: parse_status(raw.as_bytes()),
    })
}

/// Lee la salida de `git status --porcelain=v1 -z`.
pub fn parse_status(raw: &[u8]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let text = String::from_utf8_lossy(entry);
        let (code, path) = text.split_at(3);
        let code = code.trim_end().to_string();
        map.insert(path.to_string(), code.clone());
        // Un renombrado o copiado lleva la ruta original en el trozo siguiente.
        if code.starts_with('R') || code.starts_with('C') {
            let _ = parts.next();
        }
    }
    map
}

/// Rutas cambiadas entre dos fotos, sin lo que está bajo `.ez/`.
pub fn changed(folder: &Path, before: &GitSnapshot, after: &GitSnapshot) -> Vec<String> {
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for (path, code) in &after.status {
        if before.status.get(path) != Some(code) {
            paths.insert(path.clone());
        }
    }
    for path in before.status.keys() {
        if !after.status.contains_key(path) {
            paths.insert(path.clone());
        }
    }
    if let (Some(from), Some(to)) = (&before.head, &after.head) {
        if from != to {
            if let Ok(text) = git(folder, &["diff", "--name-only", from, to]) {
                for line in text.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        paths.insert(line.replace('\\', "/"));
                    }
                }
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !path.starts_with(".ez/"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn parses_z_status_with_rename() {
        let raw = b" M src/a.rs\0?? nuevo.txt\0R  nuevo.rs\0viejo.rs\0";
        let map = parse_status(raw);
        assert_eq!(map.get("src/a.rs").map(String::as_str), Some(" M"));
        assert_eq!(map.get("nuevo.txt").map(String::as_str), Some("??"));
        assert_eq!(map.get("nuevo.rs").map(String::as_str), Some("R"));
        assert!(!map.contains_key("viejo.rs"));
    }
}
