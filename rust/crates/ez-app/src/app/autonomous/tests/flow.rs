//! Recorridos del bucle: feliz, ronda sucia, verificador fuera de su carpeta,
//! reposo sin `fin`, topes y reinicio.

use ez_agent::autonomous::TaskState;

use super::{clean_verdict, dirty_verdict, token_of, Harness, CLAUDE_BASH, CODEX_EDIT};

/// Hallazgos sin parecido entre sí (para que el churn no dispare antes).
const ROUND_PROBLEMS: [&str; 5] = [
    "el token vence antes de tiempo",
    "falta la prueba de humo del arranque",
    "el registro no cierra el archivo",
    "la ruta absoluta se rompe en Windows",
    "el error de red no se informa al usuario",
];

#[test]
fn happy_path_approves_verifies_twice_and_writes_the_summary() {
    let h = Harness::new();
    let id = h.start();
    let encargo = std::fs::read_to_string(h.task_dir(&id).join("compartido").join("encargo.md"))
        .expect("encargo");
    assert!(encargo.contains("fin-1.md"), "{encargo}");
    assert!(!encargo.contains("fin-N.md"), "{encargo}");
    assert!(h.wait_launch(1), "se lanzó el ejecutor");
    h.port.set("t1", "blocked", CLAUDE_BASH);
    assert!(h.wait_keys(1), "se aprobó el permiso");
    assert_eq!(h.port.keys_sent()[0].1, vec!["1".to_string()]);
    assert!(h.loop_.covers("t1"));
    h.write_fin(&id, 1);
    assert!(h.wait_launch(2), "se lanzó el primer verificador");
    let first = h.port.last_launch().expect("verificador 1");
    assert_eq!(first.agent, "claude");
    assert!(first.task.contains("verificacion-1"));
    h.write_verdict(
        &id,
        1,
        clean_verdict(&token_of(&first.task), "cargo build: ok"),
    );
    h.port.set_state(&first.address, "done");
    assert!(h.wait_launch(3), "se lanzó el segundo verificador");
    let second = h.port.last_launch().expect("verificador 2");
    h.write_verdict(
        &id,
        2,
        clean_verdict(&token_of(&second.task), "cargo test: 12 passed"),
    );
    h.port.set_state(&second.address, "done");
    assert!(h.wait_state(TaskState::Done));
    let summary = std::fs::read_to_string(h.task_dir(&id).join("privado").join("resumen.md"))
        .expect("resumen");
    assert!(summary.contains("cargo test: 12 passed"));
    assert!(summary.contains("Aprobaciones: 1"));
    let says = h.says();
    assert!(
        says.iter()
            .any(|text| text.starts_with("Terminé «arreglar la API»")),
        "{says:?}"
    );
    assert_eq!(h.port.killed().len(), 2, "se cierran los dos verificadores");
    assert!(h.bitacora(&id).contains("\"event\":\"approve\""));
    assert!(h.audit_text().contains("autonomous.approve"));
}

#[test]
fn a_dirty_round_writes_and_sends_the_correction() {
    let h = Harness::new();
    let id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "blocked", CLAUDE_BASH);
    assert!(h.wait_keys(1));
    h.write_fin(&id, 1);
    assert!(h.wait_launch(2));
    let verifier = h.port.last_launch().expect("verificador");
    h.write_verdict(
        &id,
        1,
        dirty_verdict(&token_of(&verifier.task), "la API no valida el token"),
    );
    h.port.set_state(&verifier.address, "done");
    assert!(
        h.wait(|| h
            .port
            .sends()
            .iter()
            .any(|(_, text)| text.contains("correccion-1.md"))),
        "se mandó la corrección"
    );
    let correction =
        std::fs::read_to_string(h.task_dir(&id).join("compartido").join("correccion-1.md"))
            .expect("correccion-1.md");
    assert!(correction.contains("la API no valida el token"));
    let (_, text) = h
        .port
        .sends()
        .into_iter()
        .find(|(_, text)| text.contains("correccion-1.md"))
        .expect("envío");
    assert!(text.contains("fin-2.md"), "{text}");
    assert!(
        h.wait(|| h
            .loop_
            .status()
            .is_some_and(|s| s.state == TaskState::Running)),
        "estado: {:?} lanzamientos: {:?}",
        h.loop_.status(),
        h.port.launches()
    );
}

