//! Lectura y validación de `criterios.toml` (diseño §5).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Sección `[validacion]` con sus valores por omisión.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Validation {
    #[serde(default = "default_min_mutations")]
    pub mutaciones_minimas: u32,
    #[serde(default = "default_rounds")]
    pub rondas_maximas: u32,
    #[serde(default = "default_hours")]
    pub horas_maximas: f64,
}

fn default_min_mutations() -> u32 {
    3
}

fn default_rounds() -> u32 {
    5
}

fn default_hours() -> f64 {
    2.0
}

impl Default for Validation {
    fn default() -> Self {
        Self {
            mutaciones_minimas: default_min_mutations(),
            rondas_maximas: default_rounds(),
            horas_maximas: default_hours(),
        }
    }
}

/// Un criterio de aceptación.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Criterion {
    pub id: String,
    #[serde(default)]
    pub descripcion: String,
    pub comando: Vec<String>,
    #[serde(default = "dot")]
    pub carpeta: String,
    #[serde(default)]
    pub codigo: i32,
    #[serde(default)]
    pub contiene: Option<String>,
    #[serde(default = "default_seconds")]
    pub segundos: u64,
}

fn dot() -> String {
    ".".to_string()
}

fn default_seconds() -> u64 {
    300
}

/// Contenido validado de `criterios.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Criteria {
    #[serde(default)]
    pub validacion: Validation,
    #[serde(default = "Vec::new", rename = "criterio")]
    pub criteria: Vec<Criterion>,
}

impl Criteria {
    /// Criterios como líneas de tarjeta (`- C1: desc → comando`).
    pub fn card_lines(&self) -> Vec<String> {
        self.criteria
            .iter()
            .map(|criterion| {
                crate::texts::card_criterion(
                    &criterion.id,
                    &criterion.descripcion,
                    &criterion.command_text(),
                )
            })
            .collect()
    }
}

impl Criterion {
    /// Comando en una línea, unido con espacios (para tarjetas y mensajes).
    pub fn command_text(&self) -> String {
        self.comando.join(" ")
    }

    /// Carpeta del criterio resuelta dentro de la carpeta del espacio.
    pub fn working_dir(&self, folder: &Path) -> PathBuf {
        crate::paths::resolve_inside(folder, Path::new(&self.carpeta))
            .unwrap_or_else(|| folder.to_path_buf())
    }
}

/// Lee y valida un archivo de criterios. El error ya está redactado.
pub fn load(path: &Path, folder: &Path) -> Result<Criteria, String> {
    let text = crate::paths::read_text(path)?;
    let criteria: Criteria = toml::from_str(&text)
        .map_err(|error| format!("No se pudo leer {}: {error}.", path.display()))?;
    validate(&criteria, folder)?;
    Ok(criteria)
}

/// Valida las reglas de la sección 5 del diseño.
pub fn validate(criteria: &Criteria, folder: &Path) -> Result<(), String> {
    if criteria.criteria.is_empty() {
        return Err("Los criterios no tienen ningún `[[criterio]]`: agrega al menos uno.".into());
    }
    let mut seen: Vec<&str> = Vec::new();
    for criterion in &criteria.criteria {
        let id = criterion.id.trim();
        if id.is_empty() {
            return Err("Hay un criterio sin `id`: cada criterio necesita uno.".into());
        }
        if seen.contains(&id) {
            return Err(format!("El id de criterio {id} está repetido."));
        }
        seen.push(id);
        if criterion.comando.is_empty() || criterion.comando[0].trim().is_empty() {
            return Err(format!(
                "El criterio {id} no tiene comando: escribe `comando`."
            ));
        }
        if crate::paths::resolve_inside(folder, Path::new(&criterion.carpeta)).is_none() {
            return Err(format!(
                "La carpeta del criterio {id} queda fuera de la carpeta del espacio."
            ));
        }
        let joined = criterion.command_text();
        if let Some(reason) = ez_herdr::permission::denied(&joined) {
            return Err(format!(
                "El comando del criterio {id} está en la lista de denegación: {reason}."
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(text: &str) -> Criteria {
        toml::from_str(text).expect("criterios de prueba válidos")
    }

    #[test]
    fn defaults_apply() {
        let criteria = sample("[[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"--version\"]\n");
        assert_eq!(criteria.validacion.mutaciones_minimas, 3);
        assert_eq!(criteria.validacion.rondas_maximas, 5);
        assert_eq!(criteria.criteria[0].codigo, 0);
        assert_eq!(criteria.criteria[0].segundos, 300);
        assert_eq!(criteria.criteria[0].carpeta, ".");
    }

    #[test]
    fn rejects_empty_id_and_duplicates() {
        let folder = std::env::temp_dir();
        let empty = sample("[[criterio]]\nid = \" \"\ncomando = [\"git\"]\n");
        assert!(validate(&empty, &folder).is_err());
        let twice = sample(
            "[[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\n\
             [[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\n",
        );
        assert!(validate(&twice, &folder).is_err());
    }

    #[test]
    fn rejects_empty_command_and_outside_folder() {
        let folder = std::env::temp_dir();
        let empty = sample("[[criterio]]\nid = \"C1\"\ncomando = []\n");
        assert!(validate(&empty, &folder).is_err());
        let outside =
            sample("[[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\ncarpeta = \"../fuera\"\n");
        assert!(validate(&outside, &folder).is_err());
    }

    #[test]
    fn rejects_denied_command() {
        let folder = std::env::temp_dir();
        let denied =
            sample("[[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"reset\", \"--hard\"]\n");
        assert!(validate(&denied, &folder).is_err());
    }

    #[test]
    fn accepts_plain_git() {
        let folder = std::env::temp_dir();
        let ok = sample("[[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"--version\"]\n");
        assert!(validate(&ok, &folder).is_ok());
        assert_eq!(ok.criteria[0].command_text(), "git --version");
    }
}
