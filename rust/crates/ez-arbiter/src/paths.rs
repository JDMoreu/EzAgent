//! Rutas de la tarea, escritura atómica y huellas sha256.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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

/// Resuelve `path` dentro de `root` y comprueba que no se salga (diseño §14.7).
/// Canonicaliza la raíz y el destino; si el archivo no existe, canonicaliza su
/// padre y vuelve a juntar el nombre. `Path::starts_with` compara por componentes,
/// así que `proyecto-otro` no cuenta como dentro de `proyecto`.
pub fn inside(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let base = root
        .canonicalize()
        .map_err(|error| format!("No se pudo resolver {}: {error}.", root.display()))?;
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let resolved = match joined.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            let parent = joined
                .parent()
                .ok_or_else(|| format!("La ruta {} no tiene carpeta padre.", joined.display()))?;
            let parent = parent
                .canonicalize()
                .map_err(|error| format!("No se pudo resolver {}: {error}.", parent.display()))?;
            match joined.file_name() {
                Some(name) => parent.join(name),
                None => parent,
            }
        }
    };
    if resolved.starts_with(&base) {
        Ok(resolved)
    } else {
        Err(format!(
            "La ruta {} queda fuera de la carpeta del espacio.",
            path.display()
        ))
    }
}

/// Carpeta de estado de una tarea dentro de los datos de la app.
pub fn data_task_dir(data_dir: &Path, task: &str) -> PathBuf {
    data_dir.join("tareas").join(task)
}

#[cfg(test)]
mod tests {
    use super::inside;

    fn sandbox(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("ez-arbiter-paths-{name}"));
        let project = root.join("proyecto");
        let sibling = root.join("proyecto-otro");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(project.join("sub")).expect("crear proyecto");
        std::fs::create_dir_all(&sibling).expect("crear hermano");
        (project, root)
    }

    #[test]
    fn inside_accepts_dots_and_missing_files() {
        let (project, _root) = sandbox("puntos");
        std::fs::write(project.join("archivo.md"), "x").expect("escribir archivo");
        assert!(inside(&project, std::path::Path::new("sub/../archivo.md")).is_ok());
        let missing = inside(&project, std::path::Path::new("sub/nuevo.md"))
            .expect("archivo que todavía no existe");
        assert!(missing.ends_with("sub/nuevo.md"));
        let _ = std::fs::remove_dir_all(project.parent().expect("raíz"));
    }

    #[test]
    fn inside_rejects_parent_and_shared_prefix() {
        let (project, root) = sandbox("prefijo");
        assert!(inside(&project, std::path::Path::new("../proyecto-otro/x.md")).is_err());
        assert!(inside(&project, &root.join("proyecto-otro").join("x.md")).is_err());
        assert!(inside(&project, std::path::Path::new("../../fuera.md")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
