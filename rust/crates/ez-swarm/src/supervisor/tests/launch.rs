//! Lanzamiento de miembros: un fallo sin terminal los da por perdidos; un
//! diálogo en pantalla (la confianza en la carpeta) conserva la terminal.

use serde_json::{Map, Value};

use super::{fixture, one_member_swarm, wait_for};

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
        include_str!("../../../../ez-herdr/tests/permission/claude_trust_numbered.txt"),
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
