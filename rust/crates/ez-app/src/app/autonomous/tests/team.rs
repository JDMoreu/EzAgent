//! Modo autónomo con equipo (10.18): doble del enjambre en memoria, arranque,
//! verificación al vaciarse el tablero, tareas `fix` e incidentes.

use std::sync::Arc;

use ez_agent::autonomous::{Criterion, Mode, RoleSpec, TaskState, TeamSpec};
use ez_swarm::service::MemberSpec;
use parking_lot::Mutex;

use super::{clean_verdict, dirty_verdict, Harness};
use crate::app::autonomous::{TeamMember, TeamSwarm};

#[derive(Default)]
struct Board {
    goal: String,
    workdir: String,
    status: String,
    members: Vec<TeamMember>,
    tasks: Vec<String>,
    incidents: Vec<i64>,
    fixes: Vec<(String, String, String)>,
    started: bool,
}

/// Enjambre en memoria: nada de tablero real, ni terminales, ni herdr.
pub(crate) struct FakeTeam {
    board: Mutex<Board>,
}

impl FakeTeam {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            board: Mutex::new(Board {
                status: "active".to_string(),
                tasks: vec!["pending".to_string()],
                ..Board::default()
            }),
        })
    }

    pub(crate) fn set_tasks(&self, states: &[&str]) {
        self.board.lock().tasks = states.iter().map(|state| (*state).to_string()).collect();
    }

    pub(crate) fn set_incidents(&self, ids: &[i64]) {
        self.board.lock().incidents = ids.to_vec();
    }

    pub(crate) fn set_status(&self, status: &str) {
        self.board.lock().status = status.to_string();
    }

    pub(crate) fn goal(&self) -> String {
        self.board.lock().goal.clone()
    }

    pub(crate) fn workdir(&self) -> String {
        self.board.lock().workdir.clone()
    }

    pub(crate) fn fixes(&self) -> Vec<(String, String, String)> {
        self.board.lock().fixes.clone()
    }

    pub(crate) fn started(&self) -> bool {
        self.board.lock().started
    }
}

impl TeamSwarm for FakeTeam {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        (!name.is_empty()).then(|| name.to_string())
    }

    fn agent_names(&self) -> Vec<String> {
        vec!["claude".to_string(), "codex".to_string()]
    }

    fn start(&self, goal: &str, workdir: &str, members: &[MemberSpec]) -> Result<String, String> {
        let mut board = self.board.lock();
        board.started = true;
        board.goal = goal.to_string();
        board.workdir = workdir.to_string();
        board.members = members
            .iter()
            .map(|member| TeamMember {
                name: member.name.clone(),
                role: member.role.clone(),
                address: format!("m-{}", member.name),
            })
            .collect();
        Ok("sw1".to_string())
    }

    fn members(&self, _swarm: &str) -> Result<Vec<TeamMember>, String> {
        Ok(self.board.lock().members.clone())
    }

    fn status(&self, _swarm: &str) -> Result<String, String> {
        Ok(self.board.lock().status.clone())
    }

    fn task_states(&self, _swarm: &str) -> Result<Vec<String>, String> {
        Ok(self.board.lock().tasks.clone())
    }

    fn incidents(&self, _swarm: &str) -> Result<Vec<i64>, String> {
        Ok(self.board.lock().incidents.clone())
    }

    fn fix_task(&self, _swarm: &str, title: &str, note: &str, role: &str) -> Result<i64, String> {
        let mut board = self.board.lock();
        board
            .fixes
            .push((title.to_string(), note.to_string(), role.to_string()));
        Ok(board.fixes.len() as i64)
    }
}

fn team_spec(h: &Harness) -> TeamSpec {
    TeamSpec {
        task: "migrar la API".to_string(),
        done_criteria: vec![Criterion {
            text: "compila".to_string(),
            check: "cargo build".to_string(),
        }],
        roles: vec![
            RoleSpec {
                role: "planificador".to_string(),
                agent: "codex".to_string(),
                model: None,
            },
            RoleSpec {
                role: "desarrollador".to_string(),
                agent: "claude".to_string(),
                model: Some("opus".to_string()),
            },
        ],
        space: "proyecto".to_string(),
        workdir: h.workdir.display().to_string(),
        verifier_agent: "claude".to_string(),
        verifier_model: None,
        max_rounds: 5,
        max_minutes: 120,
        max_approvals: 50,
    }
}

