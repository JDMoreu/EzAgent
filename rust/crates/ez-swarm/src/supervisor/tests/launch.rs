//! Lanzamiento de miembros: un fallo sin terminal los da por perdidos; un
//! diálogo en pantalla (la confianza en la carpeta) conserva la terminal.

use ez_herdr::herdr::Placement;
use serde_json::{Map, Value};

use super::{fixture, one_member_swarm, wait_for};
use crate::service::MemberSpec;

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

/// Dos miembros simultáneos: el primero con dirección, el segundo por abrir.
fn two_member_swarm(fixture: &super::Fixture) -> String {
    let specs = [
        MemberSpec {
            name: "ana".to_string(),
            role: "plan".to_string(),
            agent: "claude".to_string(),
            task: "Planear".to_string(),
            parallel: true,
            ..MemberSpec::default()
        },
        MemberSpec {
            name: "bo".to_string(),
            role: "dev".to_string(),
            agent: "opencode".to_string(),
            model: "deepseek-v4".to_string(),
            effort: "high".to_string(),
            task: "Portar".to_string(),
            parallel: true,
        },
    ];
    fixture
        .service
        .start("Migrar", &specs, "", None)
        .unwrap_or_else(|error| panic!("{error}"))
        .0
}

/// Tres miembros: la raíz (ana) abre primero, luego bo y cy.
fn three_member_swarm(fixture: &super::Fixture) -> String {
    let specs = [
        MemberSpec::new("ana", "plan", "claude"),
        MemberSpec {
            name: "bo".to_string(),
            role: "dev".to_string(),
            agent: "claude".to_string(),
            task: "Portar".to_string(),
            parallel: true,
            ..MemberSpec::default()
        },
        MemberSpec {
            name: "cy".to_string(),
            role: "qa".to_string(),
            agent: "claude".to_string(),
            task: "Probar".to_string(),
            parallel: true,
            ..MemberSpec::default()
        },
    ];
    fixture
        .service
        .start("Migrar", &specs, "", None)
        .unwrap_or_else(|error| panic!("{error}"))
        .0
}

#[test]
fn the_first_member_is_launched_without_a_parent() {
    let fixture = fixture();
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
    let request = &launched[0].2;
    assert_eq!(request.parent, None);
    assert_eq!(request.hc_name, None);
    assert!(request.labels.is_empty());
    assert_eq!(request.show, Placement::Tree);
    assert!(!request.notify);
    assert!(!request.outlive_parent);
}

#[test]
fn later_members_hang_from_the_root_with_the_contract_labels() {
    let fixture = fixture();
    let swarm = two_member_swarm(&fixture);
    fixture
        .service
        .store
        .set_address(&swarm, "ana", "t1")
        .unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(launched[0].0, "opencode");
    let request = &launched[0].2;
    assert_eq!(request.parent.as_deref(), Some("t1"));
    assert_eq!(request.hc_name.as_deref(), Some("bo"));
    assert_eq!(request.show, Placement::Tab);
    assert!(!request.notify);
    assert!(request.outlive_parent);
    let label = |key: &str| request.labels.get(key).and_then(Value::as_str);
    assert_eq!(label("ez.swarm"), Some(swarm.as_str()));
    assert_eq!(label("ez.role"), Some("dev"));
    assert_eq!(label("ez.agent"), Some("opencode"));
    assert_eq!(label("ez.parent"), Some("t1"));
    assert_eq!(label("ez.model"), Some("deepseek-v4"));
    assert_eq!(label("ez.effort"), Some("high"));
}

#[test]
fn a_root_without_address_leaves_the_member_without_a_parent() {
    let fixture = fixture();
    let swarm = two_member_swarm(&fixture);
    fixture
        .service
        .store
        .set_member_state(&swarm, "ana", "gone")
        .unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(launched[0].0, "opencode");
    assert_eq!(launched[0].2.parent, None);
}

#[test]
fn a_reopened_non_root_member_hangs_from_the_root() {
    let fixture = fixture();
    let swarm = two_member_swarm(&fixture);
    for (name, address) in [("ana", "t1"), ("bo", "t2")] {
        fixture
            .service
            .store
            .set_address(&swarm, name, address)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let incident = fixture
        .service
        .store
        .open_incident(&swarm, "bo", "error", "detalle", 0.0)
        .unwrap_or_else(|error| panic!("{error}"))
        .0;
    fixture
        .supervisor
        .recovery
        .resolve(incident.id, "reopen", "", "", "")
        .unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(launched[0].2.parent.as_deref(), Some("t1"));
}

/// Reabrir al primer miembro (la raíz) no debe invertir el árbol: sigue sin padre.
/// Mata la mutación «la raíz puede ser el propio miembro»: si al buscar la raíz
/// se excluyera a ana, colgaría de la dirección de bo (t2).
#[test]
fn reopening_the_root_leaves_it_without_a_parent() {
    let fixture = fixture();
    let swarm = two_member_swarm(&fixture);
    for (name, address) in [("ana", "t1"), ("bo", "t2")] {
        fixture
            .service
            .store
            .set_address(&swarm, name, address)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let incident = fixture
        .service
        .store
        .open_incident(&swarm, "ana", "error", "detalle", 0.0)
        .unwrap_or_else(|error| panic!("{error}"))
        .0;
    fixture
        .supervisor
        .recovery
        .resolve(incident.id, "reopen", "", "", "")
        .unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(launched[0].2.name, "ana · enjambre");
    assert_eq!(launched[0].2.parent, None);
    assert!(launched[0].2.labels.is_empty());
    assert_eq!(launched[0].2.show, Placement::Tree);
    assert!(!launched[0].2.outlive_parent);
}

/// Con la raíz aún sin dirección, el tercer miembro no cuelga del segundo.
#[test]
fn a_third_member_does_not_hang_from_the_second_when_the_root_has_no_address() {
    let fixture = fixture();
    let swarm = three_member_swarm(&fixture);
    fixture
        .service
        .store
        .set_address(&swarm, "bo", "t2")
        .unwrap_or_else(|error| panic!("{error}"));
    // ana (raíz) no abrió todavía: se marca gone para que solo se lance cy.
    fixture
        .service
        .store
        .set_member_state(&swarm, "ana", "gone")
        .unwrap_or_else(|error| panic!("{error}"));
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
    assert_eq!(launched[0].2.name, "cy · enjambre");
    // Con la regla vieja, cy colgaba de bo (t2).
    assert_eq!(launched[0].2.parent, None);
}
