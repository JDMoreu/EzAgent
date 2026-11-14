//! Utilidades de disco de las skills: firmas, hash de carpeta y skills por defecto.

use std::path::{Path, PathBuf};

use ez_core::pyvalue::{char_len, head_chars};
use sha2::{Digest, Sha256};

use super::{Fingerprint, MAX_SKILL_CHARS};

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

pub(super) fn posix(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn truncate(text: &str) -> (String, bool) {
    if char_len(text) > MAX_SKILL_CHARS {
        (head_chars(text, MAX_SKILL_CHARS).to_string(), true)
    } else {
        (text.to_string(), false)
    }
}

fn mtime_ns(meta: &std::fs::Metadata) -> i128 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_nanos() as i128)
}

fn path_signature(path: &Path) -> (String, i128, i64) {
    match std::fs::metadata(path) {
        Ok(meta) => (
            path.display().to_string(),
            mtime_ns(&meta),
            meta.len() as i64,
        ),
        Err(_) => (path.display().to_string(), -1, -1),
    }
}

/// Todas las rutas bajo `root` (sin seguir enlaces a carpetas), ordenadas por ruta posix.
fn walk(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(listing) = std::fs::read_dir(root) else {
        return;
    };
    for entry in listing.filter_map(Result::ok) {
        let path = entry.path();
        out.push(path.clone());
        let is_real_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_real_dir {
            walk(&path, out);
        }
    }
}

fn sorted_tree(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    walk(root, &mut paths);
    paths.sort_by_key(|path| posix(path));
    paths
}

/// mtime y tamaño de la carpeta y de todo su contenido, sin leerlo.
pub(super) fn tree_signature(root: &Path) -> Fingerprint {
    let mut signature = vec![path_signature(root)];
    signature.extend(sorted_tree(root).iter().map(|path| path_signature(path)));
    signature
}

fn is_regular_file(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|meta| meta.is_file())
}

fn relative_posix(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(|_| posix(path), posix)
}

/// SHA-256 de todos los archivos de una carpeta, con rutas relativas.
pub fn hash_folder(folder: &Path) -> String {
    let mut files: Vec<PathBuf> = sorted_tree(folder)
        .into_iter()
        .filter(|path| is_regular_file(path))
        .collect();
    files.sort_by_key(|path| relative_posix(folder, path));
    let mut digest = Sha256::new();
    for path in files {
        digest.update(relative_posix(folder, &path).as_bytes());
        digest.update([0_u8]);
        match std::fs::read(&path) {
            Ok(bytes) => digest.update(&bytes),
            Err(_) => digest.update(b"<unreadable>"),
        }
        digest.update([0_u8]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn list_files(folder: &Path) -> Vec<String> {
    let mut files: Vec<String> = sorted_tree(folder)
        .into_iter()
        .filter(|path| is_regular_file(path) && file_name(path) != "SKILL.md")
        .map(|path| relative_posix(folder, &path))
        .collect();
    files.sort();
    files
}

/// Copia un árbol de carpetas (el `shutil.copytree` de `ensure_default_skills`).
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ())
}

/// Carpeta de las skills empaquetadas junto al ejecutable (o, en desarrollo, en el repo).
pub fn default_skills_dir() -> PathBuf {
    // DECISION: el Python usa `agent/default_skills` junto al módulo; aquí se
    // busca junto al ejecutable y, si no, en el árbol de código fuente.
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("default_skills")));
    match beside.filter(|dir| dir.is_dir()) {
        Some(dir) => dir,
        None => {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/ezagent/agent/default_skills")
        }
    }
}

/// Copia las skills empaquetadas a `skills_dir` si faltan; nunca sobrescribe.
/// Devuelve los nombres copiados.
pub fn ensure_default_skills(skills_dir: &Path, source_dir: Option<&Path>) -> Vec<String> {
    let source = source_dir.map_or_else(default_skills_dir, Path::to_path_buf);
    if !source.is_dir() {
        tracing::warn!(
            "No existe la carpeta de skills empaquetadas: {}",
            source.display()
        );
        return Vec::new();
    }
    if let Err(error) = std::fs::create_dir_all(skills_dir) {
        tracing::warn!(
            "No se pudo preparar la carpeta de skills {}: {error}",
            skills_dir.display()
        );
        return Vec::new();
    }
    let Ok(listing) = std::fs::read_dir(&source) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path.join("SKILL.md").is_file()
                && !file_name(path).starts_with(['.', '_'])
        })
        .collect();
    folders.sort_by_key(|path| file_name(path).to_lowercase());
    let mut copied = Vec::new();
    for folder in folders {
        let destination = skills_dir.join(file_name(&folder));
        if destination.exists() {
            continue;
        }
        // No se borra una copia parcial: se registra para que el usuario la revise.
        match copy_tree(&folder, &destination) {
            Ok(()) => copied.push(file_name(&folder)),
            Err(error) => tracing::warn!(
                "No se pudo copiar la skill «{}» a {}: {error}",
                file_name(&folder),
                destination.display()
            ),
        }
    }
    copied
}
