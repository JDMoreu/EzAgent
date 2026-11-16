//! Artefactos de 9.3 en un directorio temporal.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use crate::autonomous::artifacts::{self, Encargo, Event, TaskDir};
use crate::autonomous::state::{AutonomousState, Mode};
use crate::autonomous::verdict::{Finding, Severity, Verdict};

const VALID_VERDICT: &str = r#"{
  "cumple": true,
  "criterios": [{"n": 1, "cumple": true, "evidencia": "cargo test"}],
  "hallazgos": []
}"#;

fn temp_root(label: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "ez-autonomo-{}-{label}-{count}",
        std::process::id()
    ))
}

fn state() -> AutonomousState {
    AutonomousState::new(
        &artifacts::new_id(1_765_000_000.0),
        Mode::Single,
        1_765_000_000.0,
        5,
        120,
    )
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).expect("leer archivo")
}

fn cleanup(task: &TaskDir, root: &std::path::Path) {
    for name in [
        "encargo.md",
        "criterios.md",
        "estado.json",
        "bitacora.jsonl",
        "correccion-1.md",
        "ronda-1/veredicto.json",
    ] {
        let _ = std::fs::remove_file(task.path().join(name));
    }
    let _ = std::fs::remove_dir(task.path().join("ronda-1"));
    let _ = std::fs::remove_dir(task.path());
    let _ = std::fs::remove_dir(root);
}

#[test]
fn creates_the_folder_and_its_initial_artifacts() {
    let root = temp_root("crear");
    let state = state();
    assert!(state.id.len() > 12);
    let encargo = Encargo {
        task: "migrar la API".to_string(),
        space: "proyecto".to_string(),
        workdirs: vec!["C:/proyecto".to_string()],
    };
    let task = TaskDir::create(
        &root,
        &encargo,
        &["compila".to_string(), "pasan las pruebas".to_string()],
        &state,
    )
    .expect("crear artefactos");

    let encargo_text = read(&task.path().join("encargo.md"));
    assert!(encargo_text.contains("migrar la API"));
    assert!(encargo_text.contains("Espacio: proyecto"));
    assert!(encargo_text.contains("- C:/proyecto"));

    let criteria_text = read(&task.path().join("criterios.md"));
    assert!(criteria_text.contains("1. compila"));
    assert!(criteria_text.contains("2. pasan las pruebas"));

    let estado: Value =
        serde_json::from_str(&read(&task.path().join("estado.json"))).expect("JSON válido");
    assert_eq!(estado["id"], state.id.as_str());
    assert_eq!(estado["mode"], "single");
    assert_eq!(estado["state"], "preparing");
    assert_eq!(estado["round"], 1);
    assert_eq!(estado["max_rounds"], 5);
    assert_eq!(estado["approvals"], 0);
    assert_eq!(estado["clean_streak"], 0);
    assert_eq!(estado["executor"], Value::Null);
    assert_eq!(estado["team"], Value::Null);
    assert_eq!(estado["pause_reason"], Value::Null);
    assert!(estado["started_at"].as_f64().is_some());
    assert!(estado["deadline"].as_f64().is_some());
    assert!(estado.get("verify_failures").is_none());

    cleanup(&task, &root);
}

#[test]
fn state_is_written_atomically_and_reloaded() {
    let root = temp_root("estado");
    let mut state = state();
    let task = TaskDir::create(
        &root,
        &Encargo {
            task: "tarea".to_string(),
            space: "espacio".to_string(),
            workdirs: Vec::new(),
        },
        &["criterio".to_string()],
        &state,
    )
    .expect("crear artefactos");
    state.executor_started("addr-1", "codex");
    task.write_state(&state).expect("escribir estado");
    let loaded = task.load_state().expect("leer estado");
    assert_eq!(
        loaded.executor.as_ref().map(|e| e.agent.as_str()),
        Some("codex")
    );
    assert_eq!(loaded.state.as_str(), "running");
    let leftovers = std::fs::read_dir(task.path())
        .expect("leer carpeta")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
    cleanup(&task, &root);
}

