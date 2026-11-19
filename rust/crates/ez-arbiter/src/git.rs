//! Huellas de contenido del proyecto para atribuir cambios (§14.4).
//!
//! La huella es el HEAD más el sha256 del contenido de cada archivo que lista
//! `git ls-files -co --exclude-standard`, sin nada bajo `.ez/`. Un archivo que
//! no se puede leer cuenta con el hash `"ilegible"`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::texts;

/// Número de archivos a partir del cual se avisa en el log.
const FILE_WARN: usize = 20_000;
/// Bytes totales a partir de los cuales se avisa en el log.
const SIZE_WARN: u64 = 500 * 1024 * 1024;
/// Hash de un archivo que no se puede leer.
const UNREADABLE: &str = "ilegible";

/// Huella de contenido del proyecto (diseño §14.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub head: Option<String>,
    /// Ruta relativa con `/` → sha256 en hexadecimal.
    pub files: BTreeMap<String, String>,
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

/// Huella actual del proyecto (diseño §14.4).
pub fn snapshot(folder: &Path) -> Result<Fingerprint, String> {
    let head = git(folder, &["rev-parse", "HEAD"])
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let raw = git(folder, &["ls-files", "-co", "--exclude-standard", "-z"])?;
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for entry in raw
        .as_bytes()
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let relative = String::from_utf8_lossy(entry).replace('\\', "/");
        if relative.starts_with(".ez/") {
            continue;
        }
        let hash = match std::fs::read(folder.join(&relative)) {
            Ok(bytes) => {
                total = total.saturating_add(bytes.len() as u64);
                paths::sha256_hex(&bytes)
            }
            Err(_) => UNREADABLE.to_string(),
        };
        files.insert(relative, hash);
    }
    if files.len() > FILE_WARN || total > SIZE_WARN {
        tracing::warn!(
            files = files.len(),
            bytes = total,
            "huella grande: el cálculo puede tardar"
        );
    }
    Ok(Fingerprint { head, files })
}

/// Rutas cuyo contenido cambió entre dos huellas, sin lo bajo `.ez/`.
pub fn changed(before: &Fingerprint, after: &Fingerprint) -> Vec<String> {
    let mut paths: Vec<String> = before
        .files
        .keys()
        .chain(after.files.keys())
        .filter(|path| !path.starts_with(".ez/"))
        .filter(|path| before.files.get(*path) != after.files.get(*path))
        .cloned()
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::{changed, snapshot, Fingerprint};
    use std::collections::BTreeMap;

    fn fingerprint(files: &[(&str, &str)]) -> Fingerprint {
        Fingerprint {
            head: None,
            files: files
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn changed_detects_edits_adds_removes_and_ignores_ez() {
        let before = fingerprint(&[("a.rs", "1"), ("b.rs", "2"), (".ez/tareas/x", "x")]);
        let after = fingerprint(&[("a.rs", "9"), ("c.rs", "3")]);
        assert_eq!(changed(&before, &after), vec!["a.rs", "b.rs", "c.rs"]);
    }

    #[test]
    fn snapshot_hashes_a_real_project_and_ignores_ez() {
        let root =
            std::env::temp_dir().join(format!("ez-arbiter-fp-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(root.join(".ez")).expect("crear raíz");
        let _ = crate::exec::command("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status();
        std::fs::write(root.join("a.txt"), "hola").expect("escribir a");
        std::fs::write(root.join(".ez").join("interno.txt"), "ignorar").expect("escribir ez");
        let measured = snapshot(&root).expect("huella");
        assert_eq!(measured.files.len(), 1);
        assert!(measured.files.contains_key("a.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
