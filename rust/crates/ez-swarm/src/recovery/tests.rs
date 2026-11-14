//! Tests of the swarm recovery.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::*;
use crate::db::test_support::{temp_board, TempBoard};
use crate::db::Db;
use crate::models::Member;
use crate::service::{MemberSpec, SwarmService};
use crate::supervisor::test_support::FakeHost;
use crate::supervisor::{Announce, SwarmHost};

struct Fixture {
    recovery: Recovery,
    service: Arc<SwarmService>,
    host: Arc<FakeHost>,
    said: Arc<Mutex<Vec<String>>>,
    asked: Arc<Mutex<Vec<String>>>,
    resets: Arc<Mutex<Vec<(String, String)>>>,
    now: Arc<Mutex<f64>>,
    base: PathBuf,
    _board: TempBoard,
}

impl Fixture {
    fn advance(&self, secs: f64) {
        *self.now.lock().unwrap_or_else(|error| error.into_inner()) += secs;
    }

    fn swarm(&self) -> String {
        let members = [
            MemberSpec::new("ana", "plan", "claude"),
            MemberSpec::new("bo", "dev", "opencode"),
            MemberSpec::new("cy", "qa", "codex"),
        ];
        let tasks = [("Portar".to_string(), "dev".to_string())];
        self.service
            .start("Migrar", &members, "", Some(&tasks))
            .unwrap_or_else(|error| panic!("{error}"))
            .0
    }

