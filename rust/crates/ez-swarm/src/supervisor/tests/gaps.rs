//! Supervisor behaviours from `test_swarm_supervisor.py` that had no
//! assertion (TMAP-1): issue announcements and the brief-retry cap.

use super::super::MAX_NUDGES;

#[test]
fn issue_notices_are_announced_once() {
    let fixture = super::fixture();
    let swarm = super::one_member_swarm(&fixture);
    fixture.claim(&swarm);
    fixture
        .service
        .store
        .finish(&swarm, "ana", 1, "No hay repositorio", false, "")
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
    assert_eq!(
        said.iter()
            .filter(|text| text.contains("T1 falló: No hay repositorio"))
            .count(),
        1
    );
    assert!(said.iter().any(|text| text.contains("ana está bloqueado.")));
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    let said = fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    assert_eq!(
        said.iter()
            .filter(|text| text.contains("T1 falló: No hay repositorio"))
            .count(),
        1
    );
}

#[test]
fn the_start_line_retries_up_to_the_cap() {
    let fixture = super::fixture();
    let swarm = super::one_member_swarm(&fixture);
    super::address(&fixture, &swarm);
    fixture
        .supervisor
        .book
        .lock()
        .unbriefed
        .insert((swarm.clone(), "ana".to_string()));
    fixture
        .host
        .set_agents(vec![super::agent_row("t1", "idle")]);
    for attempt in 1..=MAX_NUDGES {
        fixture
            .supervisor
            .tick()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(fixture.host.sent_lines().len(), attempt as usize);
        for nudge in fixture.supervisor.book.lock().nudged.values_mut() {
            nudge.at = -1_000.0;
        }
    }
    fixture
        .supervisor
        .tick()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixture.host.sent_lines().len(), MAX_NUDGES as usize);
}
