//! Rutas de la tarea, escritura atómica y huellas sha256.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Carpeta de una tarea dentro del proyecto: `<carpeta>/.ez/tareas/<id>`.
pub fn task_dir(folder: &Path, task: &str) -> PathBuf {
    folder.join(".ez").join("tareas").join(task)
}

/// Carpeta de una ronda: `.ez/tareas/<id>/ronda-<n>`.
pub fn round_dir(folder: &Path, task: &str, round: u32) -> PathBuf {
    task_dir(folder, task).join(format!("ronda-{round}"))
}

/// Copia un archivo creando las carpetas que falten.
pub fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)?;
    Ok(())
}

/// Escribe un archivo con temporal + renombrar (estado.json).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("estado"),
        uuid::Uuid::new_v4().simple()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Agrega una línea a un archivo jsonl.
pub fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

/// Lee un archivo de texto; error con la ruta en el mensaje.
pub fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Huella sha256 en hexadecimal.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Huella sha256 de un archivo.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Normaliza `.` y `..` sin tocar el disco. `None` si se sale de la raíz.
pub fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Resuelve `candidate` (relativa a `folder` o absoluta) y comprueba que quede
/// dentro de `folder`. Devuelve la ruta normalizada.
pub fn resolve_inside(folder: &Path, candidate: &Path) -> Option<PathBuf> {
    let base = folder
        .canonicalize()
        .unwrap_or_else(|_| folder.to_path_buf());
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    let resolved = joined
        .canonicalize()
        .unwrap_or_else(|_| normalize(&joined).unwrap_or_else(|| joined.clone()));
    if resolved.starts_with(&base) {
        Some(resolved)
    } else {
        None
    }
}

/// Carpeta de estado de una tarea dentro de los datos de la app.
pub fn data_task_dir(data_dir: &Path, task: &str) -> PathBuf {
    data_dir.join("tareas").join(task)
}
