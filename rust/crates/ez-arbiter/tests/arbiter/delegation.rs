//! §15.5: delegación con `delega_en`, cantidad, profundidad, espera del padre.

use std::time::Duration;

use ez_arbiter::{Caller, Command, Verdict};
use ez_team::{builtin, builtin_roles, RoleDef, RoleId, RoleKind, RoleOrigin, Seat};

use super::support::{role, Fixture};

const LEAD: &str = "lead_dev";

fn lead_role() -> RoleDef {
    RoleDef {
        id: RoleId::parse(LEAD).expect("id"),
        name: "Desarrollador principal".to_string(),
        kind: RoleKind::Executor,
        description: "Coordina subagentes de desarrollo".to_string(),
        can_delegate: true,
        writes_tests: false,
        mutations: false,
        first_round_only: false,
        default_stage: 1,
        body: "Coordina el trabajo.".to_string(),
        origin: RoleOrigin::Task,
    }
}

fn seat(role_id: RoleId, count: u32, stage: u32, delegates_to: Vec<RoleId>) -> Seat {
    Seat {
        role: role_id,
        agent: "falso".to_string(),
        model: "modelo".to_string(),
        effort: None,
        count,
        stage,
        delegates_to,
    }
}

fn setup(f: &Fixture, max_depth: u32, dev_count: u32, delegates: Vec<RoleId>) -> String {
    let criteria_body = format!(
        "[validacion]\nprofundidad_maxima = {max_depth}\n\n\
         [[criterio]]\nid = \"C1\"\ndescripcion = \"Git responde\"\n\
         comando = [\"git\", \"--version\"]\ncontiene = \"git version\"\n"
    );
    let criteria = f.criteria(&criteria_body);
    super::task::new_task(f, criteria).expect("proponer la tarea");
    f.arbiter.approve_task("s1").expect("aprobar tarea");

    let mut roles = builtin_roles();
    roles.push(lead_role());
    let seats = vec![
        seat(role(LEAD), 1, 1, delegates),
        seat(role(builtin::DEVELOPER), dev_count, 1, Vec::new()),
        seat(role(builtin::QA), 1, 2, Vec::new()),
        seat(role(builtin::QA_VALIDATOR), 1, 3, Vec::new()),
    ];
    f.arbiter
        .execute(
            &Fixture::elephant(),
            Command::TeamPropose {
                seats,
                roles,
                card: "Equipo con delegación".to_string(),
            },
        )
        .expect("proponer el equipo");
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir la ronda");
    f.launch(role(LEAD))
}

fn delegate(
    f: &Fixture,
    parent: &str,
    role_id: RoleId,
) -> Result<ez_arbiter::Outcome, ez_arbiter::Rejection> {
    f.arbiter.execute(
        &f.as_fish(parent),
        Command::AgentDelegate {
            role: role_id,
            brief: f.brief(),
        },
    )
}

#[test]
fn a_role_without_delegation_is_rejected() {
    let f = Fixture::new("sin-delegar");
    let lead = setup(&f, 3, 2, vec![role(builtin::DEVELOPER)]);
    let developer = f.launch(role(builtin::DEVELOPER));
    let error = delegate(&f, &developer, role(builtin::DEVELOPER))
        .expect_err("el desarrollador incluido no puede delegar");
    assert_eq!(error.rule, "R5");
    assert!(
        error.message.contains("no puede delegar"),
        "{}",
        error.message
    );
    // El pez que sí puede delegar sigue funcionando.
    let child = delegate(&f, &lead, role(builtin::DEVELOPER)).expect("delegar");
    assert!(child.data["fish"].as_str().expect("hijo").contains(&lead));
}

#[test]
fn a_role_outside_delegates_to_is_rejected() {
    let f = Fixture::new("fuera-delega");
    let lead = setup(&f, 3, 2, vec![role(builtin::DEVELOPER)]);
    let error = delegate(&f, &lead, role(builtin::QA)).expect_err("el QA no está en delega_en");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("delega_en"), "{}", error.message);
}

