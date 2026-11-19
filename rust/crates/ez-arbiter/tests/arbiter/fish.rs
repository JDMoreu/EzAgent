//! R4 (empezar ronda), R5 (peces), R6 (correcciones) y R7 (informes).

use std::fs;

use ez_arbiter::{Caller, Command, FishKey, Verdict};
use ez_team::Role;

use super::support::Fixture;

#[test]
fn r4_requires_approved_team_and_no_open_round() {
    let f = Fixture::new("r4-equipo");
    let _ = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar tarea");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect_err("equipo sin aprobar");
    assert_eq!(error.rule, "R4");
    assert!(error.message.contains("equipo"));
    f.team(Fixture::tasks());
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect_err("ronda ya abierta");
    assert_eq!(error.rule, "R4");
}

#[test]
fn r5_role_must_be_in_team() {
    let f = Fixture::new("r5-papel");
    let _ = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar tarea");
    let seats: Vec<_> = Fixture::tasks()
        .into_iter()
        .filter(|seat| seat.role != Role::Planner)
        .collect();
    f.team(seats);
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda");
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::Planner,
                brief: f.brief(),
            },
        )
        .expect_err("papel fuera del equipo");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("planner"));
}

#[test]
fn r5_qa_waits_for_developers_and_validator_for_qa() {
    let f = Fixture::new("r5-orden");
    let _ = f.setup();
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::Qa,
                brief: f.brief(),
            },
        )
        .expect_err("QA antes del desarrollador");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("dev-r1-1"));
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::QaValidator,
                brief: f.brief(),
            },
        )
        .expect_err("validador antes del QA");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("qa-r1"));
}

#[test]
fn r5_one_verifier_per_round_and_developer_limit() {
    let f = Fixture::new("r5-limites");
    let _ = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    let qa = f.launch(Role::Qa);
    assert_eq!(qa, "qa-r1");
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::Qa,
                brief: f.brief(),
            },
        )
        .expect_err("dos QA en la ronda");
    assert_eq!(error.rule, "R5");
    let other = Fixture::new("r5-cantidad");
    let _ = other.setup();
    other.launch(Role::Developer);
    let error = other
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::Developer,
                brief: other.brief(),
            },
        )
        .expect_err("demasiados desarrolladores vivos");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("desarrolladores"));
}

#[test]
fn r5_launch_request_has_identity_labels_and_instructions() {
    let f = Fixture::new("r5-lanzamiento");
    let task = f.setup();
    let fish = f.launch(Role::Developer);
    assert_eq!(fish, "dev-r1-1");
    let requests = f.launcher.requests();
    let request = requests
        .iter()
        .find(|item| item.fish == fish)
        .expect("lanzamiento del desarrollador");
    assert_eq!(
        request.hc_name,
        format!("ez-{}-dev-r1-1", &task[1..9])
    );
    assert_eq!(request.labels["ez_attempt"], "1");
    assert_eq!(request.env["EZ_ROLE"], "fish");
    assert_eq!(request.env["EZ_FISH"], fish);
    assert!(request.env["EZ_TOKEN"].len() == 32);
    assert_eq!(request.labels["ez"], "1");
    assert_eq!(request.labels["ez_space"], "s1");
    assert_eq!(request.labels["ez_task"], task);
    assert_eq!(request.labels["ez_role"], "developer");
    assert_eq!(request.labels["ez_fish"], fish);
    assert!(request.first_line.contains("Lee "));
    let round = f
        .folder
        .join(".ez")
        .join("tareas")
        .join(&task)
        .join("ronda-1");
    assert!(round.join("dev-r1-1.encargo.md").is_file());
    let instructions = round.join("dev-r1-1.instrucciones.md");
    assert!(instructions.is_file());
    let events = f.bitacora(&task);
    let launched = events
        .iter()
        .find(|event| event["kind"] == "fish_launched" && event["data"]["fish"] == fish)
        .expect("evento de lanzamiento");
    assert_eq!(launched["data"]["fish"], fish);
    assert!(launched["data"].get("token").is_none());
    assert!(!fs::read_to_string(f.data_dir.join("tareas").join(&task).join("estado.json"))
        .expect("estado")
        .contains(&request.token));
    assert!(!f.bitacora(&task).iter().any(|event| event.to_string().contains(&request.token)));
}

