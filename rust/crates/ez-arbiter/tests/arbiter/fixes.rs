//! Correcciones de la revisión 1 (§14.1–§14.7) con git real y dobles.

use std::fs;
use std::time::Instant;

use crate::support::role;
use ez_arbiter::{Caller, Command, Delivery, FishKey, Verdict};
use ez_team::{builtin, Seat};

use super::support::Fixture;

/// Proponer, aprobar, abrir la ronda 1 (sin lanzar el planificador).
fn manual_round(f: &Fixture) -> String {
    manual_round_with(f, Fixture::tasks())
}

fn manual_round_with(f: &Fixture, seats: Vec<Seat>) -> String {
    let task = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar tarea");
    f.team(seats);
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda");
    task
}

fn report_with(
    f: &Fixture,
    key: FishKey,
    verdict: Verdict,
    blocking: u32,
    major: u32,
) -> Result<ez_arbiter::Outcome, ez_arbiter::Rejection> {
    let (_, path) = f.fish_info(&key.fish);
    fs::write(&path, "informe\n").expect("escribir informe");
    f.arbiter.execute(
        &Caller::Fish(key),
        Command::Report {
            verdict,
            file: path,
            blocking,
            major,
            minor: 0,
            mutations_tried: None,
            mutations_caught: None,
        },
    )
}

fn token_of(f: &Fixture, fish: &str) -> String {
    f.launcher
        .requests()
        .into_iter()
        .find(|request| request.fish == fish)
        .expect("lanzamiento del pez")
        .token
}

#[test]
fn old_fish_key_is_rejected_after_relaunch() {
    let f = Fixture::new("clave-vieja");
    let task = manual_round(&f);
    f.launcher.fail_with("sin terminal");
    let first = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: f.brief(),
            },
        )
        .expect_err("primer intento fallido");
    assert_eq!(first.rule, "R5");
    f.launcher.fail_with("otra vez");
    let second = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: f.brief(),
            },
        )
        .expect_err("segundo intento fallido");
    assert_eq!(second.rule, "R5");
    f.launcher.clear_fail();
    f.launch(role(builtin::PLANNER));

    let old = FishKey {
        space: "s1".into(),
        task,
        fish: "plan-r1".into(),
        attempt: 1,
    };
    let error = report_with(&f, old, Verdict::Pass, 0, 0).expect_err("clave vieja");
    assert_eq!(error.rule, "R1");
    assert!(
        error.message.contains("ya no está activo"),
        "{}",
        error.message
    );
}

#[test]
fn token_is_absent_from_disk_and_live_events() {
    let f = Fixture::new("token-invisible");
    let receiver = f.arbiter.subscribe();
    let _ = f.setup();
    f.launch(role(builtin::DEVELOPER));
    let token = token_of(&f, "dev-r1-1");
    let live: String = receiver
        .try_iter()
        .map(|event| serde_json::to_string(&event).expect("evento"))
        .collect();
    let task = f.fish_key("dev-r1-1").task;
    let log = fs::read_to_string(f.data_dir.join("tareas").join(&task).join("bitacora.jsonl"))
        .expect("bitácora");
    let state = fs::read_to_string(f.data_dir.join("tareas").join(&task).join("estado.json"))
        .expect("estado");
    assert!(!live.contains(&token));
    assert!(!log.contains(&token));
    assert!(!state.contains(&token));
}

#[test]
fn tokens_are_revoked_on_report_exit_and_stop() {
    let f = Fixture::new("revocaciones");
    let _ = f.setup();
    f.launch(role(builtin::DEVELOPER));
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(role(builtin::QA));
    let qa_token = token_of(&f, "qa-r1");
    f.report("qa-r1", Verdict::Pass, None, None)
        .expect("informe del QA");
    assert!(f.launcher.revoked().contains(&qa_token));

    f.launch(role(builtin::QA_VALIDATOR));
    let dev_token = token_of(&f, "valqa-r1");
    f.arbiter.fish_failed(&f.fish_key("valqa-r1"), "se cayó");
    assert!(f.launcher.revoked().contains(&dev_token));

    let other = Fixture::new("revocacion-stop");
    let _ = other.setup();
    other.launch(role(builtin::DEVELOPER));
    let dev_token = token_of(&other, "dev-r1-1");
    other.arbiter.stop("s1").expect("detener");
    assert!(other.launcher.revoked().contains(&dev_token));
}

