//! Modo autónomo sobre el enjambre (10.18): argumentos de permisos y recuperación.

use std::sync::Arc;

use crate::supervisor::{Supervisor, SupervisorConfig, SwarmHost};

use super::{fixture, one_member_swarm, wait_for};
#[test]
fn autonomous_members_lose_the_permission_skipping_arguments() {
    let fixture = fixture();
    *fixture
        .host
        .swarm_args
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = vec![
        "--yolo".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--allowedTools".to_string(),
        "Bash(ezs *)".to_string(),
    ];
    let swarm = one_member_swarm(&fixture);
    fixture.supervisor.mark_autonomous(&swarm);
    assert!(fixture.supervisor.is_autonomous(&swarm));
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
    assert_eq!(
        launched[0].2.extra_args.clone(),
        Some(vec![
            "--allowedTools".to_string(),
            "Bash(ezs *)".to_string()
        ])
    );
}

#[test]
fn a_plain_swarm_keeps_its_permission_arguments() {
    let fixture = fixture();
    *fixture
        .host
        .swarm_args
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = vec!["--yolo".to_string()];
    let _swarm = one_member_swarm(&fixture);
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
    assert_eq!(launched[0].2.extra_args, Some(vec!["--yolo".to_string()]));
}

#[test]
fn the_autonomous_mode_does_not_ask_for_swarm_incidents() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture.supervisor.mark_autonomous(&swarm);
    let info = fixture
        .service
        .store
        .get(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el enjambre"));
    let member = fixture
        .service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("falta el miembro"));
    fixture
        .supervisor
        .recovery
        .raise_incident(&info, &member, "quota", "sin saldo", 0.0)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        fixture
            .asked
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty(),
        "el modo autónomo da la única noticia"
    );
    assert!(fixture
        .said
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_empty());
}

#[test]
fn a_restart_remembers_the_autonomous_swarms() {
    let fixture = fixture();
    let swarm = one_member_swarm(&fixture);
    fixture.supervisor.mark_autonomous(&swarm);
    let host: Arc<dyn SwarmHost> = Arc::clone(&fixture.host) as Arc<dyn SwarmHost>;
    let mut config = SupervisorConfig::new(fixture.base.join("seats"), Arc::new(|_| {}));
    config.wall = Arc::new(|| 100.0);
    let restarted = Supervisor::new(Arc::clone(&fixture.service), host, config);
    assert!(restarted.is_autonomous(&swarm));
}