#[test]
fn a_verifier_that_tries_to_edit_outside_its_folder_pauses() {
    let h = Harness::new();
    let id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "blocked", CLAUDE_BASH);
    assert!(h.wait_keys(1));
    h.write_fin(&id, 1);
    assert!(h.wait_launch(2));
    let verifier = h.port.last_launch().expect("verificador");
    h.port.set(&verifier.address, "blocked", CODEX_EDIT);
    assert!(h.wait(|| h.paused_reason().is_some()), "pausa");
    let reason = h.paused_reason().expect("motivo");
    assert!(
        reason.contains("fuera de las carpetas permitidas"),
        "{reason}"
    );
    assert_eq!(
        h.port.keys_sent().len(),
        1,
        "no se enviaron teclas al verificador"
    );
}

#[test]
fn an_executor_idle_without_fin_pauses_after_three_minutes() {
    let h = Harness::new();
    let _id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "idle", "Trabajo a medias.\n");
    // Deja que el bucle registre el primer instante en reposo y luego avanza.
    std::thread::sleep(std::time::Duration::from_millis(50));
    h.clock.advance(200.0);
    assert!(
        h.wait(|| h.paused_reason().is_some()),
        "pausa por reposo: {:?} t1={:?}",
        h.loop_.status(),
        h.port.state_of("t1")
    );
    let reason = h.paused_reason().expect("motivo");
    assert!(reason.contains("se detuvo sin terminar"), "{reason}");
    assert!(h.port.keys_sent().is_empty());
}

#[test]
fn the_round_cap_pauses_and_resume_extends_by_three() {
    let h = Harness::new();
    let id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "blocked", CLAUDE_BASH);
    assert!(h.wait_keys(1));
    for round in 1..=5 {
        h.port.set("t1", "idle", "");
        h.write_fin(&id, round);
        assert!(h.wait_launch(round as usize + 1), "verificador {round}");
        let verifier = h.port.last_launch().expect("verificador");
        h.write_verdict(
            &id,
            round,
            dirty_verdict(
                &token_of(&verifier.task),
                ROUND_PROBLEMS[round as usize - 1],
            ),
        );
        h.port.set_state(&verifier.address, "done");
        assert!(
            h.wait(|| {
                h.loop_
                    .status()
                    .is_some_and(|summary| summary.state == TaskState::Paused)
                    || summary_round(&h, round + 1)
            }),
            "ronda {round}"
        );
        if h.paused_reason().is_some() {
            break;
        }
    }
    let reason = h.paused_reason().expect("pausa por rondas");
    assert_eq!(reason, "se agotaron las rondas");
    h.loop_.resume_task(0, 0).expect("sigue");
    assert!(h.wait_state(TaskState::Running));
    assert_eq!(h.loop_.status().expect("estado").max_rounds, 8);
}

fn summary_round(h: &Harness, round: i64) -> bool {
    h.loop_
        .status()
        .is_some_and(|summary| summary.round == round)
}

#[test]
fn a_restart_leaves_the_task_paused_and_says_why() {
    let h = Harness::new();
    let id = super::fixed_id();
    h.seed_running(&id);
    assert!(h.loop_.restore_after_restart(), "se restauró");
    let summary = h.loop_.status().expect("estado");
    assert_eq!(summary.state, TaskState::Paused);
    assert_eq!(summary.pause_reason.as_deref(), Some("la app se reinició"));
    let says = h.says();
    assert!(
        says.iter().any(|text| text.contains("la app se reinició")),
        "{says:?}"
    );
    assert_eq!(summary.round, 1);
    assert!(!h.loop_.running_or_verifying());
}
