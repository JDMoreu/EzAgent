//! Artefactos congelados y detección de cambios del elefante (§14.4, §14.6).

use std::path::PathBuf;

use ez_team::Seat;

use crate::paths;
use crate::state::Task;
use crate::texts;
use crate::types::Rejection;

use super::ArbiterShared;

impl ArbiterShared {
    /// Verificación de R3: los criterios no cambiaron desde la aprobación.
    pub(crate) fn check_frozen(&self, task: &Task) -> Result<(), Rejection> {
        let live = paths::task_dir(&task.space.folder, &task.id).join("criterios.toml");
        let sha = paths::sha256_file(&live).map_err(|message| Rejection::new("R3", message))?;
        if sha == task.criteria_sha {
            Ok(())
        } else {
            Err(Rejection::new("R3", texts::CRITERIA_CHANGED))
        }
    }

    /// Congela encargo, criterios y equipo en la carpeta privada (§14.6).
    pub(crate) fn freeze_artifacts(&self, task: &Task) -> Result<(String, String), Rejection> {
        let private = self.frozen_dir(task);
        let live = paths::task_dir(&task.space.folder, &task.id);
        for name in ["criterios.toml", "encargo.md"] {
            paths::copy_file(&live.join(name), &private.join(name)).map_err(|error| {
                Rejection::new("R3", format!("No se pudo congelar {name}: {error}."))
            })?;
        }
        let criteria_sha = paths::sha256_file(&private.join("criterios.toml"))
            .map_err(|message| Rejection::new("R3", message))?;
        let encargo_sha = paths::sha256_file(&private.join("encargo.md"))
            .map_err(|message| Rejection::new("R3", message))?;
        Ok((criteria_sha, encargo_sha))
    }

    /// Congela el equipo aprobado (§14.6).
    pub(crate) fn freeze_team(&self, task: &Task) -> Result<String, Rejection> {
        let bytes = serde_json::to_vec_pretty(&(&task.team, &task.team_card)).map_err(|error| {
            Rejection::new("R5", format!("No se pudo congelar el equipo: {error}."))
        })?;
        let path = self.frozen_dir(task).join("equipo.json");
        paths::write_atomic(&path, &bytes).map_err(|error| {
            Rejection::new("R5", format!("No se pudo congelar el equipo: {error}."))
        })?;
        paths::sha256_file(&path).map_err(|message| Rejection::new("R5", message))
    }

    /// Comprueba que los tres artefactos sigan iguales a la copia congelada.
    pub(crate) fn check_artifacts(&self, task: &Task) -> Result<(), Rejection> {
        let live = paths::task_dir(&task.space.folder, &task.id);
        for (name, expected) in [
            ("criterios.toml", &task.criteria_sha),
            ("encargo.md", &task.encargo_sha),
        ] {
            if expected.is_empty() {
                continue;
            }
            let sha = paths::sha256_file(&live.join(name))
                .map_err(|message| Rejection::new("R3", message))?;
            if &sha != expected {
                return Err(Rejection::new("R3", texts::ARTIFACTS_MODIFIED));
            }
        }
        if let Some(expected) = &task.team_sha {
            let bytes = serde_json::to_vec_pretty(&(&task.team, &task.team_card))
                .map_err(|error| Rejection::new("R5", format!("{error}")))?;
            if paths::sha256_hex(&bytes) != *expected {
                return Err(Rejection::new("R3", texts::ARTIFACTS_MODIFIED));
            }
        }
        Ok(())
    }

    /// Restaura los artefactos desde la copia privada al reanudar.
    pub(crate) fn restore_artifacts(&self, task: &mut Task) -> Result<(), Rejection> {
        let private = self.frozen_dir(task);
        let live = paths::task_dir(&task.space.folder, &task.id);
        for name in ["criterios.toml", "encargo.md"] {
            paths::copy_file(&private.join(name), &live.join(name)).map_err(|error| {
                Rejection::new("R4", format!("No se pudo restaurar {name}: {error}."))
            })?;
        }
        if task.team_sha.is_some() {
            let path = private.join("equipo.json");
            let bytes = std::fs::read(&path).map_err(|error| {
                Rejection::new("R4", format!("No se pudo leer el equipo: {error}."))
            })?;
            let (team, card): (Vec<Seat>, String) =
                serde_json::from_slice(&bytes).map_err(|error| {
                    Rejection::new("R4", format!("Equipo congelado ilegible: {error}."))
                })?;
            task.team = team;
            task.team_card = card;
        }
        Ok(())
    }

    /// Carpeta privada de artefactos congelados de una tarea.
    pub(crate) fn frozen_dir(&self, task: &Task) -> PathBuf {
        crate::paths::data_task_dir(&self.data_dir, &task.id).join("congelados")
    }
}
