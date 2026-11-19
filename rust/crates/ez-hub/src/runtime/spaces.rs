//! Lectura de `espacios.json` y de los archivos de elección y tokens de los
//! agentes principales (`principales.json` y `elefantes.json`).
//!
//! `espacios.json` guarda solo `{id, nombre, carpeta}` (§15.1). El formato viejo
//! traía `elefante = { agente, argv }` (o `{ agente, modelo, esfuerzo }`): se lee,
//! se descarta ese campo y su elección queda como sugerencia del formulario.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Elección de agente, modelo y esfuerzo de un agente principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainChoice {
    #[serde(rename = "agente")]
    pub agent: String,
    #[serde(rename = "modelo", default)]
    pub model: String,
    #[serde(rename = "esfuerzo", default)]
    pub effort: Option<String>,
}

/// Configuración de un espacio: solo identidad y carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceConfig {
    pub id: String,
    pub name: String,
    pub folder: PathBuf,
    /// Elección del formato viejo, para proponerla en el formulario.
    pub suggested: Option<MainChoice>,
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
    #[serde(default)]
    elefante: Option<RawElephant>,
}

#[derive(Debug, Deserialize)]
struct RawElephant {
    #[serde(rename = "agente")]
    agent: String,
    #[serde(default)]
    modelo: String,
    #[serde(default)]
    esfuerzo: Option<String>,
}

