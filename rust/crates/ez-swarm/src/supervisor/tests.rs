//! Tests of the swarm supervisor.

mod autonomous;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::test_support::FakeHost;
use super::*;
use crate::db::test_support::{temp_board, TempBoard};
use crate::service::{MemberSpec, SwarmService};

struct Fixture {
    supervisor: Arc<Supervisor>,
    service: Arc<SwarmService>,
    host: Arc<FakeHost>,
    said: Arc<Mutex<Vec<String>>>,
    asked: Arc<Mutex<Vec<String>>>,
    base: PathBuf,
    _board: TempBoard,
}

impl Fixture {
    fn claim(&self, swarm: &str) {
        self.service
            .store
            .claim(swarm, "ana", Some(1), false)
            .unwrap_or_else(|error| panic!("{error}"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        remove_tree(&self.base);
    }
}

fn remove_tree(folder: &PathBuf) {
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                remove_tree(&path);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let _ = std::fs::remove_dir(folder);
}

fn fixture() -> Fixture {
    let board = temp_board(100.0);
    let clock = Arc::new(|| 100.0);
    let db = crate::db::Db::new(board.path.clone(), clock.clone())
        .unwrap_or_else(|error| panic!("{error}"));
    let service = Arc::new(SwarmService::new(db, "ezs", clock.clone()));
    let host = Arc::new(FakeHost::new());
    let said = Arc::new(Mutex::new(Vec::new()));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let announce: Announce = {
        let said = Arc::clone(&said);
        Arc::new(move |text| {
            said.lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(text.to_string());
        })
    };
    let ask: Announce = {
        let asked = Arc::clone(&asked);
        Arc::new(move |text| {
            asked
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(text.to_string());
        })
    };
    let base = std::env::temp_dir().join(format!("ez-swarm-sup-{}", uuid::Uuid::new_v4().simple()));
    let mut config = SupervisorConfig::new(base.join("seats"), announce);
    config.ask = Some(ask);
    config.clock = clock;
    config.wall = Arc::new(|| 100.0);
    let host_dyn: Arc<dyn SwarmHost> = Arc::clone(&host) as Arc<dyn SwarmHost>;
    let supervisor = Arc::new(Supervisor::new(Arc::clone(&service), host_dyn, config));
    Fixture {
        supervisor,
        service,
        host,
        said,
        asked,
        base,
        _board: board,
    }
}

fn one_member_swarm(fixture: &Fixture) -> String {
    let members = [MemberSpec::new("ana", "dev", "claude")];
    fixture
        .service
        .start("Migrar", &members, "", None)
        .unwrap_or_else(|error| panic!("{error}"))
        .0
}

fn address(fixture: &Fixture, swarm: &str) {
    fixture
        .service
        .store
        .set_address(swarm, "ana", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
}

fn agent_row(address: &str, state: &str) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("address".to_string(), Value::String(address.to_string()));
    row.insert("state".to_string(), Value::String(state.to_string()));
    row
}

fn wait_for(check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no llegó el cambio esperado");
}

#[test]
fn agent_state_maps_python_values() {
    for (raw, expected) in [
        (Value::String(" Working ".to_string()), "working"),
        (Value::String("done".to_string()), "idle"),
        (Value::String("IDLE".to_string()), "idle"),
        (Value::String("blocked".to_string()), "blocked"),
        (Value::String("error".to_string()), "error"),
        (Value::String("raro".to_string()), "unknown"),
        (Value::Null, "unknown"),
        (Value::from(0), "unknown"),
        (Value::from("working"), "working"),
    ] {
        assert_eq!(agent_state(Some(&raw)), expected);
    }
    assert_eq!(agent_state(None), "unknown");
}

#[test]
fn tick_without_active_swarms_returns_false() {
    let fixture = fixture();
    assert!(!fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}")));
    assert_eq!(fixture.supervisor.running(), 0);
    assert!(fixture.supervisor.active_view().is_empty());
}

#[test]
fn tick_tracks_the_active_swarms_for_the_ui() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    assert!(fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}")));
    assert_eq!(fixture.supervisor.running(), 1);
    assert_eq!(
        fixture.supervisor.active_view(),
        vec![(swarm, "Migrar".to_string())]
    );
}

#[test]
fn a_member_without_terminal_is_launched_when_it_has_work() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    wait_for(|| fixture.host.launched_count() == 1);
    let launched = fixture
        .host
        .launched
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert_eq!(launched[0].0, "claude");
    assert_eq!(launched[0].1, "");
    assert_eq!(launched[0].2.name, "ana · enjambre");
    let seat = launched[0]
        .2
        .env
        .get("EZS_SEAT")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(seat.ends_with(&format!("{swarm}-ana.seat")));
    assert!(std::path::Path::new(&seat).is_file());
    let member = fixture
        .service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|member| member.name == "ana")
        .unwrap_or_else(|| panic!("falta ana"));
    assert_eq!(member.address, "t-claude");
    assert!(fixture
        .supervisor
        .book
        .lock()
        .unbriefed
        .contains(&(swarm.clone(), "ana".to_string())));
}