#[test]
fn launcher_error_and_failed_delivery_fail_the_work() {
    let error = Fixture::new("err-lanzador");
    let _ = manual_round(&error);
    error.launcher.fail_with("no hay hc");
    let rejection = error
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: error.brief(),
            },
        )
        .expect_err("error del lanzador");
    assert_eq!(rejection.rule, "R5");
    assert!(rejection
        .message
        .contains("No pude entregar el encargo al subagente plan-r1: no hay hc"));
    assert_eq!(error.snapshot()["fish"][0]["id"], "plan-r1");

    let failed = Fixture::new("delivery-fallida");
    let _ = manual_round(&failed);
    failed
        .launcher
        .deliver(Delivery::Failed("sin terminal".into()));
    let rejection = failed
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: failed.brief(),
            },
        )
        .expect_err("entrega fallida");
    assert_eq!(rejection.rule, "R5");
    assert!(rejection.message.contains("sin terminal"));
}

#[test]
fn unconfirmed_delivery_continues_and_is_announced() {
    let f = Fixture::new("delivery-dudosa");
    let _ = manual_round(&f);
    f.launcher.deliver(Delivery::Unconfirmed);
    let outcome = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: f.brief(),
            },
        )
        .expect("entrega no confirmada");
    assert!(outcome.message.contains("No se pudo confirmar"));
    assert_eq!(f.snapshot()["fish"][0]["attempt"], 1);
}

#[test]
fn report_requires_the_exact_path_and_clean_pass() {
    let f = Fixture::new("ruta-informe");
    let _ = f.setup();
    f.launch(role(builtin::DEVELOPER));
    let key = f.fish_key("dev-r1-1");
    let outside = f.write("informe-equivocado.md", " Informe.\n");
    let error = f
        .arbiter
        .execute(
            &Caller::Fish(key.clone()),
            Command::Report {
                verdict: Verdict::Pass,
                file: outside,
                blocking: 0,
                major: 0,
                minor: 0,
                mutations_tried: None,
                mutations_caught: None,
            },
        )
        .expect_err("ruta equivocada");
    assert_eq!(error.rule, "R7");
    assert!(error.message.contains("El informe debe ir en"));

    let error = report_with(&f, key, Verdict::Pass, 0, 2).expect_err("pass con mayores");
    assert_eq!(error.rule, "R7");
    assert_eq!(
        error.message,
        "El veredicto pass no puede llevar hallazgos bloqueantes o mayores."
    );
}

#[test]
fn developer_waits_for_the_plan() {
    let f = Fixture::new("fases");
    let _ = manual_round(&f);
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::DEVELOPER),
                brief: f.brief(),
            },
        )
        .expect_err("desarrollador antes del plan");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("plan-r1"));
}

#[test]
fn reviewer_phase_is_enforced() {
    let reviewer = Fixture::new("fases-revisor");
    let mut seats = Fixture::tasks();
    seats.push(Fixture::seat(role(builtin::PLAN_REVIEWER), 1));
    let _ = manual_round_with(&reviewer, seats);
    let error = reviewer
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLAN_REVIEWER),
                brief: reviewer.brief(),
            },
        )
        .expect_err("revisor antes del plan");
    assert_eq!(error.rule, "R5");
    reviewer.launch(role(builtin::PLANNER));
    reviewer
        .report("plan-r1", Verdict::Pass, None, None)
        .expect("informe del plan");
    reviewer.launch(role(builtin::PLAN_REVIEWER));
    let error = reviewer
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::DEVELOPER),
                brief: reviewer.brief(),
            },
        )
        .expect_err("desarrollador antes del revisor");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("revplan-r1"));
    reviewer
        .report("revplan-r1", Verdict::Pass, None, None)
        .expect("informe del revisor");
    reviewer.launch(role(builtin::DEVELOPER));
}

#[test]
fn planner_is_rejected_in_later_rounds() {
    let late = Fixture::new("plan-fuera-ronda1");
    let _ = late.setup();
    super::task::clean_round(&late, 1);
    late.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    let error = late
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: late.brief(),
            },
        )
        .expect_err("planificador en ronda 2");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("El plan se hace en la ronda 1"));
}

#[test]
fn developer_cannot_be_launched_or_corrected_after_qa() {
    let f = Fixture::new("dev-tras-qa");
    let _ = f.setup();
    f.launch(role(builtin::DEVELOPER));
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(role(builtin::QA));
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::DEVELOPER),
                brief: f.brief(),
            },
        )
        .expect_err("developer tras QA");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("Ya empezó el QA"));
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishSend {
                fish: "dev-r1-1".into(),
                file: f.write("correccion.md", "cambia\n"),
            },
        )
        .expect_err("corrección tras QA");
    assert_eq!(error.rule, "R6");
    assert!(error.message.contains("Ya empezó el QA"));
}