fn harness() -> (Harness, Arc<FakeTeam>) {
    let team = FakeTeam::new();
    let harness = Harness::with_team(Arc::clone(&team) as Arc<dyn TeamSwarm>);
    (harness, team)
}

#[test]
fn the_team_starts_with_normalized_roles_and_covers_its_members() {
    let (h, team) = harness();
    let id = h
        .loop_
        .start_team_task(&team_spec(&h))
        .expect("arranca el equipo");
    assert!(team.started());
    assert_eq!(team.goal(), "migrar la API");
    assert!(team.workdir().ends_with("auto-work"), "{}", team.workdir());
    assert_eq!(h.loop_.status().expect("estado").mode, Mode::Team);
    assert_eq!(h.loop_.status().expect("estado").state, TaskState::Running);
    assert!(h.wait(|| h.loop_.covers("m-plan") && h.loop_.covers("m-dev")));
    let state = std::fs::read_to_string(h.task_dir(&id).join("privado").join("estado.json"))
        .expect("estado.json");
    assert!(state.contains("\"mode\": \"team\""), "{state}");
    assert!(state.contains("\"swarm\": \"sw1\""), "{state}");
    assert!(state.contains("\"role\": \"plan\""), "{state}");
    assert!(state.contains("\"role\": \"dev\""), "{state}");
    let bitacora = h.bitacora(&id);
    assert!(bitacora.contains("\"event\":\"start\""));
    assert!(bitacora.contains("\"swarm\":\"sw1\""));
}

#[test]
fn verification_starts_when_the_board_has_no_pending_or_running_tasks() {
    let (h, team) = harness();
    let id = h
        .loop_
        .start_team_task(&team_spec(&h))
        .expect("arranca el equipo");
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(h.port.launches().is_empty(), "el tablero tiene trabajo");
    team.set_tasks(&[]);
    assert!(h.wait_launch(1), "se lanzó el verificador");
    let verifier = h.port.last_launch().expect("verificador");
    assert_eq!(verifier.agent, "claude");
    assert!(verifier.task.contains("verificacion-1"));
    assert!(h.wait(|| h
        .loop_
        .status()
        .is_some_and(|summary| summary.state == TaskState::Verifying)));
    h.write_verdict(
        &id,
        1,
        clean_verdict(&super::token_of(&verifier.task), "cargo build: ok"),
    );
    h.port.set_state(&verifier.address, "done");
    assert!(h.wait_launch(2), "segunda verificación limpia");
    let second = h.port.last_launch().expect("verificador 2");
    h.write_verdict(
        &id,
        2,
        clean_verdict(&super::token_of(&second.task), "cargo build: ok"),
    );
    h.port.set_state(&second.address, "done");
    assert!(h.wait_state(TaskState::Done));
    assert!(h
        .says()
        .iter()
        .any(|text| text.starts_with("Terminé «migrar la API»")));
}

#[test]
fn a_dirty_round_creates_one_fix_task_per_blocking_finding() {
    let (h, team) = harness();
    let id = h
        .loop_
        .start_team_task(&team_spec(&h))
        .expect("arranca el equipo");
    team.set_tasks(&[]);
    assert!(h.wait_launch(1));
    let verifier = h.port.last_launch().expect("verificador");
    h.write_verdict(
        &id,
        1,
        dirty_verdict(
            &super::token_of(&verifier.task),
            "la API no valida el token",
        ),
    );
    h.port.set_state(&verifier.address, "done");
    assert!(
        h.wait(|| !team.fixes().is_empty()),
        "se crean las tareas fix"
    );
    let fixes = team.fixes();
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].0, "Corregir hallazgo 1");
    assert!(fixes[0].1.contains("correccion-1.md"), "{}", fixes[0].1);
    assert_eq!(fixes[0].2, "dev");
    let correction =
        std::fs::read_to_string(h.task_dir(&id).join("compartido").join("correccion-1.md"))
            .expect("correccion-1.md");
    assert!(correction.contains("la API no valida el token"));
    assert!(h.wait(|| h
        .loop_
        .status()
        .is_some_and(|summary| summary.state == TaskState::Running && summary.round == 2)));
}

