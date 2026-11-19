//! Lectura de `espacios.json` (formatos viejo y nuevo) y de `elefantes.json`.
//!
//! El formato nuevo guarda `elefante = { agente, modelo, esfuerzo }`; el viejo
//! guardaba `elefante = { agente, argv }` y no tiene modelo (diseño §14.2).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::Deserialize;
use uuid::Uuid;

/// Configuración del elefante de un espacio, ya normalizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceConfig {
    pub id: String,
    pub name: String,
    pub folder: PathBuf,
    pub agent: String,
    /// Vacío en el formato viejo: el espacio pide configurarlo.
    pub model: String,
    pub effort: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpacesFile {
    #[serde(default)]
    espacios: Vec<RawSpace>,
}

#[derive(Debug, Deserialize)]
struct RawSpace {
    id: String,
    #[serde(rename = "nombre")]
    name: String,
    #[serde(rename = "carpeta")]
    folder: String,
    elefante: RawElephant,
}

#[derive(Debug, Deserialize)]
struct RawElephant {
    #[serde(rename = "agente")]
    agent: String,
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    modelo: String,
    #[serde(default)]
    esfuerzo: Option<String>,
}

impl RawSpace {
    fn normalize(self) -> SpaceConfig {
        // DECISION: el formato viejo no tiene modelo; se deja vacío para que la
        // persona lo configure, en lugar de adivinarlo desde `argv`.
        let _ = &self.elefante.argv;
        SpaceConfig {
            id: self.id,
            name: self.name,
            folder: PathBuf::from(&self.folder),
            agent: self.elefante.agent,
            model: self.elefante.modelo,
            effort: self.elefante.esfuerzo,
        }
    }
}

/// Lee los espacios; un archivo ausente o vacío da una lista vacía.
pub fn load_spaces(path: &Path) -> Result<Vec<SpaceConfig>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("No pude leer {}: {error}.", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: SpacesFile = serde_json::from_str(&text)
        .map_err(|error| format!("No pude leer {}: {error}.", path.display()))?;
    Ok(file.espacios.into_iter().map(RawSpace::normalize).collect())
}

/// Tokens de los elefantes, guardados en `elefantes.json` (espacio → token).
pub struct ElephantTokens {
    path: PathBuf,
    tokens: Mutex<BTreeMap<String, String>>,
}

impl ElephantTokens {
    /// Carga el archivo; uno ausente o inválido empieza vacío (los tokens se
    /// regeneran al lanzar cada elefante).
    pub fn load(path: PathBuf) -> Self {
        let tokens = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            tokens: Mutex::new(tokens),
        }
    }

    /// Todos los pares (espacio, token) registrados al arrancar.
    pub fn all(&self) -> Vec<(String, String)> {
        self.tokens
            .lock()
            .iter()
            .map(|(space, token)| (space.clone(), token.clone()))
            .collect()
    }

    /// Token guardado para `space`, si existe.
    pub fn get(&self, space: &str) -> Option<String> {
        self.tokens.lock().get(space).cloned()
    }

    /// Asocia `space` con `token` y guarda el archivo de forma atómica.
    pub fn set(&self, space: &str, token: &str) -> Result<(), String> {
        let mut tokens = self.tokens.lock();
        tokens.insert(space.to_string(), token.to_string());
        write_atomic(&self.path, &tokens)
    }

    /// Quita el token de `space` y guarda el archivo.
    pub fn remove(&self, space: &str) {
        let mut tokens = self.tokens.lock();
        if tokens.remove(space).is_some() {
            if let Err(error) = write_atomic(&self.path, &tokens) {
                tracing::warn!(%error, "no pude guardar elefantes.json");
            }
        }
    }
}

fn write_atomic(path: &Path, tokens: &BTreeMap<String, String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(tokens).map_err(|error| error.to_string())?;
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    fs::write(&tmp, &text).map_err(|error| error.to_string())?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn reads_the_new_elephant_format() {
        let dir = TempDir::new("runtime-spaces-new");
        let path = dir.path().join("espacios.json");
        fs::write(
            &path,
            r#"{"espacios":[{"id":"s1","nombre":"Ez","carpeta":"C:\\p",
                "elefante":{"agente":"opencode","modelo":"opencode-go/x","esfuerzo":"max"}}]}"#,
        )
        .expect("escribir");
        let spaces = load_spaces(&path).expect("leer");
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].model, "opencode-go/x");
        assert_eq!(spaces[0].effort.as_deref(), Some("max"));
    }

    #[test]
    fn reads_the_old_argv_format_with_an_empty_model() {
        let dir = TempDir::new("runtime-spaces-old");
        let path = dir.path().join("espacios.json");
        fs::write(
            &path,
            r#"{"espacios":[{"id":"s1","nombre":"Ez","carpeta":".",
                "elefante":{"agente":"claude","argv":["claude"]}}]}"#,
        )
        .expect("escribir");
        let spaces = load_spaces(&path).expect("leer");
        assert_eq!(spaces[0].agent, "claude");
        assert!(spaces[0].model.is_empty());
        assert_eq!(spaces[0].effort, None);
    }

    #[test]
    fn missing_spaces_file_is_an_empty_list() {
        let dir = TempDir::new("runtime-spaces-missing");
        assert!(load_spaces(&dir.path().join("espacios.json"))
            .expect("leer")
            .is_empty());
    }

    #[test]
    fn elephant_tokens_round_trip_and_remove() {
        let dir = TempDir::new("runtime-elephant-tokens");
        let path = dir.path().join("elefantes.json");
        let tokens = ElephantTokens::load(path.clone());
        assert_eq!(tokens.get("s1"), None);
        tokens.set("s1", "abc").expect("guardar");
        assert_eq!(
            ElephantTokens::load(path.clone()).get("s1").as_deref(),
            Some("abc")
        );
        tokens.remove("s1");
        assert_eq!(ElephantTokens::load(path).get("s1"), None);
    }
}
