//! Búsqueda del ejecutable `hc`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Nombre del ejecutable en cada sistema.
const HC_EXE: &str = if cfg!(windows) { "hc.exe" } else { "hc" };

/// Busca `hc` en este orden: la ruta configurada si existe,
/// `%LOCALAPPDATA%\Programs\herdr-core\hc.exe` y `hc.exe` en `PATH`.
pub fn locate_hc(configured: Option<&Path>) -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA");
    let path = std::env::var_os("PATH");
    locate_in(
        configured,
        local_app_data.as_deref().map(Path::new),
        path.as_deref(),
    )
}

/// Parte pura de `locate_hc`, con el entorno recibido para poder probarla.
fn locate_in(
    configured: Option<&Path>,
    local_app_data: Option<&Path>,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        if configured.is_file() {
            return Some(configured.to_path_buf());
        }
    }
    if let Some(base) = local_app_data {
        let candidate = base.join("Programs").join("herdr-core").join(HC_EXE);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path_var = path_var?;
    std::env::split_paths(path_var)
        .map(|directory| directory.join(HC_EXE))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::locate_in;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("ez-term-locate-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("carpeta temporal");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("carpeta del ejecutable");
        }
        fs::write(path, b"").expect("ejecutable vacio");
    }

    #[test]
    fn configured_path_wins_when_it_exists() {
        let dir = TempDir::new("configured");
        let hc = dir.0.join("hc.exe");
        touch(&hc);
        let found = locate_in(Some(&hc), None, None).expect("ruta configurada");
        assert_eq!(found, hc);
    }

    #[test]
    fn local_app_data_is_second() {
        let dir = TempDir::new("local");
        let hc = dir
            .0
            .join("Programs")
            .join("herdr-core")
            .join(super::HC_EXE);
        touch(&hc);
        let found = locate_in(None, Some(&dir.0), None).expect("localappdata");
        assert_eq!(found, hc);
    }

    #[test]
    fn path_is_the_last_resort() {
        let dir = TempDir::new("path");
        let hc = dir.0.join(super::HC_EXE);
        touch(&hc);
        let path_var = std::env::join_paths([dir.0.as_path()]).expect("PATH de prueba");
        let found = locate_in(None, None, Some(&path_var)).expect("PATH");
        assert_eq!(found, hc);
    }

    #[test]
    fn missing_everywhere_is_none() {
        let dir = TempDir::new("missing");
        let path_var = std::env::join_paths([dir.0.as_path()]).expect("PATH de prueba");
        assert_eq!(
            locate_in(Some(&dir.0.join("nope.exe")), Some(&dir.0), Some(&path_var)),
            None
        );
    }
}