#[test]
fn a_team_without_dev_sends_the_fix_to_the_first_other_role() {
    let (h, team) = harness();
    let mut spec = team_spec(&h);
    spec.roles.retain(|role| role.role != "desarrollador");
    spec.roles.push(RoleSpec {
        role: "arquitecto".to_string(),
        agent: "codex".to_string(),
        model: None,
    });
    let id = h.loop_.start_team_task(&spec).expect("arranca el equipo");
    team.set_tasks(&[]);
    assert!(h.wait_launch(1));
    let verifier = h.port.last_launch().expect("verificador");
    h.write_verdict(
        &id,
        1,
        dirty_verdict(&super::token_of(&verifier.task), "falta el índice"),
    );
    h.port.set_state(&verifier.address, "done");
    assert!(h.wait(|| !team.fixes().is_empty()));
    assert_eq!(team.fixes()[0].2, "arch");
}

#[test]
fn a_team_with_only_plan_and_qa_pauses_instead_of_sending_the_fix() {
    let (h, team) = harness();
    let mut spec = team_spec(&h);
    spec.roles = vec![
        RoleSpec {
            role: "plan".to_string(),
            agent: "codex".to_string(),
            model: None,
        },
        RoleSpec {
            role: "qa".to_string(),
            agent: "claude".to_string(),
            model: None,
        },
    ];
    let id = h.loop_.start_team_task(&spec).expect("arranca el equipo");
    team.set_tasks(&[]);
    assert!(h.wait_launch(1));
    let verifier = h.port.last_launch().expect("verificador");
    h.write_verdict(
        &id,
        1,
        dirty_verdict(&super::token_of(&verifier.task), "falta el índice"),
    );
    h.port.set_state(&verifier.address, "done");
    assert!(h.wait(|| h.paused_reason().is_some()), "pausa sin rol");
    assert_eq!(
        h.paused_reason().expect("motivo"),
        ez_agent::autonomous::texts::REASON_TEAM_ROLE
    );
    assert!(team.fixes().is_empty());
}

#[test]
fn a_swarm_incident_pauses_the_loop_without_swarm_questions() {
    let (h, team) = harness();
    h.loop_
        .start_team_task(&team_spec(&h))
        .expect("arranca el equipo");
    team.set_incidents(&[3]);
    assert!(
        h.wait(|| h.paused_reason().is_some()),
        "pausa por incidente"
    );
    assert_eq!(
        h.paused_reason().expect("motivo"),
        ez_agent::autonomous::texts::REASON_TEAM_INCIDENT
    );
    assert!(h.port.keys_sent().is_empty());
    assert!(h
        .says()
        .iter()
        .any(|text| text.contains("se quedó sin cuota, contexto o ventana")));
}

#[test]
fn a_stopped_swarm_pauses_the_autonomous_task() {
    let (h, team) = harness();
    h.loop_
        .start_team_task(&team_spec(&h))
        .expect("arranca el equipo");
    team.set_status("stopped");
    assert!(h.wait(|| h.paused_reason().is_some()));
    assert_eq!(
        h.paused_reason().expect("motivo"),
        ez_agent::autonomous::texts::REASON_TEAM_STOPPED
    );
}

#[test]
fn the_team_needs_the_swarm_mode_and_keeps_the_slot_free_in_that_case() {
    let h = Harness::new();
    let result = h.loop_.start_team_task(&team_spec(&h));
    assert_eq!(
        result,
        Err(ez_agent::autonomous::texts::TEAM_UNAVAILABLE.to_string())
    );
    assert!(!h.loop_.busy());
}