    fn info(&self, swarm: &str) -> crate::models::SwarmInfo {
        self.service
            .store
            .get(swarm)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("falta el enjambre"))
    }

    fn member(&self, swarm: &str, name: &str) -> Member {
        self.service
            .store
            .members(swarm)
            .unwrap_or_else(|error| panic!("{error}"))
            .into_iter()
            .find(|member| member.name == name)
            .unwrap_or_else(|| panic!("falta {name}"))
    }

    fn incident(
        &self,
        swarm: &str,
        member: &str,
        kind: &str,
        resets: f64,
    ) -> crate::models::Incident {
        self.service
            .store
            .open_incident(swarm, member, kind, "detalle", resets)
            .unwrap_or_else(|error| panic!("{error}"))
            .0
    }

    fn claim(&self, swarm: &str) {
        let member = self.member(swarm, "bo");
        self.service
            .store
            .claim(swarm, &member.name, Some(1), false)
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

fn sink(cell: &Arc<Mutex<Vec<String>>>) -> Announce {
    let cell = Arc::clone(cell);
    Arc::new(move |text| {
        cell.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(text.to_string());
    })
}

fn reset_recorder(cell: &Arc<Mutex<Vec<(String, String)>>>) -> ResetLaunches {
    let cell = Arc::clone(cell);
    Arc::new(move |swarm: &str, member: &str| {
        cell.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((swarm.to_string(), member.to_string()));
    })
}

fn reading(cell: &Arc<Mutex<f64>>) -> crate::db::Clock {
    let cell = Arc::clone(cell);
    Arc::new(move || *cell.lock().unwrap_or_else(|error| error.into_inner()))
}

fn fixture() -> Fixture {
    let board = temp_board(100.0);
    let clock = Arc::new(|| 100.0);
    let db = Db::new(board.path.clone(), clock.clone()).unwrap_or_else(|error| panic!("{error}"));
    let service = Arc::new(SwarmService::new(db, "ezs", clock));
    let host = Arc::new(FakeHost::new());
    *host.names.lock().unwrap_or_else(|error| error.into_inner()) = vec![
        "claude".to_string(),
        "opencode".to_string(),
        "codex".to_string(),
    ];
    let said = Arc::new(Mutex::new(Vec::new()));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let resets = Arc::new(Mutex::new(Vec::new()));
    let now = Arc::new(Mutex::new(1000.0));
    let announce = sink(&said);
    let ask = sink(&asked);
    let reset_launches = reset_recorder(&resets);
    let wall = reading(&now);
    let clock = reading(&now);
    let base = std::env::temp_dir().join(format!("ez-swarm-rec-{}", uuid::Uuid::new_v4().simple()));
    let host_dyn: Arc<dyn SwarmHost> = Arc::clone(&host) as Arc<dyn SwarmHost>;
    let recovery = Recovery::with_options(
        service.clone(),
        host_dyn,
        base.join("handoff"),
        announce,
        ask,
        RecoveryOptions {
            reset_launches,
            wall,
            clock,
        },
    );
    Fixture {
        recovery,
        service,
        host,
        said,
        asked,
        resets,
        now,
        base,
        _board: board,
    }
}

#[test]
fn wait_and_unknown_choices() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    let incident = fixture.incident(&swarm, "bo", "quota", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(incident.id, " WAIT ", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "Espero a bo; el resto sigue."
    );
    let saved = fixture
        .service
        .store
        .incident(incident.id)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(saved.state, "waiting");
    assert_eq!(saved.action, "wait");
    assert_eq!(
        fixture
            .recovery
            .resolve(incident.id + 9, "wait", "", "", "")
            .unwrap_err()
            .to_string(),
        "No hay un incidente abierto I10."
    );
    assert_eq!(
        fixture
            .recovery
            .resolve(incident.id, "xyz", "", "", "")
            .unwrap_err()
            .to_string(),
        "Opción desconocida: 'xyz'. Usa: wait, replace, reopen, release, pause."
    );
}

#[test]
fn pause_and_release_choices() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    let paused = fixture.incident(&swarm, "bo", "quota", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(paused.id, "pause", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "Enjambre en pausa. Dime «retoma el enjambre» para seguir."
    );
    assert_eq!(
        fixture
            .service
            .store
            .get(&swarm)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("falta el enjambre"))
            .status,
        "paused"
    );

    fixture
        .service
        .resume(&swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    fixture.claim(&swarm);
    let released = fixture.incident(&swarm, "bo", "closed", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(released.id, "release", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "bo sale del enjambre; su trabajo (T1) queda para su rol."
    );
    assert_eq!(fixture.member(&swarm, "bo").state, "gone");
    assert_eq!(
        fixture
            .recovery
            .resolve(released.id, "wait", "", "", "")
            .unwrap_err()
            .to_string(),
        format!("No hay un incidente abierto I{}.", released.id)
    );
}

#[test]
fn replace_uses_the_suggestion_and_accepts_explicit_agents() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture.claim(&swarm);
    let incident = fixture.incident(&swarm, "bo", "quota", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(incident.id, "replace", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "Reemplazo a bo: opencode sale y entra claude, con el avance guardado."
    );
    assert_eq!(fixture.member(&swarm, "bo").agent, "claude");
    assert_eq!(
        fixture
            .resets
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone(),
        vec![(swarm.clone(), "bo".to_string())]
    );
    let message = fixture
        .service
        .store
        .history(&swarm, 0, 10)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|message| message.recipient == "bo")
        .unwrap_or_else(|| panic!("falta el recado"));
    assert_eq!(message.body, "Reemplazas a opencode: retoma T1.");

    let second = fixture.incident(&swarm, "bo", "quota", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(second.id, "replace", "codex", "gpt-5", "high")
            .unwrap_or_else(|error| panic!("{error}")),
        "Reemplazo a bo: claude sale y entra codex gpt-5, con el avance guardado."
    );
    let member = fixture.member(&swarm, "bo");
    assert_eq!(member.agent, "codex");
    assert_eq!(member.model, "gpt-5");
    assert_eq!(member.effort, "high");

    let third = fixture.incident(&swarm, "bo", "quota", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(third.id, "replace", "nope", "", "")
            .unwrap_err()
            .to_string(),
        "Indica con qué agente lo reemplazo (no conozco 'nope')."
    );
}

#[test]
fn reopen_saves_the_screen_as_handoff() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture.claim(&swarm);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture
        .host
        .set_screen("t1", "primera línea  \n\nsegunda línea\n");
    let incident = fixture.incident(&swarm, "bo", "closed", 0.0);
    assert_eq!(
        fixture
            .recovery
            .resolve(incident.id, "reopen", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "Reabro a bo con opencode; retoma con su avance guardado."
    );
    let message = fixture
        .service
        .store
        .history(&swarm, 0, 10)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|message| message.recipient == "bo")
        .unwrap_or_else(|| panic!("falta el recado"));
    assert!(message.ref_.ends_with(".md"));
    assert!(message.body.ends_with(
        "El avance de la sesión anterior está en el archivo (léelo solo si lo necesitas)."
    ));
    let body = std::fs::read_to_string(&message.ref_).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        body.starts_with("# Avance de bo (opencode)\n\nÚltima pantalla, sin editar:\n\n```text\n")
    );
    assert!(body.contains("primera línea\nsegunda línea"));
    assert_eq!(
        fixture.member(&swarm, "bo").address,
        "",
        "update_member limpia la dirección"
    );
    let saved = fixture
        .service
        .store
        .incident(incident.id)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(saved.state, "resolved");
    assert_eq!(saved.action, "reopen");
}

