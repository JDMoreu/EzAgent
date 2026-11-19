//! ECO-2c: huellas, ejecutor secuencial, reinicio, margen, eventos y tarjetas.

use std::sync::{Arc, Barrier};
use std::thread;

use ez_arbiter::{Arbiter, Command, Outcome, Rejection, Verdict};
use ez_team::{Role, Seat};

use super::support::{kind, FakeWriter, Fixture};

/// Id de la enésima tarjeta abierta de un tipo en la bitácora.
fn card_id(f: &Fixture, task: &str, type_name: &str, nth: usize) -> String {
    let opened: Vec<_> = f
        .bitacora(task)
        .into_iter()
        .filter(|event| kind(event) == "card_opened" && event["kind"]["kind"] == type_name)
        .collect();
    opened[nth]["kind"]["id"]
        .as_str()
        .expect("id de tarjeta")
        .to_string()
}

/// Informes de una ronda sin cerrarla.
fn all_reports(f: &Fixture, round: u32) {
    f.launch(Role::Developer);
    f.report(&format!("dev-r{round}-1"), Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    f.report(&format!("qa-r{round}"), Verdict::Pass, None, None)
        .expect("informe del QA");
    f.launch(Role::QaValidator);
    f.report(&format!("valqa-r{round}"), Verdict::Pass, Some(3), Some(3))
        .expect("informe del validador");
}

/// Prepara una tarea con criterios elegidos y el plan aprobado en la ronda 1.
fn setup_with_criteria(f: &Fixture, body: &str) {
    f.arbiter
        .execute(
            &Fixture::elephant(),
            Command::TaskNew {
                title: "Margen".into(),
                brief: f.brief(),
                criteria: f.criteria(body),
            },
        )
        .expect("proponer");
    f.arbiter.approve_task("s1").expect("aprobar");
    f.team(Fixture::tasks());
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda");
    f.launch(Role::Planner);
    f.report("plan-r1", Verdict::Pass, None, None)
        .expect("plan");
}

#[test]
fn fingerprint_detects_modified_and_untracked_but_ignores_ez() {
    let edited = Fixture::new("huella-editado");
    let _ = edited.setup();
    let path = edited.folder.join("encargo.md");
    let original = std::fs::read_to_string(&path).expect("encargo original");
    std::fs::write(&path, format!("{original}\ncambio\n")).expect("editar encargo");
    let error = edited
        .arbiter
        .execute(&Fixture::elephant(), Command::Verify)
        .expect_err("el cambio se detecta");
    assert_eq!(error.rule, "R11");
    assert_eq!(edited.snapshot()["pause_reason"], "cambios_fuera_de_ronda");

    let added = Fixture::new("huella-nuevo");
    let _ = added.setup();
    added.write("nuevo.txt", "sin seguimiento\n");
    let error = added
        .arbiter
        .execute(&Fixture::elephant(), Command::Verify)
        .expect_err("el archivo nuevo se detecta");
    assert_eq!(error.rule, "R11");

    let ignored = Fixture::new("huella-ez");
    let task = ignored.propose();
    std::fs::write(
        ignored
            .folder
            .join(".ez")
            .join("tareas")
            .join(&task)
            .join("interno.txt"),
        "cambio privado\n",
    )
    .expect("escribir bajo .ez");
    ignored.arbiter.approve_task("s1").expect("aprobar");
    ignored.team(Fixture::tasks());
    ignored.arbiter.approve_team("s1").expect("aprobar equipo");
    ignored
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect(".ez/ se ignora en la huella");
}

#[test]
fn files_are_attributed_to_the_developer_role_with_parallel_developers() {
    let f = Fixture::new("atribucion-paralela");
    let task = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar");
    let seats: Vec<Seat> = vec![
        Fixture::seat(Role::Planner, 1),
        Fixture::seat(Role::Developer, 2),
        Fixture::seat(Role::Qa, 1),
        Fixture::seat(Role::QaValidator, 1),
    ];
    f.team(seats);
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda");
    f.launch(Role::Planner);
    f.report("plan-r1", Verdict::Pass, None, None)
        .expect("plan");

    f.launch(Role::Developer);
    f.write("dev-uno.txt", "uno\n");
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("dev uno");
    f.launch(Role::Developer);
    f.write("dev-dos.txt", "dos\n");
    f.report("dev-r1-2", Verdict::Pass, None, None)
        .expect("dev dos");
    f.launch(Role::Qa);
    f.report("qa-r1", Verdict::Pass, None, None).expect("qa");
    f.launch(Role::QaValidator);
    f.report("valqa-r1", Verdict::Pass, Some(3), Some(3))
        .expect("validador");
    f.close_round().expect("cerrar ronda");

    let state = f.state_json(&task);
    let attributed = state["rounds"][0]["attribution"]["developer"]
        .as_array()
        .expect("atribución de developer");
    let names: Vec<&str> = attributed
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(names.contains(&"dev-uno.txt"), "{names:?}");
    assert!(names.contains(&"dev-dos.txt"), "{names:?}");
}

#[test]
fn elephant_changes_between_works_pause_and_resume_resets_the_streak() {
    let f = Fixture::new("elefante-racha");
    let _ = f.setup();
    super::task::clean_round(&f, 1);
    assert_eq!(f.snapshot()["clean_streak"], 1);

    f.write("fuera.txt", "el elefante escribió\n");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect_err("cambio fuera de ronda");
    assert_eq!(error.rule, "R11");
    assert_eq!(f.snapshot()["status"], "paused");

    f.arbiter.resume("s1").expect("reanudar");
    assert_eq!(f.snapshot()["status"], "between_rounds");
    assert_eq!(f.snapshot()["clean_streak"], 0);
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("la huella actual quedó registrada al reanudar");
}

#[test]
fn task_close_rejects_a_project_changed_since_the_last_clean_round() {
    let f = Fixture::new("cierre-cambiado");
    let _ = f.setup();
    super::task::clean_round(&f, 1);
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    f.launch(Role::Qa);
    f.report("qa-r2", Verdict::Pass, None, None).expect("qa");
    f.launch(Role::QaValidator);
    // El validador cambia el proyecto: la ronda queda sucia y la huella de la
    // última ronda limpia sigue siendo la de la ronda 1.
    f.write("extra.txt", "cambio mientras trabajaba el validador\n");
    f.report("valqa-r2", Verdict::Pass, Some(3), Some(3))
        .expect("validador");
    f.close_round().expect("cerrar ronda 2 sucia");

    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::TaskClose)
        .expect_err("el proyecto cambió desde la última ronda limpia");
    assert_eq!(error.rule, "R10");
    assert!(error
        .message
        .contains("El proyecto cambió desde la última ronda limpia"));
}

