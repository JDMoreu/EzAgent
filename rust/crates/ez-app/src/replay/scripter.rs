//! Guion de `agent.scenes` sobre el estado de `fake-hc` (F7-01b).
//!
//! Porta `AgentScripter` de `parity/replay/run_py.py`: `fake-hc` pinta la
//! pantalla del agente y el vigía consulta `hc ls`; este hilo hace de programa
//! `fake-agent`: deja ver `working` y después pasa a `done`, `blocked` u otra
//! escena tras `delay_ms`, anotando `agent.scene`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ez_core::metrics::monotonic;
use ez_core::pyvalue::{python_str, truthy};
use serde_json::{json, Value};

use super::hc_b::{apply_hc_scene, edit_hc, find_terminal_mut, read_hc};
use super::trace::Trace;

/// Texto de la escena `done` cuando el YAML no trae conclusión.
pub const DEFAULT_CONCLUSION: &str = "Listo. La tarea terminó correctamente.";

struct Seen {
    index: usize,
    due: Option<f64>,
}

/// Aplica el guion `agent.scenes` al estado de `fake-hc`.
pub struct AgentScripter {
    path: PathBuf,
    scenes: Vec<String>,
    delay: f64,
    conclusion: String,
    trace: Arc<Trace>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AgentScripter {
    /// Guion del bloque `agent:` del escenario.
    pub fn new(hc_path: &Path, spec: &Value, trace: Arc<Trace>) -> Self {
        let scenes = spec
            .get("scenes")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .map_or_else(
                || vec!["working".to_string(), "done".to_string()],
                |items| items.iter().map(python_str).collect(),
            );
        let delay = spec
            .get("delay_ms")
            .and_then(Value::as_f64)
            .unwrap_or(1800.0)
            / 1000.0;
        let conclusion = spec
            .get("conclusion")
            .filter(|value| truthy(value))
            .map_or_else(|| DEFAULT_CONCLUSION.to_string(), python_str);
        Self {
            path: hc_path.to_path_buf(),
            scenes,
            delay,
            conclusion,
            trace,
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Arranca el hilo `services` (solo si hay al menos dos escenas).
    pub fn start(&mut self) {
        if self.scenes.len() < 2 {
            return;
        }
        let path = self.path.clone();
        let scenes = self.scenes.clone();
        let conclusion = self.conclusion.clone();
        let delay = self.delay;
        let trace = Arc::clone(&self.trace);
        let stop = Arc::clone(&self.stop);
        let handle = std::thread::Builder::new()
            .name("services".to_string())
            .spawn(move || {
                let mut runner = Runner {
                    path: &path,
                    scenes: &scenes,
                    conclusion: &conclusion,
                    delay,
                    trace: &trace,
                    stop: &stop,
                    seen: HashMap::new(),
                };
                runner.run();
            });
        match handle {
            Ok(handle) => self.handle = Some(handle),
            Err(error) => tracing::warn!("No pude crear el hilo services: {error}"),
        }
    }

    /// Detiene el hilo (el `join(3)` del Python).
    ///
    // DECISION: sin plazo de 3 s: el bucle despierta cada 50 ms (200 ms tras
    // un error), así que el join termina enseguida.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AgentScripter {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Bucle del guion: mira el estado cada 50 ms.
struct Runner<'a> {
    path: &'a Path,
    scenes: &'a [String],
    conclusion: &'a str,
    delay: f64,
    trace: &'a Trace,
    stop: &'a AtomicBool,
    seen: HashMap<String, Seen>,
}

impl Runner<'_> {
    fn run(&mut self) {
        while !self.stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
            if self.stop.load(Ordering::SeqCst) {
                return;
            }
            if self.tick().is_err() {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }

    fn tick(&mut self) -> Result<(), String> {
        let state = read_hc(self.path);
        for (address, name, agent, scene) in self.plan(&state) {
            self.trace.record(
                "agent.scene",
                json!({"name": name, "agent": agent, "state": scene}),
            );
            edit_hc(self.path, |data| {
                let target = find_terminal_mut(data, &address).ok_or_else(|| {
                    format!("no hay terminal '{address}' en el estado de fake-hc")
                })?;
                apply_hc_scene(target, &scene, self.conclusion);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Terminales a las que toca cambiar de escena ahora.
    fn plan(&mut self, state: &Value) -> Vec<(String, String, String, String)> {
        let Some(terminals) = state.get("terminals").and_then(Value::as_array) else {
            return Vec::new();
        };
        let now = monotonic();
        let mut plan = Vec::new();
        for terminal in terminals {
            let agent = terminal
                .get("agent")
                .map(python_str)
                .unwrap_or_default()
                .to_lowercase();
            if agent.is_empty() || agent == "powershell.exe" || agent == "cmd.exe" {
                continue;
            }
            let address = terminal.get("address").map(python_str).unwrap_or_default();
            let name = terminal.get("name").map(python_str).unwrap_or_default();
            let entry = self.seen.entry(address.clone()).or_insert(Seen {
                index: 0,
                due: None,
            });
            let due = match entry.due {
                None => {
                    if terminal.get("state").map(python_str).as_deref() == Some("working") {
                        entry.due = Some(now + self.delay);
                    }
                    continue;
                }
                Some(due) => due,
            };
            if now < due || entry.index + 1 >= self.scenes.len() {
                continue;
            }
            entry.index += 1;
            plan.push((address, name, agent, self.scenes[entry.index].clone()));
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_default_to_working_and_done() {
        let trace = Trace::new(Path::new("/tmp/x"));
        let scripter = AgentScripter::new(Path::new("/tmp/hc.json"), &json!({}), trace);
        assert_eq!(scripter.scenes, vec!["working", "done"]);
        assert_eq!(scripter.conclusion, DEFAULT_CONCLUSION);
        assert!((scripter.delay - 1.8).abs() < 1e-9);
    }

    #[test]
    fn the_plan_advances_after_the_delay() {
        let trace = Trace::new(Path::new("/tmp/x"));
        let path = std::env::temp_dir().join(format!("ez-replay-plan-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"terminals":[{"address":"t1","name":"opencode","agent":"opencode","state":"working"}]}"#,
        )
        .expect("estado");
        let mut runner = Runner {
            path: &path,
            scenes: &["working".to_string(), "done".to_string()],
            conclusion: "Listo.",
            delay: 0.0,
            trace: &trace,
            stop: &AtomicBool::new(false),
            seen: HashMap::new(),
        };
        let state = read_hc(&path);
        assert!(
            runner.plan(&state).is_empty(),
            "el primer tick solo arma el plazo"
        );
        let state = read_hc(&path);
        let plan = runner.plan(&state);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].3, "done");
        let _ = std::fs::remove_file(&path);
    }
}