#[test]
fn an_idle_member_with_unread_messages_is_woken_once() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture.host.set_agents(vec![agent_row("t1", "idle")]);
    fixture
        .service
        .tell(&swarm, "hola", "ana")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let sent = fixture.host.sent_lines();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].1,
        "[EzAgent] Tienes 1 mensaje nuevo y 1 tarea lista en el enjambre. Ejecuta: ezs next"
    );
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixture.host.sent_lines().len(), 1);
}

#[test]
fn working_agents_touch_the_board_and_leave_blocked() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture
        .service
        .store
        .set_member_state(&swarm, "ana", "blocked")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture.host.set_agents(vec![agent_row("t1", "working")]);
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let member = fixture
        .service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|member| member.name == "ana")
        .unwrap_or_else(|| panic!("falta ana"));
    assert_eq!(member.state, "working");
}

#[test]
fn three_missing_ticks_open_a_closed_incident() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture.host.set_agents(Vec::new());
    for _ in 0..MISSING_TICKS {
        fixture
            .supervisor
            .tick()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let incident = fixture
        .service
        .store
        .member_incident(&swarm, "ana")
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(incident.kind, "closed");
    assert_eq!(incident.detail, "Su ventana ya no está abierta.");
    let asked = fixture
        .asked
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(asked.iter().any(|text| text.contains("¿Lo reabro")));
}

#[test]
fn a_shell_without_the_agent_is_reopened_up_to_the_limit() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture.host.set_agents(Vec::new());
    let mut detail = Map::new();
    detail.insert("state".to_string(), Value::String("unknown".to_string()));
    detail.insert(
        "agent".to_string(),
        Value::String("powershell.exe".to_string()),
    );
    *fixture
        .host
        .detail
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(detail);
    for _ in 0..MISSING_TICKS {
        fixture
            .supervisor
            .tick()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let member = fixture
        .service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|member| member.name == "ana")
        .unwrap_or_else(|| panic!("falta ana"));
    assert_eq!(member.address, "");
    assert!(fixture
        .service
        .store
        .member_incident(&swarm, "ana")
        .unwrap_or_else(|error| panic!("{error}"))
        .is_none());
}

#[test]
fn a_shell_after_two_launches_reports_exited() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture.host.set_agents(Vec::new());
    let mut detail = Map::new();
    detail.insert("state".to_string(), Value::String("unknown".to_string()));
    detail.insert("agent".to_string(), Value::String("pwsh.exe".to_string()));
    *fixture
        .host
        .detail
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(detail);
    fixture
        .supervisor
        .book
        .lock()
        .launches
        .insert((swarm.clone(), "ana".to_string()), MAX_LAUNCHES);
    for _ in 0..MISSING_TICKS {
        fixture
            .supervisor
            .tick()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let incident = fixture
        .service
        .store
        .member_incident(&swarm, "ana")
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(incident.kind, "exited");
}

#[test]
fn a_finished_event_asks_the_final_summary() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture.supervisor.book.on_event(&swarm, "ana", "finished");
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let asked = fixture
        .asked
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(asked
        .last()
        .is_some_and(|text| text.ends_with(crate::protocol::FINAL_OFFER)));
}

