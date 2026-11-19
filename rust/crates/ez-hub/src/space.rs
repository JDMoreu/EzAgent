//! Espacios de trabajo y su archivo `espacios.json` (§4 y §10 del diseño).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{dirs, texts};

/// Agente elefante de un espacio: id del agente y su línea de comandos.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elephant {
    #[serde(rename = "agente")]
    pub agent: String,
    pub argv: Vec<String>,
}

/// Un espacio de trabajo tal como vive en `espacios.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "carpeta")]
    pub folder: String,
    #[serde(rename = "elefante")]
    pub elephant: Elephant,
}

/// Forma del archivo completo.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SpacesFile {
    espacios: Vec<Space>,
}

/// Elefante por omisión de cada agente que ofrece el formulario.
pub fn elephant_for(agent: &str) -> Option<Elephant> {
    let program = match agent {
        "claude" | "opencode" | "codex" => agent,
        _ => return None,
    };
    Some(Elephant {
        agent: program.to_string(),
        argv: vec![program.to_string()],
    })
}

/// Comprueba que la carpeta existe y es una carpeta; devuelve su ruta canónica.
pub fn validate_folder(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(texts::folder_missing(trimmed));
    }
    let path = PathBuf::from(trimmed);
    match fs::metadata(&path) {
        Ok(meta) if meta.is_dir() => Ok(fs::canonicalize(&path).unwrap_or(path)),
        Ok(_) => Err(texts::folder_not_dir(trimmed)),
        Err(_) => Err(texts::folder_missing(trimmed)),
    }
}

/// Arma un espacio nuevo validando nombre, carpeta y agente.
pub fn create_space(name: &str, folder: &str, agent: &str) -> Result<Space, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(texts::SPACE_NAME_EMPTY.to_string());
    }
    let folder = validate_folder(folder)?;
    let elephant = elephant_for(agent).ok_or_else(|| texts::unknown_agent(agent))?;
    Ok(Space {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        folder: folder.display().to_string(),
        elephant,
    })
}

/// Error de lectura o escritura de `espacios.json`.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(error) => write!(f, "{error}"),
            StoreError::Json(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        StoreError::Io(error)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        StoreError::Json(error)
    }
}

/// Archivo `%APPDATA%\EzAgent\hub\espacios.json`.
pub struct SpaceStore {
    path: PathBuf,
}

impl SpaceStore {
    /// Ubicación real de la app.
    pub fn for_app() -> Self {
        Self {
            path: dirs::app_data_dir().join("hub").join("espacios.json"),
        }
    }

    /// Ruta explícita (pruebas y recuperación).
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Lee los espacios; un archivo ausente o vacío es una lista vacía.
    pub fn load(&self) -> Result<Vec<Space>, StoreError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let file: SpacesFile = serde_json::from_str(&text)?;
        Ok(file.espacios)
    }

    /// Escribe la lista completa de forma atómica (temporal + renombrar).
    pub fn save(&self, spaces: &[Space]) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = SpacesFile {
            espacios: spaces.to_vec(),
        };
        let text = serde_json::to_string_pretty(&file)?;
        let tmp = self.path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        fs::write(&tmp, text)?;
        match fs::rename(&tmp, &self.path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&tmp);
                Err(error.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn missing_file_is_an_empty_list() {
        let dir = TempDir::new("spaces-missing");
        let store = SpaceStore::at(dir.path().join("espacios.json"));
        assert_eq!(store.load().expect("leer"), Vec::new());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = TempDir::new("spaces-round-trip");
        let store = SpaceStore::at(dir.path().join("espacios.json"));
        let space = create_space("Mi espacio", &dir.path().display().to_string(), "opencode")
            .expect("espacio válido");
        store.save(std::slice::from_ref(&space)).expect("guardar");
        let loaded = store.load().expect("leer");
        assert_eq!(loaded, vec![space]);
    }

    #[test]
    fn json_uses_the_agreed_keys() {
        let dir = TempDir::new("spaces-keys");
        let store = SpaceStore::at(dir.path().join("espacios.json"));
        let space = create_space("Ez", &dir.path().display().to_string(), "claude").expect("alto");
        store.save(&[space]).expect("guardar");
        let text = fs::read_to_string(store.path()).expect("leer texto");
        for key in [
            "\"espacios\"",
            "\"nombre\"",
            "\"carpeta\"",
            "\"elefante\"",
            "\"agente\"",
        ] {
            assert!(text.contains(key), "falta {key} en {text}");
        }
    }

    #[test]
    fn empty_file_is_an_empty_list() {
        let dir = TempDir::new("spaces-empty");
        let path = dir.path().join("espacios.json");
        fs::write(&path, "  \n").expect("escribir");
        let store = SpaceStore::at(path);
        assert_eq!(store.load().expect("leer"), Vec::new());
    }

    #[test]
    fn broken_file_is_an_error() {
        let dir = TempDir::new("spaces-broken");
        let path = dir.path().join("espacios.json");
        fs::write(&path, "{no es json").expect("escribir");
        let store = SpaceStore::at(path);
        assert!(store.load().is_err());
    }

    #[test]
    fn folder_must_exist_and_be_a_folder() {
        let dir = TempDir::new("spaces-folder");
        let file = dir.path().join("archivo.txt");
        fs::write(&file, "x").expect("escribir");
        assert!(validate_folder(&dir.path().display().to_string()).is_ok());
        assert!(validate_folder(&file.display().to_string()).is_err());
        assert!(validate_folder("").is_err());
        assert!(validate_folder("C:\\ruta\\que\\no\\existe\\ez-hub").is_err());
    }

    #[test]
    fn space_rejects_empty_name_and_unknown_agent() {
        let dir = TempDir::new("spaces-invalid");
        let folder = dir.path().display().to_string();
        assert!(create_space("  ", &folder, "claude").is_err());
        assert!(create_space("Ez", &folder, "vscode").is_err());
    }

    #[test]
    fn elephant_argv_matches_the_chosen_agent() {
        for agent in ["claude", "opencode", "codex"] {
            let elephant = elephant_for(agent).expect("agente conocido");
            assert_eq!(elephant.argv, vec![agent.to_string()]);
        }
    }
}
