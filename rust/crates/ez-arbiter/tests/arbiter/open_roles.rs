//! Papeles propios con etapas: el orden depende de la etapa y del tipo del papel.

use ez_arbiter::{Command, Verdict};
use ez_team::{builtin, builtin_roles, RoleDef, RoleId, RoleKind, RoleOrigin, Seat};

use super::support::{role, Fixture};

const AUDITOR: &str = "security_auditor";

fn auditor() -> RoleDef {
    RoleDef {
        id: RoleId::parse(AUDITOR).expect("id"),
        name: "Auditor de seguridad".to_string(),
        kind: RoleKind::Verifier,
        description: "Revisa el código cambiado".to_string(),
        can_delegate: false,
        writes_tests: false,
        mutations: false,
        first_round_only: false,
        default_stage: 4,
        body: "Revisa el código.".to_string(),
        origin: RoleOrigin::Task,
    }
}

fn seat(role: RoleId, stage: u32) -> Seat {
    Seat {
        role,
        agent: "falso".to_string(),
        model: "modelo".to_string(),
        effort: None,
        count: 1,
        stage,
        delegates_to: Vec::new(),
    }
}

#[test]
fn a_custom_verifier_blocks_the_next_stage_and_its_fail_dirties_the_round() {
    let f = Fixture::new("papel-propio");
    let task = f.propose();
    f.arbiter.approve_task("s1").expect("aprobar tarea");

    let mut roles = builtin_roles();
    roles.push(auditor());
    let mut seats = Fixture::tasks();
    seats.push(seat(RoleId::parse(AUDITOR).expect("id"), 4));
    f.arbiter
        .execute(
            &Fixture::elephant(),
            Command::TeamPropose {
                seats,
                roles,
                card: "Equipo con auditor".to_string(),
            },
        )
        .expect("proponer el equipo");
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir la ronda");

    f.launch(role(builtin::PLANNER));
    f.report("plan-r1", Verdict::Pass, None, None)
        .expect("plan");
    f.launch(role(builtin::DEVELOPER));
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("desarrollo");

    let launched = f.launch(RoleId::parse(AUDITOR).expect("id"));
    assert_eq!(launched, "security_a-r1");

    let error = f
        .arbiter
        .execute(
            &Fixture::elephant(),
            Command::FishNew {
                role: role(builtin::QA_VALIDATOR),
                brief: f.brief(),
            },
        )
        .expect_err("la etapa 5 espera al auditor de la etapa 4");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("security_a-r1"), "{}", error.message);

    f.report(&launched, Verdict::Fail, None, None)
        .expect("informe del auditor");
    f.launch(role(builtin::QA));
    f.report("qa-r1", Verdict::Pass, None, None).expect("QA");
    f.launch(role(builtin::QA_VALIDATOR));
    f.report("valqa-r1", Verdict::Pass, Some(3), Some(3))
        .expect("validador");

    let outcome = f.close_round().expect("cerrar la ronda");
    assert!(outcome.message.contains("sucia"), "{}", outcome.message);
    assert!(
        outcome.message.contains("security_a-r1"),
        "{}",
        outcome.message
    );
    assert_eq!(f.state_json(&task)["clean_streak"], 0);
}
