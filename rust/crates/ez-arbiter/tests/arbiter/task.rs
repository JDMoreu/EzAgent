//! R1 (quién puede qué), R2 (tarea nueva), R3 (congelados), R10 y R11.

use std::path::PathBuf;

use crate::support::role;
use ez_arbiter::{Caller, Command, FishKey, Outcome, Rejection, Verdict};
use ez_team::builtin;

use super::support::Fixture;

/// Crea una tarea con criterios elegidos.
pub fn new_task(f: &Fixture, criteria: PathBuf) -> Result<Outcome, Rejection> {
    f.arbiter.execute(
        &Fixture::elephant(),
        Command::TaskNew {
            title: "Tarea de prueba".to_string(),
            brief: f.brief(),
            criteria,
        },
    )
}

/// Informes que hacen limpia la ronda `n` (que ya está abierta).
pub fn clean_round(f: &Fixture, n: u32) {
    f.launch(role(builtin::DEVELOPER));
    f.report(&format!("dev-r{n}-1"), Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(role(builtin::QA));
    f.report(&format!("qa-r{n}"), Verdict::Pass, None, None)
        .expect("informe del QA");
    f.launch(role(builtin::QA_VALIDATOR));
    f.report(&format!("valqa-r{n}"), Verdict::Pass, Some(3), Some(3))
        .expect("informe del validador");
    f.close_round().expect("cerrar ronda limpia");
}

#[test]
fn r1_fish_cannot_use_elephant_commands() {
    let f = Fixture::new("r1-elefante");
    let unknown = Caller::Fish(FishKey {
        space: "s1".into(),
        task: "t000000000000".into(),
        elephant: "e00000000".into(),
        fish: "dev-r1-1".into(),
        attempt: 1,
        parent: None,
    });
    let error = f
        .arbiter
        .execute(&unknown, Command::RoundStart)
        .expect_err("un pez no abre rondas");
    assert_eq!(error.rule, "R1");
    assert!(error.message.contains("agente principal"));
}

#[test]
fn r1_elephant_cannot_report_and_fish_is_its_own_identity() {
    let f = Fixture::new("r1-informe");
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::Report {
                verdict: Verdict::Pass,
                file: f.brief(),
                blocking: 0,
                major: 0,
                minor: 0,
                mutations_tried: None,
                mutations_caught: None,
            },
        )
        .expect_err("el elefante no informa");
    assert_eq!(error.rule, "R1");
    let task = f.setup();
    let error = f
        .arbiter
        .execute(
            &Caller::Fish(FishKey {
                space: "s1".into(),
                task,
                elephant: "e00000000".into(),
                fish: "otro".into(),
                attempt: 1,
                parent: None,
            }),
            Command::Verify,
        )
        .expect_err("pez desconocido");
    assert_eq!(error.rule, "R1");
}

#[test]
fn r2_rejects_broken_criteria() {
    let cases = [
        ("vacio", "[validacion]\nrondas_maximas = 5\n"),
        (
            "repetido",
            "[[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"--version\"]\n\
             [[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"--version\"]\n",
        ),
        ("sin-comando", "[[criterio]]\nid = \"C1\"\ncomando = []\n"),
        (
            "fuera",
            "[[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\ncarpeta = \"../fuera\"\n",
        ),
        (
            "denegado",
            "[[criterio]]\nid = \"C1\"\ncomando = [\"git\", \"reset\", \"--hard\"]\n",
        ),
    ];
    for (name, body) in cases {
        let f = Fixture::new(&format!("r2-{name}"));
        let criteria = f.criteria(body);
        let error = new_task(&f, criteria).expect_err("criterios inválidos");
        assert_eq!(error.rule, "R2", "{name}: {}", error.message);
    }
}

#[test]
fn r2_requires_git_and_only_one_active_task() {
    let no_git = Fixture::without_git("r2-git");
    let error = new_task(&no_git, no_git.default_criteria()).expect_err("sin git");
    assert_eq!(error.rule, "R2");
    assert!(error.message.contains("git init"));

    let f = Fixture::new("r2-activa");
    let task = f.propose();
    let error = new_task(&f, f.default_criteria()).expect_err("segunda tarea");
    assert_eq!(error.rule, "R2");
    assert!(f.state_json(&task)["status"].as_str() == Some("pending_approval"));
}

#[test]
fn r2_task_card_has_title_excerpt_and_criteria() {
    let f = Fixture::new("r2-tarjeta");
    let outcome = new_task(&f, f.default_criteria()).expect("tarea propuesta");
    assert!(outcome.message.contains("Tarea de prueba"));
    assert!(outcome.message.contains("Haz la tarea."));
    assert!(outcome
        .message
        .contains("- C1: Git responde → git --version"));
    let events = f.bitacora(outcome.data["task"].as_str().expect("id"));
    assert!(events
        .iter()
        .any(|event| super::support::kind(event) == "task_proposed"));
}

#[test]
fn r3_changed_criteria_pauses_on_round_start() {
    let f = Fixture::new("r3-ronda");
    let task = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar");
    f.team(Fixture::tasks());
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    let live = f
        .folder
        .join(".ez")
        .join("tareas")
        .join(&task)
        .join("criterios.toml");
    std::fs::write(&live, "[[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\n")
        .expect("modificar criterios");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect_err("criterios modificados");
    assert_eq!(error.rule, "R3");
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "criterios_modificados");
    let mut paused = None;
    for _ in 0..4 {
        let event = f
            .arbiter
            .wait_for_elephant("s1", std::time::Duration::from_millis(50))
            .expect("evento del elefante");
        if event.kind == "paused" {
            paused = Some(event);
            break;
        }
    }
    assert_eq!(paused.expect("evento de pausa").kind, "paused");
}

#[test]
fn r3_changed_criteria_pauses_on_verify() {
    let f = Fixture::new("r3-verificar");
    let task = f.setup();
    let live = f
        .folder
        .join(".ez")
        .join("tareas")
        .join(&task)
        .join("criterios.toml");
    std::fs::write(&live, "[[criterio]]\nid = \"C1\"\ncomando = [\"git\"]\n")
        .expect("modificar criterios");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::Verify)
        .expect_err("verificar con criterios modificados");
    assert_eq!(error.rule, "R3");
    assert_eq!(f.state_json(&task)["pause_reason"], "criterios_modificados");
}

#[test]
fn r10_needs_two_clean_rounds_in_a_row() {
    let f = Fixture::new("r10-dos");
    let task = f.setup();
    clean_round(&f, 1);
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::TaskClose)
        .expect_err("solo una ronda limpia");
    assert_eq!(error.rule, "R10");
    assert!(error.message.contains("llevas 1"));
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda 2");
    clean_round(&f, 2);
    let outcome = f
        .arbiter
        .execute(&Fixture::elephant(), Command::TaskClose)
        .expect("cerrar tarea");
    assert!(outcome.message.contains("2 rondas limpias"));
    assert_eq!(f.state_json(&task)["status"], "done");
}

#[test]
fn r11_elephant_changes_outside_round_pause() {
    let f = Fixture::new("r11-fuera");
    let task = f.setup();
    clean_round(&f, 1);
    f.write("cambio.txt", "el elefante escribió fuera de ronda\n");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect_err("cambios fuera de .ez");
    assert_eq!(error.rule, "R11");
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "cambios_fuera_de_ronda");
}