#[test]
fn verification_only_round_requires_a_clean_previous_round() {
    let clean = Fixture::new("solo-verificacion");
    let _ = clean.setup();
    super::task::clean_round(&clean, 1);
    clean
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    clean.launch(role(builtin::QA));
    clean
        .report("qa-r2", Verdict::Pass, None, None)
        .expect("informe del QA en ronda de verificación");

    let dirty = Fixture::new("no-verificacion");
    let _ = manual_round(&dirty);
    dirty.launch(role(builtin::PLANNER));
    dirty
        .report("plan-r1", Verdict::Pass, None, None)
        .expect("plan");
    dirty.launch(role(builtin::DEVELOPER));
    dirty
        .report("dev-r1-1", Verdict::Pass, None, None)
        .expect("dev");
    dirty.launch(role(builtin::QA));
    dirty
        .report("qa-r1", Verdict::Pass, None, None)
        .expect("qa");
    dirty.launch(role(builtin::QA_VALIDATOR));
    dirty
        .report("valqa-r1", Verdict::Pass, Some(1), Some(1))
        .expect("validador");
    dirty.close_round().expect("ronda 1 sucia");
    dirty
        .arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    let error = dirty
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::QA),
                brief: dirty.brief(),
            },
        )
        .expect_err("ronda de verificación sin ronda limpia");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("ronda sin desarrolladores"));
}

#[test]
fn three_attempts_per_role_and_round_then_rejected() {
    let f = Fixture::new("tres-intentos");
    let _ = manual_round(&f);
    for detail in ["uno", "dos", "tres"] {
        f.launcher.fail_with(detail);
        let error = f
            .arbiter
            .execute(
                &Fixture::elephant(),
                Command::FishNew {
                    role: role(builtin::PLANNER),
                    brief: f.brief(),
                },
            )
            .expect_err("intento fallido");
        assert_eq!(error.rule, "R5");
    }
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::PLANNER),
                brief: f.brief(),
            },
        )
        .expect_err("cuarto intento");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("3 intentos"));
}

#[test]
fn modified_artifact_pauses_and_resume_restores_it() {
    let f = Fixture::new("artefactos");
    let task = f.setup();
    let live = f
        .folder
        .join(".ez")
        .join("tareas")
        .join(&task)
        .join("encargo.md");
    let original = fs::read_to_string(&live).expect("encargo original");
    fs::write(&live, "# Encargo cambiado\n").expect("modificar encargo");
    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::DEVELOPER),
                brief: f.brief(),
            },
        )
        .expect_err("artefacto modificado");
    assert_eq!(error.rule, "R3");
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "artefactos_modificados");
    f.arbiter.resume("s1").expect("reanudar");
    assert_eq!(
        fs::read_to_string(&live).expect("encargo restaurado"),
        original
    );
    f.launch(role(builtin::DEVELOPER));
}

#[test]
fn timeout_returns_in_under_five_seconds() {
    let f = Fixture::new("tope-lento");
    let body = "[[criterio]]\nid = \"C1\"\ndescripcion = \"Ping\"\n\
                comando = [\"ping\", \"-n\", \"30\", \"127.0.0.1\"]\nsegundos = 1\n";
    let outcome = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::TaskNew {
                title: "Tope".into(),
                brief: f.brief(),
                criteria: f.criteria(body),
            },
        )
        .expect("proponer");
    let _task = outcome.data["task"].as_str().expect("id");
    f.arbiter.approve_task("s1").expect("aprobar");
    f.team(Fixture::tasks());
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda");
    let started = Instant::now();
    let verified = f
        .arbiter
        .execute(&Fixture::elephant(), Command::Verify)
        .expect("verificar");
    assert!(started.elapsed().as_secs_f64() < 5.0);
    assert_eq!(verified.data["criterios"][0]["ok"], false);
    assert!(verified.data["criterios"][0]["motivo"]
        .as_str()
        .expect("motivo")
        .contains("tope"));
}

#[test]
fn hc_name_uses_eight_hex_digits_and_attempt_suffix() {
    let f = Fixture::new("nombre-hc");
    let task = manual_round(&f);
    f.launch(role(builtin::PLANNER));
    let first = token_of(&f, "plan-r1");
    assert!(!first.is_empty());
    let request = f
        .launcher
        .requests()
        .into_iter()
        .find(|item| item.fish == "plan-r1")
        .expect("lanzamiento");
    assert_eq!(request.hc_name, format!("ez-{}-plan-r1", &task[1..9]));
    assert_eq!(request.labels["ez_attempt"], "1");

    f.arbiter.fish_failed(&f.fish_key("plan-r1"), "se fue");
    f.launch(role(builtin::PLANNER));
    let second = f
        .launcher
        .requests()
        .into_iter()
        .rfind(|item| item.fish == "plan-r1")
        .expect("relanzamiento");
    assert_eq!(second.hc_name, format!("ez-{}-plan-r1-a2", &task[1..9]));
    assert_eq!(second.labels["ez_attempt"], "2");
}