/// Dos llamadas simultáneas: exactamente una cambia el estado.
fn only_one_wins(results: [Result<Outcome, Rejection>; 2], rejected_rule: &str) {
    let ok = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(ok, 1, "una sola transición: {results:?}");
    let rejected = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("un rechazo");
    assert_eq!(rejected.rule, rejected_rule);
}

/// Lanza el mismo comando desde dos hilos a la vez.
fn twice(arbiter: Arbiter, command: Command) -> [Result<Outcome, Rejection>; 2] {
    let barrier = Arc::new(Barrier::new(2));
    let a = arbiter.clone();
    let b1 = barrier.clone();
    let first = command.clone();
    let one = thread::spawn(move || {
        b1.wait();
        a.execute(&Fixture::elephant(), first)
    });
    let b2 = barrier.clone();
    let two = thread::spawn(move || {
        b2.wait();
        arbiter.execute(&Fixture::elephant(), command)
    });
    [one.join().expect("hilo 1"), two.join().expect("hilo 2")]
}

#[test]
fn two_round_starts_only_open_one_round() {
    let f = Fixture::new("conc-rondas");
    let _ = f.setup();
    super::task::clean_round(&f, 1);
    only_one_wins(twice(f.arbiter.clone(), Command::RoundStart), "R4");
    assert_eq!(f.snapshot()["round"], 2);
}

#[test]
fn two_qa_launches_only_launch_one_fish() {
    let f = Fixture::new("conc-qa");
    let _ = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("dev");
    let command = Command::FishNew {
        role: Role::Qa,
        brief: f.brief(),
    };
    only_one_wins(twice(f.arbiter.clone(), command), "R5");
}

#[test]
fn two_round_closes_only_close_one_round() {
    let f = Fixture::new("conc-cierre");
    let _ = f.setup();
    all_reports(&f, 1);
    only_one_wins(twice(f.arbiter.clone(), Command::RoundClose), "R9");
}

