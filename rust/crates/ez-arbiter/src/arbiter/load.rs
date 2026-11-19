//! Recarga de tareas no finales al abrir el árbitro.

use std::fs;
use std::path::Path;

use crate::state::Task;

/// Lee `tareas/*/estado.json`; ignora las tareas terminadas y los archivos
/// ilegibles (avisan por `tracing`).
pub(crate) fn load_tasks(data_dir: &Path) -> Vec<Task> {
    let root = data_dir.join("tareas");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("estado.json");
        if !path.is_file() {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Task>(&text) {
                Ok(task) if task.is_active() => tasks.push(task),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, path = %path.display(), "estado ilegible"),
            },
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "no se pudo leer el estado")
            }
        }
    }
    tasks
}
