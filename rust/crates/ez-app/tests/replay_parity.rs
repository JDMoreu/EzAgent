//! Paridad de los escenarios 01-25 contra las trazas doradas de Python.
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
use std::sync::Mutex;

use ez_app::replay::{causal_diff, Paths};

/// Los tres bloques corren de uno en uno: cada escenario ya usa varios hilos y
/// procesos, y en paralelo los avisos hablados cambian de orden en la traza.
static SERIAL: Mutex<()> = Mutex::new(());

/// Escenarios de esta tarea, en orden.
const SCENARIOS: [&str; 6] = [
    "01-greeting",
    "02-time-tool",
    "03-local-shortcuts",
    "04-split-phrase",
    "05-barge-in",
    "06-echo-discarded",
];

/// Escenarios de F7-01b: agentes, confirmaciones, taint, memoria y errores.
const SCENARIOS_B: [&str; 13] = [
    "07-delegate-opencode",
    "08-chain-planner-developer",
    "09-agent-blocked",
    "10a-delete-confirm-yes",
    "10b-delete-confirm-no",
    "11-taint-web-write",
    "12-memory",
    "13a-skill-script",
    "13b-skill-script-no-python",
    "14a-llm-http-errors",
    "14b-llm-disconnect",
    "14c-llm-timeout",
    "15-anthropic-tools",
];

/// Escenarios 16-25 (F7-01c): topes, marcado, enjambre, chat, menú y degradación.
const SCENARIOS_16_25: [&str; 12] = [
    "16-tool-limits",
    "17-tool-markup",
    "18-swarm-delivery",
    "19-swarm-quota",
    "20-swarm-incidents",
    "21-chat-panel",
    "22-pause-wake",
    "23-menu-settings",
    "24-model-match",
    "25a-no-whisper",
    "25b-no-kokoro",
    "25c-no-smart-turn",
];

/// Corre un escenario y compara su orden causal con la traza dorada.
fn run_against_golden(stem: &str, paths: &Paths, out_dir: &std::path::Path) {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_ezagent-replay"));
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

#[test]
fn scenarios_01_to_06_follow_the_golden_trace() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let paths = Paths::discover().expect("rutas del arnés");
    let out_dir =
        std::env::temp_dir().join(format!("ezagent-replay-parity-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("carpeta de trazas");
    for stem in SCENARIOS {
        run_against_golden(stem, &paths, &out_dir);
    }
}

#[test]
fn scenarios_07_to_15_follow_the_golden_trace() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let paths = Paths::discover().expect("rutas del arnés");
    let out_dir =
        std::env::temp_dir().join(format!("ezagent-replay-parity-b-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("carpeta de trazas");
    for stem in SCENARIOS_B {
        run_against_golden(stem, &paths, &out_dir);
    }
}

/// Los escenarios del enjambre necesitan el binario `ezs` del mismo build.
fn ensure_ezs(paths: &Paths) {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.join("ezs.exe")));
    if beside.as_ref().is_some_and(|path| path.is_file()) {
        return;
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "-q", "-p", "ez-swarm", "--bin", "ezs"])
        .current_dir(paths.root.join("rust"))
        .status()
        .expect("cargo para compilar ezs");
    assert!(status.success(), "no pude compilar el binario ezs");
}

#[test]
fn scenarios_16_to_25_follow_the_golden_trace() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let paths = Paths::discover().expect("rutas del arnés");
    ensure_ezs(&paths);
    let out_dir =
        std::env::temp_dir().join(format!("ezagent-replay-parity-c-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("carpeta de trazas");
    for stem in SCENARIOS_16_25 {
        run_against_golden(stem, &paths, &out_dir);
    }
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Líneas normalizadas con el normalizador de paridad.
fn normalized(text: &str) -> String {
    ez_parity::trace::normalize(text).join("\n")
}
