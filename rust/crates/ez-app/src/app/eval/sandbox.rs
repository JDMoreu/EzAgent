//! Perfil de usuario temporal de una corrida de evals (`provider.py` 39-50).
//!
//! Todo lo que la app lee del perfil (configuración, memoria, skills, diario)
//! vive en la carpeta del caso, y la carpeta se borra al terminar.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `tempfile.mkdtemp(prefix="ezagent-evals-")` con el entorno redirigido.
pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    pub fn new() -> Result<Self, String> {
        let root = unique_dir("ezagent-evals-")?;
        std::fs::create_dir_all(root.join("home")).map_err(|error| error.to_string())?;
        for name in ["USERPROFILE", "HOME"] {
            std::env::set_var(name, root.join("home"));
        }
        for name in ["APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME"] {
            std::env::set_var(name, root.join(name.to_lowercase()));
        }
        std::env::remove_var("EZ_SYSTEM_PROMPT");
        Ok(Self { root })
    }

    /// `Path(tempfile.mkdtemp(dir=_SANDBOX))`: la carpeta de un caso.
    pub fn case_dir(&self) -> Result<CaseDir, String> {
        let path = self.root.join(unique_name());
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(CaseDir { path })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        remove_tree(&self.root);
    }
}

/// Carpeta de un caso; se borra al soltarla (`finally: shutil.rmtree(case_dir)`).
pub struct CaseDir {
    path: PathBuf,
}

impl CaseDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CaseDir {
    fn drop(&mut self) {
        remove_tree(&self.path);
    }
}

fn unique_dir(prefix: &str) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("{prefix}{}", unique_name()));
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

/// DECISION: la crate `tempfile` no está declarada en `ez-app`; pid, reloj y
/// contador dan un nombre distinto por corrida, que es lo único que se necesita.
fn unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos:x}-{count:x}", std::process::id())
}

/// `shutil.rmtree(dirs_exist_ok)`: solo dentro de nuestras carpetas temporales.
fn remove_tree(path: &Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        // `file_type` no sigue enlaces: un enlace simbólico o una unión de
        // Windows se quita sin entrar, así nunca se borra nada fuera de la arena.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            if std::fs::remove_file(&child).is_err() {
                let _ = std::fs::remove_dir(&child);
            }
        } else if kind.is_dir() {
            remove_tree(&child);
        } else {
            let _ = std::fs::remove_file(&child);
        }
    }
    let _ = std::fs::remove_dir(path);
}

#[cfg(test)]
pub(super) fn remove_tree_for_tests(path: &Path) {
    remove_tree(path);
}
