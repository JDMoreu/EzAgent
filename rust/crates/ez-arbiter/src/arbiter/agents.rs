//! Agentes principales, sus worktrees y la integración de ramas (diseño §15.1).
//!
//! `agentes.json` vive en los datos de la app; los agentes no lo escriben.
//! Git se ejecuta siempre fuera del candado del árbitro.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::texts;
use crate::types::{ElephantKey, MainAgent, Rejection};

use super::{elephant_queue, ArbiterShared};

#[derive(Debug, Default, Serialize, Deserialize)]
struct AgentsFile {
    #[serde(default)]
    agents: Vec<MainAgent>,
}

/// Lee `agentes.json`; un archivo ilegible se ignora con un aviso.
pub(crate) fn load(data_dir: &Path) -> Vec<MainAgent> {
    let path = data_dir.join("agentes.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<AgentsFile>(&text) {
        Ok(file) => file.agents,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "agentes.json ilegible");
            Vec::new()
        }
    }
}

/// Guarda la lista de agentes con escritura atómica.
pub(crate) fn save(data_dir: &Path, agents: HashMap<String, MainAgent>) -> Result<(), Rejection> {
    let mut list: Vec<MainAgent> = agents.into_values().collect();
    list.sort_by(|left, right| left.key.storage_key().cmp(&right.key.storage_key()));
    let bytes = serde_json::to_vec_pretty(&AgentsFile { agents: list }).map_err(|error| {
        Rejection::new(
            "R12",
            format!("No se pudo serializar agentes.json: {error}."),
        )
    })?;
    let path = data_dir.join("agentes.json");
    paths::write_atomic(&path, &bytes).map_err(|error| {
        Rejection::new("R12", format!("No se pudo guardar agentes.json: {error}."))
    })
}

/// `git -C <folder> <args>` con stdout; error con el stderr redactado.
fn git(folder: &Path, args: &[&str]) -> Result<String, String> {
    let output = crate::exec::command("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .map_err(|error| texts::git_failed(&error.to_string()))?;
    if !output.status.success() {
        return Err(texts::git_failed(
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Rama actual de una carpeta git.
pub(crate) fn current_branch(folder: &Path) -> Result<String, String> {
    git(folder, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|text| text.trim().to_string())
}

/// Crea el worktree y la rama `ez/<id>` desde el HEAD actual.
fn add(space_folder: &Path, agent: &MainAgent) -> Result<(), String> {
    git(
        space_folder,
        &[
            "worktree",
            "add",
            "-b",
            &agent.branch,
            &agent.worktree.to_string_lossy(),
        ],
    )?;
    Ok(())
}

/// ¿Está limpio el worktree? (`git status --porcelain` sin salida).
fn clean(worktree: &Path) -> Result<bool, String> {
    git(worktree, &["status", "--porcelain"]).map(|text| text.trim().is_empty())
}

/// Quita el worktree sin forzar.
fn remove(space_folder: &Path, worktree: &Path) -> Result<(), String> {
    git(
        space_folder,
        &["worktree", "remove", &worktree.to_string_lossy()],
    )?;
    Ok(())
}

/// Integra la rama del agente en la rama base del espacio, sin fast-forward.
pub(crate) fn merge(space_folder: &Path, branch: &str, title: &str) -> Result<(), String> {
    let message = format!("EzAgent: integra «{title}»");
    if git(space_folder, &["merge", "--no-ff", branch, "-m", &message]).is_ok() {
        return Ok(());
    }
    let _ = git(space_folder, &["merge", "--abort"]);
    Err("la integración produjo un conflicto".to_string())
}

impl ArbiterShared {
    /// Crea un agente principal con su worktree y lo registra en `agentes.json`.
    pub fn create_main_agent(&self, space: &str, name: &str) -> Result<MainAgent, Rejection> {
        let space_info = self.space(space)?;
        let key = ElephantKey::new(space);
        let folder =
            PathBuf::from(format!("{}.ez-wt", space_info.folder.display())).join(&key.elephant);
        if folder.exists() {
            return Err(Rejection::new(
                "R12",
                texts::cannot_remove_agent("ya existe esa carpeta de trabajo"),
            ));
        }
        let base_branch =
            current_branch(&space_info.folder).map_err(|message| Rejection::new("R12", message))?;
        let agent = MainAgent {
            key: key.clone(),
            name: name.to_string(),
            worktree: folder,
            branch: format!("ez/{}", key.elephant),
            base_branch,
        };
        add(&space_info.folder, &agent).map_err(|message| Rejection::new("R12", message))?;
        let scope = key.storage_key();
        let mut inner = self.inner.lock();
        inner.main_agents.insert(scope.clone(), agent.clone());
        inner.elephants.entry(scope).or_insert_with(elephant_queue);
        let agents = inner.main_agents.clone();
        drop(inner);
        save(&self.data_dir, agents)?;
        Ok(agent)
    }

    /// Quita un agente principal solo sin tarea activa y con worktree limpio.
    pub fn remove_main_agent(&self, key: &ElephantKey) -> Result<(), Rejection> {
        let scope = key.storage_key();
        let agent = {
            let inner = self.inner.lock();
            inner.main_agents.get(&scope).cloned().ok_or_else(|| {
                Rejection::new("R12", texts::cannot_remove_agent("no existe ese agente"))
            })?
        };
        if key.elephant != ElephantKey::default_for(&key.space).elephant {
            let active = self
                .inner
                .lock()
                .tasks
                .values()
                .any(|task| task.is_active() && task.elephant == *key);
            if active {
                return Err(Rejection::new(
                    "R12",
                    texts::cannot_remove_agent("tiene una tarea activa"),
                ));
            }
            let is_clean =
                clean(&agent.worktree).map_err(|message| Rejection::new("R12", message))?;
            if !is_clean {
                return Err(Rejection::new(
                    "R12",
                    texts::cannot_remove_agent("su worktree tiene cambios"),
                ));
            }
            let space_info = self.space(&key.space)?;
            remove(&space_info.folder, &agent.worktree)
                .map_err(|message| Rejection::new("R12", message))?;
        }
        let mut inner = self.inner.lock();
        inner.main_agents.remove(&scope);
        inner.elephants.remove(&scope);
        let agents = inner.main_agents.clone();
        drop(inner);
        save(&self.data_dir, agents)
    }

    /// Agentes principales registrados en un espacio.
    pub fn main_agents(&self, space: &str) -> Vec<MainAgent> {
        let inner = self.inner.lock();
        let mut agents: Vec<MainAgent> = inner
            .main_agents
            .values()
            .filter(|agent| agent.key.space == space)
            .cloned()
            .collect();
        agents.sort_by(|left, right| left.key.elephant.cmp(&right.key.elephant));
        agents
    }

    /// Agente principal por su clave; el por omisión si no está registrado.
    pub(crate) fn agent_for(&self, key: &ElephantKey) -> Option<MainAgent> {
        self.inner
            .lock()
            .main_agents
            .get(&key.storage_key())
            .cloned()
    }
}