#[test]
fn the_thread_starts_and_stops() {
    let fixture = fixture();
    fixture.supervisor.start();
    assert!(fixture
        .supervisor
        .thread
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some());
    fixture.supervisor.poke();
    fixture.supervisor.stop();
    assert!(fixture
        .supervisor
        .thread
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_none());
}

#[test]
fn issue_notices_are_said_once_and_skip_incidents() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture
        .service
        .store
        .open_incident(&swarm, "ana", "quota", "sin saldo", 0.0)
        .unwrap_or_else(|error| panic!("{error}"));
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(!said.iter().any(|text| text.contains("incidente")));
    assert!(!said.iter().any(|text| text.contains("quota")));
}

#[test]
fn blocked_without_work_is_released_to_idle() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture.claim(&swarm);
    fixture
        .service
        .store
        .finish(&swarm, "ana", 1, "listo", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture
        .service
        .store
        .set_member_state(&swarm, "ana", "blocked")
        .unwrap_or_else(|error| panic!("{error}"));
    address(&fixture, &swarm);
    fixture.host.set_agents(vec![agent_row("t1", "idle")]);
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(fixture.host.sent_lines().is_empty());
    let member = fixture
        .service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|member| member.name == "ana")
        .unwrap_or_else(|| panic!("falta ana"));
    assert_eq!(member.state, "idle");
}

#[test]
fn an_open_task_is_reminded_once() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture.claim(&swarm);
    address(&fixture, &swarm);
    fixture.host.set_agents(vec![agent_row("t1", "idle")]);
    fixture
        .supervisor
        .book
        .lock()
        .idle_since
        .insert((swarm, "ana".to_string()), 40.0);
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let sent = fixture.host.sent_lines();
    assert_eq!(sent.len(), 1);
    assert!(sent[0]
        .1
        .starts_with("[EzAgent] T1 sigue abierta en el enjambre."));
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixture.host.sent_lines().len(), 1);
}

#[test]
fn the_start_line_repeats_until_the_agent_uses_ezs() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    address(&fixture, &swarm);
    fixture
        .supervisor
        .book
        .lock()
        .unbriefed
        .insert((swarm.clone(), "ana".to_string()));
    fixture.host.set_agents(vec![agent_row("t1", "idle")]);
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let sent = fixture.host.sent_lines();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].1,
        format!(
            "[EzAgent] Eres «ana» (desarrollar) en el enjambre {swarm}. Ejecuta ahora: ezs next"
        )
    );
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixture.host.sent_lines().len(), 1);
}

#[test]
fn a_member_waiting_for_the_trust_dialog_keeps_its_terminal() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    let mut row = Map::new();
    row.insert("ok".to_string(), Value::Bool(false));
    row.insert("address".to_string(), Value::String("t7".to_string()));
    row.insert(
        "error".to_string(),
        Value::String(
            "claude está pidiendo confirmar algo en la terminal t7 (por \
             ejemplo, confiar en la carpeta)."
                .to_string(),
        ),
    );
    *fixture
        .host
        .run_result
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(Ok(row));
    fixture.host.set_screen(
        "t7",
        include_str!("../../../ez-herdr/tests/permission/claude_trust_numbered.txt"),
    );
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    wait_for(|| {
        fixture
            .service
            .store
            .members(&swarm)
            .map(|members| {
                members
                    .iter()
                    .any(|member| member.address == "t7" && member.state == "blocked")
            })
            .unwrap_or(false)
    });
    // El bucle autónomo aprueba ese diálogo: el supervisor no lo da por perdido.
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(
        !said.iter().any(|text| text.contains("no pude abrir")),
        "{said:?}"
    );
}

#[test]
fn a_failed_launch_marks_the_member_gone_and_announces() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    *fixture
        .host
        .run_result
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(Err("boom".to_string()));
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    wait_for(|| {
        fixture
            .service
            .store
            .members(&swarm)
            .map(|members| members.iter().any(|member| member.state == "gone"))
            .unwrap_or(false)
    });
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(said
        .iter()
        .any(|text| text == "Enjambre: no pude abrir a ana con claude. boom"));
}
