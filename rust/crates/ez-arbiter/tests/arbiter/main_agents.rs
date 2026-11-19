//! §15.1: varios agentes principales, worktrees aislados, recarga e integración.

use std::fs;
use std::path::{Path, PathBuf};

use ez_arbiter::{Caller, Command, ElephantKey, Outcome, Verdict};
use ez_team::builtin;

use super::support::{git, git_output, role, Fixture};

const CRITERIA: &str = "[[criterio]]\nid = \"C1\"\ndescripcion = \"Git responde\"\n\
                        comando = [\"git\", \"--version\"]\ncontiene = \"git version\"\n";

fn inputs(worktree: &Path) -> (PathBuf, PathBuf) {
    let brief = worktree.join("encargo.md");
    let criteria = worktree.join("criterios.toml");
    fs::write(&brief, "# Encargo\n\nHaz la tarea.\n").expect("escribir el encargo");
    fs::write(&criteria, CRITERIA).expect("escribir los criterios");
    (brief, criteria)
}

fn call(f: &Fixture, key: &ElephantKey, command: Command) -> Outcome {
    f.arbiter
        .execute(&Caller::Elephant(key.clone()), command)
        .expect("comando del agente principal")
}

fn propose_for(f: &Fixture, key: &ElephantKey, worktree: &Path) -> String {
    let (brief, criteria) = inputs(worktree);
    call(
        f,
        key,
        Command::TaskNew {
            title: "Tarea del agente B".to_string(),
            brief,
            criteria,
        },
    )
    .data["task"]
        .as_str()
        .expect("id de tarea")
        .to_string()
}

fn launch_for(f: &Fixture, key: &ElephantKey, worktree: &Path, role_id: ez_team::RoleId) -> String {
    call(
        f,
        key,
        Command::FishNew {
            role: role_id,
            brief: worktree.join("encargo.md"),
        },
    )
    .data["fish"]
        .as_str()
        .expect("id del pez")
        .to_string()
}

fn snapshot_of(f: &Fixture, key: &ElephantKey) -> serde_json::Value {
    f.arbiter
        .snapshot_main(key)
        .expect("instantánea del agente principal")
}

fn clean_round_for(f: &Fixture, key: &ElephantKey, worktree: &Path) {
    let dev = launch_for(f, key, worktree, role(builtin::DEVELOPER));
    f.report(&dev, Verdict::Pass, None, None)
        .expect("desarrollo");
    let qa = launch_for(f, key, worktree, role(builtin::QA));
    f.report(&qa, Verdict::Pass, None, None).expect("QA");
    let val = launch_for(f, key, worktree, role(builtin::QA_VALIDATOR));
    f.report(&val, Verdict::Pass, Some(3), Some(3))
        .expect("validador");
    call(f, key, Command::RoundClose);
}

fn integrate_card(f: &Fixture, task: &str) -> String {
    f.bitacora(task)
        .into_iter()
        .filter(|event| event["kind"]["type"] == "card_opened")
        .find(|event| event["kind"]["kind"] == "integrate")
        .and_then(|event| event["kind"]["id"].as_str().map(str::to_string))
        .expect("tarjeta de integración")
}

#[test]
fn two_main_agents_have_isolated_worktrees_and_tasks() {
    let f = Fixture::new("dos-agentes");
    let second = f
        .arbiter
        .create_main_agent("s1", "Agente B")
        .expect("crear agente principal");
    assert!(second.worktree.ends_with(&second.key.elephant));
    assert!(second.branch.starts_with("ez/"));
    assert!(second.worktree.is_dir());

    let first_task = f.propose();
    let second_task = propose_for(&f, &second.key, &second.worktree);
    assert_ne!(first_task, second_task);
    assert_eq!(snapshot_of(&f, &second.key)["task"], second_task);

    f.arbiter.approve_task("s1").expect("aprobar tarea A");
    f.arbiter
        .approve_task_for(&second.key)
        .expect("aprobar tarea B");

    fs::write(second.worktree.join("solo-b.txt"), "b").expect("cambio en B");
    call(&f, &Fixture::elephant_key("e00000000"), Command::Status);
    assert_ne!(
        snapshot_of(&f, &ElephantKey::default_for("s1"))["status"],
        "paused",
        "el cambio en B no debe verse en A"
    );

    fs::write(f.folder.join("solo-a.txt"), "a").expect("cambio en A");
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::Status)
        .expect_err("el cambio en A sí se detecta");
    assert_eq!(error.rule, "R11");
    assert_eq!(
        snapshot_of(&f, &second.key)["status"],
        "approved",
        "B sigue igual"
    );
}