#[test]
fn count_limit_counts_all_live_fish_of_the_role() {
    let f = Fixture::new("tope-cantidad");
    let lead = setup(&f, 3, 1, vec![role(builtin::DEVELOPER)]);
    f.launch(role(builtin::DEVELOPER));
    let error =
        delegate(&f, &lead, role(builtin::DEVELOPER)).expect_err("ya hay un desarrollador vivo");
    assert_eq!(error.rule, "R5");
    assert!(
        error.message.contains("desarrolladores vivos"),
        "{}",
        error.message
    );
}

#[test]
fn depth_limit_is_taken_from_criteria() {
    let f = Fixture::new("tope-profundidad");
    let lead = setup(&f, 1, 2, vec![role(builtin::DEVELOPER)]);
    let error = delegate(&f, &lead, role(builtin::DEVELOPER))
        .expect_err("profundidad 2 supera el máximo 1");
    assert_eq!(error.rule, "R5");
    assert!(error.message.contains("profundidad"), "{}", error.message);
}

#[test]
fn a_parent_cannot_report_before_its_children() {
    let f = Fixture::new("padre-antes");
    let lead = setup(&f, 3, 2, vec![role(builtin::DEVELOPER)]);
    let child = delegate(&f, &lead, role(builtin::DEVELOPER))
        .expect("delegar")
        .data["fish"]
        .as_str()
        .expect("hijo")
        .to_string();

    let error = f
        .report(&lead, Verdict::Pass, None, None)
        .expect_err("el padre tiene un hijo pendiente");
    assert_eq!(error.rule, "R7");
    assert!(error.message.contains(&child), "{}", error.message);

    f.report(&child, Verdict::Pass, None, None)
        .expect("informe del hijo");
    f.report(&lead, Verdict::Pass, None, None)
        .expect("informe del padre");
}

#[test]
fn a_parent_wait_receives_its_children_reports() {
    let f = Fixture::new("espera-padre");
    let lead = setup(&f, 3, 2, vec![role(builtin::DEVELOPER)]);
    delegate(&f, &lead, role(builtin::DEVELOPER)).expect("delegar");
    let parent_key = f.fish_key(&lead);
    let child = delegate(&f, &lead, role(builtin::DEVELOPER))
        .expect("delegar el segundo hijo")
        .data["fish"]
        .as_str()
        .expect("hijo")
        .to_string();
    f.report(&child, Verdict::Pass, None, None)
        .expect("informe del hijo");

    let event = f
        .arbiter
        .wait_for_fish(&parent_key, Duration::from_millis(200))
        .expect("novedad del hijo");
    assert_eq!(event.kind, "fish_reported");
    assert_eq!(event.data["fish"], child);
    assert_eq!(event.data["parent"], lead);
}

#[test]
fn a_delegated_child_carries_parent_and_elephant_labels() {
    let f = Fixture::new("etiquetas-hijo");
    let lead = setup(&f, 3, 2, vec![role(builtin::DEVELOPER)]);
    delegate(&f, &lead, role(builtin::DEVELOPER)).expect("delegar");
    let requests = f.launcher.requests();
    let last = requests.last().expect("petición del hijo");
    assert_eq!(last.labels["ez_parent"], lead);
    assert_eq!(last.labels["ez_elephant"], "e00000000");
    assert_eq!(last.labels["ez_role"], builtin::DEVELOPER);
    assert_eq!(last.seat.stage, 1, "hereda la etapa del padre");
    let key = f
        .fish_info(&format!("{lead}-{}", role(builtin::DEVELOPER).short()))
        .0;
    assert_eq!(key.parent.as_deref(), Some(lead.as_str()));
    assert_eq!(key.elephant, "e00000000");
    assert!(
        Caller::Fish(key) == f.as_fish(&format!("{lead}-{}", role(builtin::DEVELOPER).short()))
    );
}