#[test]
fn r6_cannot_correct_a_verifier_or_a_dead_terminal() {
    let f = Fixture::new("r6-correccion");
    let _ = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishSend {
                fish: "qa-r1".to_string(),
                file: f.write("correccion.md", "Cambia esto.\n"),
            },
        )
        .expect_err("no se corrige a un verificador");
    assert_eq!(error.rule, "R6");

    let dead = Fixture::new("r6-muerto");
    let _ = dead.setup();
    dead.launch(Role::Developer);
    dead.launcher.kill(&dead.launcher.terminal("dev-r1-1"));
    let error = dead
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishSend {
                fish: "dev-r1-1".to_string(),
                file: dead.write("correccion.md", "Cambia esto.\n"),
            },
        )
        .expect_err("terminal muerta");
    assert_eq!(error.rule, "R6");
}

#[test]
fn r6_live_developer_gets_the_exact_correction_line() {
    let f = Fixture::new("r6-vivo");
    let _ = f.setup();
    f.launch(Role::Developer);
    let file = f.write("correccion.md", "Cambia esto.\n");
    let outcome = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishSend {
                fish: "dev-r1-1".to_string(),
                file,
            },
        )
        .expect("corrección al desarrollador");
    assert!(outcome.message.contains("dev-r1-1"));
    let sent = f.launcher.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1.starts_with("Corrección 1 de la ronda 1: lee "));
    assert!(sent[0].1.contains("y al terminar informa con ez report usando --file \""));
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: Role::Qa,
                brief: f.brief(),
            },
        )
        .expect_err("el desarrollador vuelve a tener trabajo");
    assert_eq!(error.rule, "R5");
}

#[test]
fn r7_verifier_reports_once_and_validator_needs_mutations() {
    let f = Fixture::new("r7-informes");
    let _ = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    f.report("qa-r1", Verdict::Pass, None, None)
        .expect("primer informe del QA");
    let error = f
        .report("qa-r1", Verdict::Pass, None, None)
        .expect_err("segundo informe del verificador");
    assert_eq!(error.rule, "R7");
    let error = f
        .report("dev-r1-1", Verdict::Pass, None, None)
        .expect_err("desarrollador sin trabajo pendiente");
    assert_eq!(error.rule, "R7");
    f.launch(Role::QaValidator);
    let error = f
        .report("valqa-r1", Verdict::Pass, None, None)
        .expect_err("validador sin mutaciones");
    assert_eq!(error.rule, "R7");
    let outside = std::env::temp_dir().join("informe-fuera-ez-arbiter.md");
    std::fs::write(&outside, "fuera del espacio").expect("archivo fuera");
    let error = f
        .arbiter
        .execute(
            &f.as_fish("valqa-r1"),
            Command::Report {
                verdict: Verdict::Pass,
                file: outside.clone(),
                blocking: 0,
                major: 0,
                minor: 0,
                mutations_tried: Some(3),
                mutations_caught: Some(3),
            },
        )
        .expect_err("informe fuera del espacio");
    assert_eq!(error.rule, "R7");
    let _ = std::fs::remove_file(outside);
}

#[test]
fn r7_unknown_fish_cannot_report() {
    let f = Fixture::new("r7-desconocido");
    let error = f
        .arbiter
        .execute(
            &Caller::Fish(FishKey {
                space: "s1".into(),
                task: "t000000000000".into(),
                fish: "dev-r9-9".into(),
                attempt: 1,
            }),
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
        .expect_err("pez desconocido");
    assert_eq!(error.rule, "R1");
}
