//! Carpeta temporal con nombre único para las pruebas; se borra al soltarla.

use std::path::{Path, PathBuf};

use uuid::Uuid;

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ez-hub-{tag}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("crear carpeta temporal");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