#[test]
fn inspect_recognizes_a_quota_screen_and_asks() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture
        .host
        .set_screen("t1", "hit your usage limit, available 3:00pm");
    let info = fixture.info(&swarm);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    let member = fixture.member(&swarm, "bo");
    assert!(fixture
        .recovery
        .inspect(&info, &member, false, true)
        .unwrap_or_else(|error| panic!("{error}")));
    let incident = fixture
        .service
        .store
        .member_incident(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(incident.kind, "quota");
    assert_eq!(incident.state, "waiting");
    assert_eq!(fixture.member(&swarm, "bo").state, "blocked");
    let asked = fixture
        .asked
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(
        asked
            .iter()
            .any(|text| text.contains("bo (opencode, desarrollar) se quedó sin uso"))
            && asked.iter().any(|text| text.contains("¿Espero"))
    );
    assert!(!fixture
        .recovery
        .inspect(&info, &member, false, true)
        .unwrap_or_else(|error| panic!("{error}")));
}

#[test]
fn inspect_ignores_compacting_and_reports_plain_errors() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    let info = fixture.info(&swarm);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    let member = fixture.member(&swarm, "bo");
    fixture.host.set_screen("t1", "Compacting conversation…");
    assert!(!fixture
        .recovery
        .inspect(&info, &member, true, true)
        .unwrap_or_else(|error| panic!("{error}")));
    fixture.advance(INSPECT_SECS + 1.0);
    fixture
        .host
        .set_screen("t1", "texto normal sin causa conocida");
    assert!(fixture
        .recovery
        .inspect(&info, &member, true, true)
        .unwrap_or_else(|error| panic!("{error}")));
    let incident = fixture
        .service
        .store
        .member_incident(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(incident.kind, "error");
    assert_eq!(incident.detail, "Herdr marca el agente con error.");
}

#[test]
fn hold_marks_a_working_member_as_back() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture.claim(&swarm);
    fixture
        .service
        .store
        .set_member_state(&swarm, "bo", "blocked")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture.incident(&swarm, "bo", "quota", 0.0);
    let info = fixture.info(&swarm);
    let member = fixture.member(&swarm, "bo");
    assert!(!fixture
        .recovery
        .hold(&info, &member, true)
        .unwrap_or_else(|error| panic!("{error}")));
    let incident = fixture
        .service
        .store
        .member_incident(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(incident.is_none());
    assert_eq!(fixture.member(&swarm, "bo").state, "working");
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(said
        .iter()
        .any(|text| text == "Enjambre: bo (opencode) volvió y retoma T1."));
}

#[test]
fn hold_resumes_a_waiting_member_after_the_reset() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture.incident(&swarm, "bo", "quota", 900.0);
    let info = fixture.info(&swarm);
    let member = fixture.member(&swarm, "bo");
    assert!(fixture
        .recovery
        .hold(&info, &member, false)
        .unwrap_or_else(|error| panic!("{error}")));
    let sent = fixture.host.sent_lines();
    assert_eq!(sent.len(), 1);
    assert!(sent[0]
        .1
        .contains("Ya se restableció tu uso. Sigue con tu trabajo: ezs next"));
}

#[test]
fn renew_reopens_once_and_then_asks() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    fixture.claim(&swarm);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    fixture.host.set_screen("t1", "Prompt is too long");
    let info = fixture.info(&swarm);
    let member = fixture.member(&swarm, "bo");
    assert!(fixture
        .recovery
        .inspect(&info, &member, false, true)
        .unwrap_or_else(|error| panic!("{error}")));
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(said.iter().any(|text| text
        == "Enjambre: bo se quedó sin contexto; lo reabro en una sesión nueva con su avance guardado."));
    assert_eq!(fixture.member(&swarm, "bo").address, "");
    fixture.advance(INSPECT_SECS + 1.0);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
    let member = fixture.member(&swarm, "bo");
    assert!(fixture
        .recovery
        .inspect(&info, &member, false, true)
        .unwrap_or_else(|error| panic!("{error}")));
    let asked = fixture
        .asked
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(asked
        .iter()
        .any(|text| text.contains("se quedó sin contexto otra vez en la misma tarea")));
}

#[test]
fn closed_reports_the_two_causes() {
    let fixture = fixture();
    let swarm = fixture.swarm();
    let info = fixture.info(&swarm);
    let member = fixture.member(&swarm, "bo");
    fixture
        .recovery
        .closed(&info, &member, false)
        .unwrap_or_else(|error| panic!("{error}"));
    let closed = fixture
        .service
        .store
        .member_incident(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el incidente"));
    assert_eq!(closed.kind, "closed");
    assert_eq!(closed.detail, "Su ventana ya no está abierta.");
    let asked = fixture
        .asked
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert!(asked
        .iter()
        .any(|text| text.contains("tiene su ventana cerrada")));
    assert_eq!(
        fixture
            .recovery
            .resolve(closed.id, "release", "", "", "")
            .unwrap_or_else(|error| panic!("{error}")),
        "bo sale del enjambre; su trabajo (nada pendiente) queda para su rol."
    );
    assert_eq!(fixture.member(&swarm, "bo").state, "gone");
}
