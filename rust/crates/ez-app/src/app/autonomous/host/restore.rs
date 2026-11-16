//! Al arrancar la app: una tarea viva vuelve en `paused` y se avisa; nunca se
//! reanuda sola (9.3, 10.6 y decisión 6 de A3).

use std::path::Path;

use ez_agent::autonomous::artifacts::{Event, TaskDir};
use ez_agent::autonomous::{Action, Criterion};
use ez_core::pyvalue::py_strip;
use serde_json::json;

use super::super::{AutonomousLoop, Session};

impl AutonomousLoop {
    /// Restaura la última tarea viva y la deja en pausa. `true` si la encontró.
    pub fn restore_after_restart(&self) -> bool {
        if self.busy() {
            return false;
        }
        let Some(dir) = latest_task(&self.root) else {
            return false;
        };
        let Ok(mut state) = dir.load_state() else {
            tracing::warn!("No pude leer el estado autónomo guardado.");
            return false;
        };
        if !state.unfinished() {
            return false;
        }
        let restarted = matches!(state.on_restart(self.now()), Action::Pause(_));
        let task = std::fs::read_to_string(dir.shared().join("encargo.md"))
            .map(|text| parse_encargo(&text))
            .unwrap_or_default();
        let criteria = std::fs::read_to_string(dir.shared().join("criterios.md"))
            .map(|text| parse_criteria(&text))
            .unwrap_or_default();
        let executor_agent = state
            .executor
            .as_ref()
            .map_or_else(String::new, |executor| executor.agent.clone());
        let session = Session {
            state,
            dir: dir.clone(),
            task: task.0,
            criteria,
            space: task.1,
            verifier_agent: executor_agent.clone(),
            executor_agent,
            sent_at: 0.0,
            idle_since: None,
            verification: None,
            last_verdict: None,
            last_clean: None,
            fingerprints: Vec::new(),
            announced_round: 0,
        };
        if restarted {
            let reason = ez_agent::autonomous::texts::REASON_RESTART;
            if let Err(error) = session.dir.write_state(&session.state) {
                tracing::warn!("No pude guardar la pausa del reinicio: {error}");
            }
            self.log(&session, Event::Pause, json!({"reason": reason}));
            self.say(&ez_agent::autonomous::texts::paused(reason));
        }
        self.adopt(session);
        true
    }
}

/// La carpeta de tarea más reciente con `privado/estado.json`.
fn latest_task(root: &Path) -> Option<TaskDir> {
    let mut best: Option<(String, TaskDir)> = None;
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("privado").join("estado.json").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if best.as_ref().is_none_or(|(known, _)| name > *known) {
            best = Some((name, TaskDir { dir: path }));
        }
    }
    best.map(|(_, dir)| dir)
}

/// Tarea y espacio de `encargo.md`, tal como los escribe el código.
pub fn parse_encargo(text: &str) -> (String, String) {
    let mut task = String::new();
    let mut space = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = py_strip(line);
        if trimmed == "# Encargo" && task.is_empty() {
            for next in lines.by_ref() {
                let candidate = py_strip(next);
                if !candidate.is_empty() {
                    task = candidate.to_string();
                    break;
                }
            }
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("Espacio: ") {
            space = py_strip(value).to_string();
        }
    }
    (task, space)
}

/// Criterios numerados con su comprobación, tal como los escribe el código.
pub fn parse_criteria(text: &str) -> Vec<Criterion> {
    let mut criteria: Vec<Criterion> = Vec::new();
    for line in text.lines() {
        let trimmed = py_strip(line);
        if let Some((number, rest)) = trimmed.split_once(". ") {
            if number.chars().all(|character| character.is_ascii_digit()) {
                criteria.push(Criterion {
                    text: rest.to_string(),
                    check: String::new(),
                });
                continue;
            }
        }
        if let Some(check) = trimmed.strip_prefix("Comprobación: ") {
            if let Some(last) = criteria.last_mut() {
                last.check = check.to_string();
            }
        }
    }
    criteria
}