#[test]
fn restart_pauses_the_task_and_fails_pending_work() {
    let f = Fixture::new("reinicio-trabajo");
    let _ = f.setup();
    f.launch(Role::Developer);
    let task = f.fish_key("dev-r1-1").task;
    let reopened = Arbiter::new(
        f.data_dir.clone(),
        f.launcher.clone(),
        Arc::new(FakeWriter),
        f.clock.clone(),
    )
    .expect("reabrir el árbitro");
    reopened.register_space(Fixture::space(&f.folder));

    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "reinicio");
    let failed = state["work"]
        .as_array()
        .expect("trabajos")
        .iter()
        .any(|item| item["state"] == "failed" && item["key"]["fish"] == "dev-r1-1");
    assert!(failed, "el trabajo pendiente pasa a failed");
    assert!(kind(&f.bitacora(&task)[12]) == "fish_failed");
}

#[test]
fn more_time_extends_limits_and_pauses_do_not_consume_time() {
    let extra = Fixture::new("margen-rondas");
    setup_with_criteria(
        &extra,
        "[validacion]\nrondas_maximas = 2\n\
         [[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\ncomando = [\"git\", \"--version\"]\n",
    );
    super::task::clean_round(&extra, 1);
    extra
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    super::task::clean_round(&extra, 2);
    assert_eq!(extra.snapshot()["pause_reason"], "tope_rondas");

    extra.arbiter.more_time("s1").expect("dar más margen");
    let task_id = extra.snapshot()["task"]
        .as_str()
        .expect("tarea")
        .to_string();
    let state = extra.state_json(&task_id);
    assert_eq!(state["status"], "between_rounds");
    assert_eq!(state["extra_rounds"], 2);
    assert_eq!(state["extra_hours"], 1.0);
    extra
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 3 dentro del nuevo margen");
    all_reports(&extra, 3);
    extra.close_round().expect("cerrar ronda 3 sin tope");
    assert_ne!(extra.snapshot()["pause_reason"], "tope_rondas");

    let clock = Fixture::new("margen-tiempo");
    let _ = clock.setup();
    clock.clock.advance(4000.0);
    clock
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::Pause {
                reason: "espera".into(),
            },
        )
        .expect("pausar");
    clock.clock.advance(4000.0);
    clock.arbiter.resume("s1").expect("reanudar");
    super::task::clean_round(&clock, 1);
    assert_ne!(clock.snapshot()["pause_reason"], "tope_tiempo");
}

#[test]
fn sequence_grows_and_snapshot_reports_last_seq() {
    let f = Fixture::new("seq");
    let _ = f.propose();
    let first = f.snapshot()["last_seq"].as_u64().expect("last_seq");
    assert!(first >= 2);
    f.arbiter.approve_task("s1").expect("aprobar");
    let second = f.snapshot()["last_seq"].as_u64().expect("last_seq");
    assert!(second > first, "{first} → {second}");
    let seqs: Vec<u64> = f
        .bitacora(f.snapshot()["task"].as_str().expect("tarea"))
        .iter()
        .map(|event| event["seq"].as_u64().expect("seq"))
        .collect();
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn card_action_dispatches_each_type_and_rejects_a_wrong_action() {
    let f = Fixture::new("tarjetas");
    let task = f.propose();
    let task_card = card_id(&f, &task, "task", 0);
    f.arbiter
        .card_action(&task_card, "approve", None)
        .expect("aprobar tarea desde la tarjeta");
    assert_eq!(f.state_json(&task)["status"], "approved");

    f.team(Fixture::tasks());
    let team_card = card_id(&f, &task, "team", 0);
    let error = f
        .arbiter
        .card_action(&team_card, "bailar", None)
        .expect_err("acción inválida");
    assert_eq!(error.rule, "R12");
    assert!(error.message.contains("no corresponde"));
    f.arbiter
        .card_action(&team_card, "reject", Some("cambia el equipo"))
        .expect("rechazar equipo");
    assert_eq!(f.state_json(&task)["team_status"], "rejected");

    let pause = Fixture::new("tarjeta-pausa");
    let task = pause.propose();
    pause.arbiter.approve_task("s1").expect("aprobar");
    pause.team(Fixture::tasks());
    pause.arbiter.approve_team("s1").expect("aprobar equipo");
    pause
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::Pause {
                reason: "espera".into(),
            },
        )
        .expect("pausar");
    let pause_card = card_id(&pause, &task, "pause", 0);
    pause
        .arbiter
        .card_action(&pause_card, "more_time", None)
        .expect("más margen desde la tarjeta");
    assert_ne!(pause.snapshot()["status"], "paused");
}