#[test]
fn remove_main_agent_checks_task_and_worktree() {
    let f = Fixture::new("quitar-agente");
    let busy = f
        .arbiter
        .create_main_agent("s1", "Ocupado")
        .expect("crear agente");
    propose_for(&f, &busy.key, &busy.worktree);
    let error = f
        .arbiter
        .remove_main_agent(&busy.key)
        .expect_err("no se puede quitar con tarea activa");
    assert!(error.message.contains("tarea activa"), "{}", error.message);

    let free = f
        .arbiter
        .create_main_agent("s1", "Libre")
        .expect("crear otro agente");
    f.arbiter
        .remove_main_agent(&free.key)
        .expect("quitar agente sin tarea");
    assert!(f
        .arbiter
        .main_agents("s1")
        .iter()
        .all(|agent| agent.key != free.key));
    assert!(!free.worktree.exists());
}

#[test]
fn agentes_json_is_reloaded_by_a_new_arbiter() {
    let f = Fixture::new("recargar-agentes");
    let created = f
        .arbiter
        .create_main_agent("s1", "Persistido")
        .expect("crear agente");
    let reopened = f.reopen();
    let agents = reopened.main_agents("s1");
    assert!(agents.iter().any(|agent| agent.key == created.key));
    assert_eq!(
        reopened
            .main_agents("s1")
            .into_iter()
            .find(|agent| agent.key == created.key)
            .expect("agente recargado")
            .branch,
        created.branch
    );
}

#[test]
fn closing_a_task_offers_integration_and_merge_succeeds() {
    let f = Fixture::new("integrar");
    let agent = f
        .arbiter
        .create_main_agent("s1", "Integrador")
        .expect("crear agente");
    fs::write(agent.worktree.join("entregable.txt"), "hecho").expect("escribir entregable");
    git(&agent.worktree, &["add", "entregable.txt"]);
    git(&agent.worktree, &["commit", "-q", "-m", "entregable"]);
    let task = propose_for(&f, &agent.key, &agent.worktree);
    f.arbiter
        .approve_task_for(&agent.key)
        .expect("aprobar tarea");
    call(
        &f,
        &agent.key,
        Command::TeamPropose {
            seats: Fixture::tasks(),
            roles: ez_team::builtin_roles(),
            card: "Equipo".to_string(),
        },
    );
    f.arbiter
        .approve_team_for(&agent.key)
        .expect("aprobar equipo");
    call(&f, &agent.key, Command::RoundStart);
    let plan = launch_for(&f, &agent.key, &agent.worktree, role(builtin::PLANNER));
    f.report(&plan, Verdict::Pass, None, None).expect("plan");
    clean_round_for(&f, &agent.key, &agent.worktree);
    call(&f, &agent.key, Command::RoundStart);
    clean_round_for(&f, &agent.key, &agent.worktree);
    call(&f, &agent.key, Command::TaskClose);

    let card = integrate_card(&f, &task);
    let outcome = f
        .arbiter
        .card_action(&card, "merge", None)
        .expect("integrar la rama");
    assert!(outcome.message.contains("integró"), "{}", outcome.message);
    let log = git_output(&f.folder, &["log", "--oneline", "-3"]);
    assert!(log.contains("EzAgent: integra"), "{log}");
    assert!(f
        .bitacora(&task)
        .iter()
        .any(|event| event["kind"]["type"] == "integrated"));
}

#[test]
fn keeping_the_branch_closes_the_integration_card() {
    let f = Fixture::new("mantener-rama");
    let agent = f
        .arbiter
        .create_main_agent("s1", "Conservador")
        .expect("crear agente");
    let task = propose_for(&f, &agent.key, &agent.worktree);
    f.arbiter
        .approve_task_for(&agent.key)
        .expect("aprobar tarea");
    call(
        &f,
        &agent.key,
        Command::TeamPropose {
            seats: Fixture::tasks(),
            roles: ez_team::builtin_roles(),
            card: "Equipo".to_string(),
        },
    );
    f.arbiter
        .approve_team_for(&agent.key)
        .expect("aprobar equipo");
    call(&f, &agent.key, Command::RoundStart);
    let plan = launch_for(&f, &agent.key, &agent.worktree, role(builtin::PLANNER));
    f.report(&plan, Verdict::Pass, None, None).expect("plan");
    clean_round_for(&f, &agent.key, &agent.worktree);
    call(&f, &agent.key, Command::RoundStart);
    clean_round_for(&f, &agent.key, &agent.worktree);
    call(&f, &agent.key, Command::TaskClose);

    let card = integrate_card(&f, &task);
    f.arbiter
        .card_action(&card, "keep", None)
        .expect("dejar en su rama");
    assert!(
        f.arbiter.card_action(&card, "merge", None).is_err(),
        "la tarjeta ya no está abierta"
    );
    git(&f.folder, &["status", "--porcelain"]);
}