#[test]
fn bitacora_is_one_json_line_per_event() {
    let root = temp_root("bitacora");
    let task = TaskDir::create(
        &root,
        &Encargo {
            task: "tarea".to_string(),
            space: "espacio".to_string(),
            workdirs: Vec::new(),
        },
        &["criterio".to_string()],
        &state(),
    )
    .expect("crear artefactos");
    task.log_at(
        Event::Start,
        &json!({"task": "tarea"}),
        "2026-11-16T12:00:00-05:00",
    )
    .expect("bitácora");
    task.log(Event::VerifyStart, &json!({"round": 1}))
        .expect("bitácora");
    let text = read(&task.path().join("bitacora.jsonl"));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    let first: Value = serde_json::from_str(lines[0]).expect("línea JSON");
    assert_eq!(first["event"], "start");
    assert_eq!(first["ts"], "2026-11-16T12:00:00-05:00");
    assert_eq!(first["detail"]["task"], "tarea");
    let second: Value = serde_json::from_str(lines[1]).expect("línea JSON");
    assert_eq!(second["event"], "verify_start");
    cleanup(&task, &root);
}

#[test]
fn reads_and_validates_the_round_verdict() {
    let root = temp_root("veredicto");
    let task = TaskDir::create(
        &root,
        &Encargo {
            task: "tarea".to_string(),
            space: "espacio".to_string(),
            workdirs: Vec::new(),
        },
        &["criterio".to_string()],
        &state(),
    )
    .expect("crear artefactos");
    let path = task.prepare_round(1).expect("carpeta de ronda");
    assert_eq!(path, task.verdict_path(1));
    std::fs::write(&path, VALID_VERDICT).expect("veredicto");
    assert!(task.read_verdict(1).expect("leer veredicto").clean());
    std::fs::write(&path, "{ roto").expect("veredicto roto");
    assert!(task.read_verdict(1).is_err());
    assert!(task.read_verdict(2).is_err());
    cleanup(&task, &root);
}

#[test]
fn writes_the_correction_with_only_blocking_findings() {
    let root = temp_root("correccion");
    let task = TaskDir::create(
        &root,
        &Encargo {
            task: "tarea".to_string(),
            space: "espacio".to_string(),
            workdirs: Vec::new(),
        },
        &["criterio".to_string()],
        &state(),
    )
    .expect("crear artefactos");
    let verdict = Verdict {
        cumple: false,
        criteria: Vec::new(),
        findings: vec![
            Finding {
                severity: Severity::Blocking,
                criterion: Some(2),
                problem: "no compila".to_string(),
                evidence: "error E0308".to_string(),
            },
            Finding {
                severity: Severity::Minor,
                criterion: None,
                problem: "estilo".to_string(),
                evidence: String::new(),
            },
        ],
    };
    let path = task.write_correction(1, &verdict).expect("corrección");
    assert_eq!(path, task.correction_path(1));
    let text = read(&path);
    assert!(text.contains("Bloqueante (criterio 2): no compila"));
    assert!(text.contains("Evidencia: error E0308"));
    assert!(!text.contains("estilo"));
    cleanup(&task, &root);
}

#[test]
fn ids_have_the_documented_shape() {
    let id = artifacts::new_id(1_765_000_000.0);
    let (date, rest) = id.split_at(8);
    let (time, suffix) = rest.split_at(7);
    assert!(date.chars().all(|character| character.is_ascii_digit()));
    assert_eq!(&time[..1], "-");
    assert_eq!(suffix.len(), 5);
    assert_eq!(&suffix[..1], "-");
    assert!(suffix[1..]
        .chars()
        .all(|character| character.is_ascii_hexdigit()));
    assert!(artifacts::root().ends_with("autonomo"));
}
