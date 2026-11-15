//! Arnés de reproducción en Rust (F7-01a).
//!
//! Arranca el `VoiceApp` real sin ventana ni hardware con las mismas costuras
//! que `parity/replay/run_py.py`: micrófono de WAV/silencio/ruido a 30 ms por
//! bloque con el `Segmenter` y el `BargeInDetector` reales, altavoz que anota y
//! simula la duración, HUD registrador, y `fake-llm`/`fake-whisper`/`fake-hc`
//! de `ez-fakes`. Produce `trace.jsonl` con los mismos eventos y etiquetas de
//! hilo que la traza dorada de Python.
//!
//! Esta entrega cubre los escenarios 01-06; los pasos y las aserciones nuevas
//! de 07-25 se añaden a la tabla de [`driver::Step`] sin tocar el bucle.

pub mod compare;
pub mod driver;
pub mod expect;
pub mod hooks;
pub mod hud;
pub mod mic;
pub mod recording;
pub mod run;
pub mod scenario;
pub mod services;
pub mod speaker;
pub mod steps_c;
mod swarm_c;
pub mod trace;
pub mod traced_runtime;

use std::path::{Path, PathBuf};

pub use compare::{causal_diff, Diff};
pub use run::{run_path, Outcome};
pub use scenario::{scenario_paths, Paths, Scenario};

/// Escenarios elegidos por id o nombre de archivo, como `scenario_paths`.
pub fn select(paths: &Paths, selection: &[String]) -> Result<Vec<PathBuf>, String> {
    let all = scenario_paths(&paths.scenarios)?;
    if selection.is_empty() {
        return Ok(all);
    }
    let mut chosen = Vec::new();
    let mut matched: Vec<String> = Vec::new();
    for path in all {
        let scenario = Scenario::load(&path)?;
        if selection.iter().any(|wanted| wanted == &scenario.stem)
            || selection.iter().any(|wanted| wanted == &scenario.id)
        {
            matched.push(scenario.stem.clone());
            matched.push(scenario.id.clone());
            chosen.push(path);
        }
    }
    let mut missing: Vec<String> = selection
        .iter()
        .filter(|wanted| !matched.iter().any(|name| name == *wanted))
        .cloned()
        .collect();
    if !missing.is_empty() {
        missing.sort();
        return Err(format!(
            "escenarios desconocidos: {}",
            expect::py_repr(&serde_json::json!(missing))
        ));
    }
    Ok(chosen)
}

/// Lista `archivo: nombre` de todos los escenarios.
pub fn list(paths: &Paths) -> Result<Vec<(String, String)>, String> {
    let mut items = Vec::new();
    for path in scenario_paths(&paths.scenarios)? {
        let scenario = Scenario::load(&path)?;
        items.push((scenario.stem, scenario.name));
    }
    Ok(items)
}

/// `write_trace`: una línea por evento, salto `\n`, UTF-8.
pub fn write_trace(path: &Path, lines: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    let text = format!("{}\n", lines.join("\n"));
    std::fs::write(path, text)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}