impl RawSpace {
    fn normalize(self) -> SpaceConfig {
        // DECISION: el campo `elefante` del formato viejo no se guarda en el
        // espacio: se conserva en memoria como sugerencia del formulario para no
        // perder la elección de la persona al migrar.
        let suggested = self.elefante.map(|old| MainChoice {
            agent: old.agent,
            model: old.modelo,
            effort: old.esfuerzo,
        });
        SpaceConfig {
            id: self.id,
            name: self.name,
            folder: PathBuf::from(&self.folder),
            suggested,
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

/// Elección de cada agente principal, guardada en `principales.json`.
pub struct Principales {
    path: PathBuf,
    choices: Mutex<BTreeMap<String, MainChoice>>,
}

impl Principales {
    /// Carga el archivo; uno ausente o inválido empieza vacío.
    pub fn load(path: PathBuf) -> Self {
        let choices = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<BTreeMap<String, MainChoice>>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            choices: Mutex::new(choices),
        }
    }

    /// Elección de `elephant`, si está guardada.
    pub fn get(&self, elephant: &str) -> Option<MainChoice> {
        self.choices.lock().get(elephant).cloned()
    }

    /// Asocia `elephant` con `choice` y guarda el archivo de forma atómica.
    pub fn set(&self, elephant: &str, choice: &MainChoice) -> Result<(), String> {
        let mut choices = self.choices.lock();
        choices.insert(elephant.to_string(), choice.clone());
        write_atomic(&self.path, &*choices)
    }

    /// Quita la elección de `elephant` y guarda el archivo.
    pub fn remove(&self, elephant: &str) {
        let mut choices = self.choices.lock();
        if choices.remove(elephant).is_some() {
            if let Err(error) = write_atomic(&self.path, &*choices) {
                tracing::warn!(%error, "no pude guardar principales.json");
            }
        }
    }
}

/// Tokens de los agentes principales, por id de agente principal (no por espacio).
pub struct ElephantTokens {
    path: PathBuf,
    tokens: Mutex<BTreeMap<String, String>>,
}

impl ElephantTokens {
    /// Carga el archivo; uno ausente o inválido empieza vacío (los tokens se
    /// regeneran al lanzar cada agente principal).
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

    /// Todos los pares (id de agente principal, token) registrados al arrancar.
    pub fn all(&self) -> Vec<(String, String)> {
        self.tokens
            .lock()
            .iter()
            .map(|(elephant, token)| (elephant.clone(), token.clone()))
            .collect()
    }

    /// Token guardado para `elephant`, si existe.
    pub fn get(&self, elephant: &str) -> Option<String> {
        self.tokens.lock().get(elephant).cloned()
    }

    /// Asocia `elephant` con `token` y guarda el archivo de forma atómica.
    pub fn set(&self, elephant: &str, token: &str) -> Result<(), String> {
        let mut tokens = self.tokens.lock();
        tokens.insert(elephant.to_string(), token.to_string());
        write_atomic(&self.path, &*tokens)
    }

    /// Quita el token de `elephant` y guarda el archivo.
    pub fn remove(&self, elephant: &str) {
        let mut tokens = self.tokens.lock();
        if tokens.remove(elephant).is_some() {
            if let Err(error) = write_atomic(&self.path, &*tokens) {
                tracing::warn!(%error, "no pude guardar elefantes.json");
            }
        }
    }
}

fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
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
    fn reads_the_new_spaces_format_without_an_elephant() {
        let dir = TempDir::new("runtime-spaces-new");
        let path = dir.path().join("espacios.json");
        fs::write(
            &path,
            r#"{"espacios":[{"id":"s1","nombre":"Ez","carpeta":"C:\\p"}]}"#,
        )
        .expect("escribir");
        let spaces = load_spaces(&path).expect("leer");
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].folder, PathBuf::from("C:\\p"));
        assert_eq!(spaces[0].suggested, None);
    }

    #[test]
    fn reads_the_old_elephant_format_as_a_suggestion() {
        let dir = TempDir::new("runtime-spaces-old");
        let path = dir.path().join("espacios.json");
        fs::write(
            &path,
            r#"{"espacios":[{"id":"s1","nombre":"Ez","carpeta":".",
                "elefante":{"agente":"claude","argv":["claude"],"modelo":"opus","esfuerzo":"max"}}]}"#,
        )
        .expect("escribir");
        let spaces = load_spaces(&path).expect("leer");
        assert_eq!(
            spaces[0].suggested,
            Some(MainChoice {
                agent: "claude".to_string(),
                model: "opus".to_string(),
                effort: Some("max".to_string()),
            })
        );
    }

    #[test]
    fn reads_the_old_argv_format_with_an_empty_model() {
        let dir = TempDir::new("runtime-spaces-argv");
        let path = dir.path().join("espacios.json");
        fs::write(
            &path,
            r#"{"espacios":[{"id":"s1","nombre":"Ez","carpeta":".",
                "elefante":{"agente":"claude","argv":["claude"]}}]}"#,
        )
        .expect("escribir");
        let spaces = load_spaces(&path).expect("leer");
        let suggested = spaces[0].suggested.as_ref().expect("sugerencia");
        assert_eq!(suggested.agent, "claude");
        assert!(suggested.model.is_empty());
        assert_eq!(suggested.effort, None);
    }

    #[test]
    fn missing_spaces_file_is_an_empty_list() {
        let dir = TempDir::new("runtime-spaces-missing");
        assert!(load_spaces(&dir.path().join("espacios.json"))
            .expect("leer")
            .is_empty());
    }

    #[test]
    fn main_choices_round_trip() {
        let dir = TempDir::new("runtime-principales");
        let path = dir.path().join("principales.json");
        let choices = Principales::load(path.clone());
        assert_eq!(choices.get("e1"), None);
        let choice = MainChoice {
            agent: "opencode".to_string(),
            model: "opencode-go/x".to_string(),
            effort: None,
        };
        choices.set("e1", &choice).expect("guardar");
        assert_eq!(Principales::load(path.clone()).get("e1"), Some(choice));
        choices.remove("e1");
        assert_eq!(Principales::load(path).get("e1"), None);
    }

    #[test]
    fn elephant_tokens_are_keyed_by_main_agent_id() {
        let dir = TempDir::new("runtime-main-tokens");
        let path = dir.path().join("elefantes.json");
        let tokens = ElephantTokens::load(path.clone());
        tokens.set("e1", "abc").expect("guardar");
        assert_eq!(
            ElephantTokens::load(path.clone()).get("e1").as_deref(),
            Some("abc")
        );
        assert_eq!(tokens.all(), vec![("e1".to_string(), "abc".to_string())]);
        tokens.remove("e1");
        assert_eq!(ElephantTokens::load(path).get("e1"), None);
    }
}
