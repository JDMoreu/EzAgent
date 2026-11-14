//! Port of `src/ezagent/swarm/progress.py` (F3-12).
//!
//! Progress notices: which stage finished and which one starts. Only top-level
//! tasks are considered; parts advance silently and their parent closes alone.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::models::{Task, MAX_ROUNDS};
use crate::protocol::stage_name;

fn join(names: &[String]) -> String {
    let mut unique: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for name in names {
        if seen.insert(name.as_str()) {
            unique.push(name);
        }
    }
    match unique.len() {
        0 => String::new(),
        1 => unique[0].to_string(),
        _ => format!(
            "{} y {}",
            unique[..unique.len() - 1].join(", "),
            unique[unique.len() - 1]
        ),
    }
}

fn name(task: &Task) -> String {
    stage_name(&task.role, &task.title)
}

#[derive(Default)]
struct Seen {
    done: HashMap<String, HashSet<i64>>,
    rounds: HashMap<String, HashMap<i64, i64>>,
}

/// Remembers which tasks were already reported as done or returned, per swarm.
#[derive(Default)]
pub struct Progress {
    seen: Mutex<Seen>,
}

impl Progress {
    /// Empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets a swarm (after its final summary).
    pub fn forget(&self, swarm: &str) {
        let mut seen = self.seen.lock().unwrap_or_else(|error| error.into_inner());
        seen.done.remove(swarm);
        seen.rounds.remove(swarm);
    }

    /// New phrases since the last pass; the first one only takes note.
    pub fn check(&self, swarm: &str, tasks: &[Task]) -> Vec<String> {
        let top: Vec<&Task> = tasks.iter().filter(|task| task.parent == 0).collect();
        let done_now: HashSet<i64> = top
            .iter()
            .filter(|task| task.state == "done")
            .map(|task| task.id)
            .collect();
        let rounds_now: HashMap<i64, i64> = top.iter().map(|task| (task.id, task.rounds)).collect();
        let mut seen = self.seen.lock().unwrap_or_else(|error| error.into_inner());
        let Some(previous_rounds) = seen.rounds.get(swarm) else {
            seen.done.insert(swarm.to_string(), done_now);
            seen.rounds.insert(swarm.to_string(), rounds_now);
            return Vec::new();
        };
        let previous_done = seen.done.get(swarm).cloned().unwrap_or_default();
        let mut news = Vec::new();
        for task in &top {
            if task.rounds > previous_rounds.get(&task.id).copied().unwrap_or(0)
                && task.state != "done"
            {
                news.push(format!(
                    "Enjambre: devolvieron {} para corregir (ronda {} de {MAX_ROUNDS}).",
                    name(task),
                    task.rounds
                ));
            }
        }
        let finished: Vec<&Task> = top
            .iter()
            .copied()
            .filter(|task| done_now.contains(&task.id) && !previous_done.contains(&task.id))
            .collect();
        if !finished.is_empty() && done_now.len() < top.len() {
            news.push(Self::stage_text(&finished, &top));
        }
        seen.done.insert(swarm.to_string(), done_now);
        seen.rounds.insert(swarm.to_string(), rounds_now);
        news
    }

    fn stage_text(finished: &[&Task], top: &[&Task]) -> String {
        let ids: HashSet<i64> = finished.iter().map(|task| task.id).collect();
        let names: Vec<String> = finished.iter().map(|task| name(task)).collect();
        let unique: HashSet<&str> = names.iter().map(String::as_str).collect();
        let verb = if unique.len() == 1 {
            "terminó"
        } else {
            "terminaron"
        };
        let text = format!("Enjambre: {verb} {}.", join(&names));
        let next: Vec<&Task> = top
            .iter()
            .copied()
            .filter(|task| task.state != "done" && task.deps.iter().any(|dep| ids.contains(dep)))
            .collect();
        if !next.is_empty() {
            let next_names: Vec<String> = next.iter().map(|task| name(task)).collect();
            let unique: HashSet<&str> = next_names.iter().map(String::as_str).collect();
            let start = if unique.len() == 1 {
                "empieza"
            } else {
                "empiezan"
            };
            return format!("{text} Ahora {start} {}.", join(&next_names));
        }
        let running: Vec<&Task> = top
            .iter()
            .copied()
            .filter(|task| task.state == "claimed")
            .collect();
        if running.is_empty() {
            return text;
        }
        let running_names: Vec<String> = running.iter().map(|task| name(task)).collect();
        let unique: HashSet<&str> = running_names.iter().map(String::as_str).collect();
        let still = if unique.len() == 1 {
            "sigue en curso"
        } else {
            "siguen en curso"
        };
        let who = join(&running_names);
        let mut characters = who.chars();
        let first = characters
            .next()
            .map(|character| character.to_uppercase().collect::<String>())
            .unwrap_or_default();
        format!("{text} {first}{} {still}.", characters.as_str())
    }
}

#[cfg(test)]
mod tests;
