//! Paridad de los escenarios 01-06 contra las trazas doradas de Python.
//!
//! Cada escenario corre en un proceso propio (`ezagent-replay`), porque el
//! perfil temporal y los servicios falsos no se pueden mezclar entre corridas.
//! La comparación es la del brief: **orden causal por hilo**, ignorando los
//! eventos `driver.step` (son del arnés) y `seq` (al quitarlos deja de cuadrar).
//! Las aserciones `expect:` del YAML ya las evalúa el proceso; aquí se exige que
//! termine con código 0.
#![cfg(feature = "replay")]

use std::path::PathBuf;
use std::process::Command;

use ez_app::replay::{causal_diff, Paths};

/// Escenarios de esta tarea, en orden.
const SCENARIOS: [&str; 6] = [
    "01-greeting",
    "02-time-tool",
    "03-local-shortcuts",
    "04-split-phrase",
    "05-barge-in",
    "06-echo-discarded",
];

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Líneas normalizadas con el normalizador de paridad.
fn normalized(text: &str) -> String {
    ez_parity::trace::normalize(text).join("\n")
}

#[test]
fn scenarios_01_to_06_follow_the_golden_trace() {
    let paths = Paths::discover().expect("rutas del arnés");
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_ezagent-replay"));
    let out_dir =
        std::env::temp_dir().join(format!("ezagent-replay-parity-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("carpeta de trazas");
    for stem in SCENARIOS {
        let produced_path = out_dir.join(format!("{stem}.jsonl"));
        let status = Command::new(&binary)
            .arg("--scenario")
            .arg(stem)
            .arg("--trace-out")
            .arg(&produced_path)
            .status()
            .unwrap_or_else(|error| panic!("{stem}: no pude lanzar el arnés: {error}"));
        assert!(
            status.success(),
            "{stem}: el guión o sus aserciones fallaron"
        );
        let golden = paths.golden.join(format!("{stem}.trace.jsonl"));
        let expected = normalized(&read(&golden));
        let produced = normalized(&read(&produced_path));
        if let Some(diff) = causal_diff(&expected, &produced).expect("trazas válidas") {
            panic!(
                "{stem}: primera diferencia en {} índice {}:\n  esperado: {}\n  producido: {}",
                diff.thread, diff.index, diff.expected, diff.produced
            );
        }
    }
}
