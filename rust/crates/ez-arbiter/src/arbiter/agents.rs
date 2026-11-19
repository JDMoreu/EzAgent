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
    list.sort_by_key(|left| left.key.storage_key());
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

/// Añade `.ez/` a `info/exclude` del repositorio común si no está (§15.1).
/// El archivo es común a todos los worktrees, así los artefactos nunca se confirman.
fn exclude_ez(space_folder: &Path) -> Result<(), String> {
    let reported = git(space_folder, &["rev-parse", "--git-path", "info/exclude"])?;
    let path = PathBuf::from(reported.trim());
    let path = if path.is_absolute() {
        path
    } else {
        space_folder.join(path)
    };
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    if previous.lines().any(|line| line.trim() == ".ez/") {
        return Ok(());
    }
    let mut text = previous;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(".ez/\n");
    std::fs::write(&path, text).map_err(|error| error.to_string())
}

/// Confirma en la rama `ez/<id>` el trabajo sin confirmar del worktree.
/// Devuelve `false` si no había cambios; los artefactos bajo `.ez/` nunca entran.
pub(crate) fn commit_work(worktree: &Path, title: &str, round: u32) -> Result<bool, String> {
    // DECISION: la forma `:(exclude):(top).ez` evita que git rechace excluir
    // una ruta ya ignorada por `info/exclude` (la de §15.1); `exclude_ez`
    // garantiza esa condición también en worktrees heredados.
    exclude_ez(worktree)?;
    git(worktree, &["add", "-A", "--", ".", ":(exclude):(top).ez"])?;
    let check = crate::exec::command("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", "--cached", "--quiet"])
        .output()
        .map_err(|error| texts::git_failed(&error.to_string()))?;
    match check.status.code() {
        Some(0) => Ok(false),
        Some(1) => {
            let message = format!("EzAgent: «{title}» (ronda {round}, dos rondas limpias)");
            git(worktree, &["commit", "-q", "-m", &message])?;
            Ok(true)
        }
        _ => Err(texts::git_failed(
            String::from_utf8_lossy(&check.stderr).trim(),
        )),
    }
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
        exclude_ez(&space_info.folder).map_err(|message| Rejection::new("R12", message))?;
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

#[cfg(test)]
mod tests {
    use super::{commit_work, current_branch, merge};
    use std::path::Path;

    fn run(folder: &Path, args: &[&str]) {
        let mut command = crate::exec::command("git");
        command.arg("-C").arg(folder).args(args);
        assert!(command.status().expect("git").success(), "git {args:?}");
    }

    fn repo(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ez-arbiter-merge-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("crear raíz");
        run(&root, &["init", "-q"]);
        run(&root, &["config", "user.name", "Pez de prueba"]);
        run(&root, &["config", "user.email", "pez@example.test"]);
        std::fs::write(root.join("x.txt"), "base").expect("escribir x");
        run(&root, &["add", "x.txt"]);
        run(&root, &["commit", "-q", "-m", "inicial"]);
        root
    }

    #[test]
    fn merges_a_branch_with_new_work() {
        let root = repo("ok");
        let branch = current_branch(&root).expect("rama base");
        run(&root, &["checkout", "-q", "-b", "ez/prueba"]);
        std::fs::write(root.join("nuevo.txt"), "hecho").expect("nuevo archivo");
        run(&root, &["add", "nuevo.txt"]);
        run(&root, &["commit", "-q", "-m", "trabajo"]);
        run(&root, &["checkout", "-q", &branch]);
        merge(&root, "ez/prueba", "Tarea").expect("integrar");
        assert!(root.join("nuevo.txt").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn commit_work_ignores_ez_and_reports_whether_there_were_changes() {
        let root = repo("confirmar");
        std::fs::create_dir_all(root.join(".ez/tareas")).expect("crear .ez");
        std::fs::write(root.join(".ez/tareas/x"), "secreto").expect("artefacto");
        std::fs::write(root.join("obra.txt"), "hecho").expect("obra");
        assert!(commit_work(&root, "Tarea", 2).expect("confirmar"));
        let output = crate::exec::command("git")
            .arg("-C")
            .arg(&root)
            .args(["ls-tree", "-r", "--name-only", "HEAD"])
            .output()
            .expect("git ls-tree");
        let tree = String::from_utf8_lossy(&output.stdout);
        assert!(tree.contains("obra.txt"), "{tree}");
        assert!(!tree.contains(".ez/"), "{tree}");
        assert!(!commit_work(&root, "Tarea", 2).expect("sin cambios"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_conflict_leaves_the_working_tree_unchanged() {
        let root = repo("conflicto");
        let branch = current_branch(&root).expect("rama base");
        run(&root, &["checkout", "-q", "-b", "ez/prueba"]);
        std::fs::write(root.join("x.txt"), "del worktree").expect("cambiar x");
        run(&root, &["add", "x.txt"]);
        run(&root, &["commit", "-q", "-m", "trabajo"]);
        run(&root, &["checkout", "-q", &branch]);
        std::fs::write(root.join("x.txt"), "de la base").expect("cambiar x en base");
        run(&root, &["add", "x.txt"]);
        run(&root, &["commit", "-q", "-m", "base"]);
        let error = merge(&root, "ez/prueba", "Tarea").expect_err("conflicto");
        assert!(error.contains("conflicto"), "{error}");
        assert_eq!(
            std::fs::read_to_string(root.join("x.txt")).expect("x"),
            "de la base"
        );
        assert!(
            !root.join("MERGE_HEAD").exists(),
            "no quedó una fusión a medias"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
