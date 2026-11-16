//! Pruebas nuevas del contrato de lectura y plazos de la sesión.

use super::*;

#[test]
fn confirm_submitted_pantalla_vacia_no_confirma() {
    let mut state = HostState {
        info: idle_info("claude"),
        ..HostState::default()
    };
    state.screen = String::new();
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.confirm_submitted("t1", "arregla el bug del parser"));
    assert!(session.host().state().keys.is_empty());
}

#[test]
fn wait_ready_deadline_includes_slow_reads() {
    let clock = Arc::new(FakeClock::new());
    let state = HostState {
        info: json!({"agent": "codex", "state": "working"})
            .as_object()
            .cloned()
            .expect("objeto"),
        slow_get: 40.0,
        ..HostState::default()
    };
    let host = FakeHost::new(state).with_clock(Arc::clone(&clock));
    let session = AgentSession::with_clock(
        host,
        VoiceConfig::new(),
        Arc::clone(&clock) as Arc<dyn SessionClock>,
    );
    assert!(!session.wait_ready("t1", 30.0));
    assert_eq!(session.host().state().gets, 1);
}
