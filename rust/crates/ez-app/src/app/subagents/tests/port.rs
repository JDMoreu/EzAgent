//! El registro de un lanzamiento de la app (`AppHerdrPort`).

use ez_herdr::herdr::Herdr;
use serde_json::{json, Map, Value};

use crate::app::subagents::port::{register_launch, Launch};

use super::*;

fn outcome(row: Value) -> Map<String, Value> {
    row.as_object().cloned().expect("objeto")
}

#[test]
fn a_launch_leaves_a_record_under_the_voice_parent() {
    let temp = TempDir::new("port");
    let registry = Registry::at(temp.0.join("subagentes"));
    let herdr = Herdr::new(VoiceConfig::new());
    let launch = Launch {
        agent: "opencode".to_string(),
        model: Some("glm-5.3".to_string()),
        effort: None,
        cwd: Some("C:\\work".to_string()),
        name: Some("ir-9".to_string()),
        hc_name: None,
        task: "Revisa la rama work/NV-3.".to_string(),
    };
    let row = outcome(json!({
        "ok": true,
        "terminal": "t1",
        "agent": "opencode",
        "model": "glm-5.3",
        "cwd": "C:\\work",
        "launching": true
    }));
    registry
        .create(&record("probe", TaskState::InProgress, VOICE_PARENT))
        .expect("registro directo");
    let created = register_launch(&herdr, &registry, &launch, &row);
    assert!(created.is_ok(), "registro: {created:?}");
    let listed = registry.list_lenient(VOICE_PARENT).expect("lista");
    assert!(
        listed.records.len() == 2,
        "sonda + lanzamiento; inválidos: {:?}",
        listed.invalid
    );
    let saved = registry
        .get(VOICE_PARENT, "ir-9")
        .expect("lectura")
        .expect("registro");
    assert_eq!(saved.terminal, "t1");
    assert_eq!(saved.parent, VOICE_PARENT);
    assert_eq!(saved.agent, "opencode");
    assert_eq!(saved.state, TaskState::InProgress);
    assert_eq!(saved.brief_head, "Revisa la rama work/NV-3.");
    assert_eq!(saved.history[0].by, VOICE_PARENT);
    assert_eq!(saved.notified, None);

    // Un segundo lanzamiento con el mismo nombre visible usa el de §6.
    let again = register_launch(&herdr, &registry, &launch, &row);
    assert!(again.is_ok(), "segundo registro: {again:?}");
    let fallback = registry
        .get(VOICE_PARENT, "voz1-opencode")
        .expect("lectura")
        .expect("nombre automático");
    assert_eq!(fallback.terminal, "t1");
    assert_eq!(
        registry.list(VOICE_PARENT).expect("lista").len(),
        3,
        "dos lanzamientos, dos registros (más la sonda)"
    );

    // Un fallo de `hc` no deja registro.
    let failed = outcome(json!({"ok": false, "error": "sin hc"}));
    assert!(register_launch(&herdr, &registry, &launch, &failed).is_err());
    assert_eq!(registry.list(VOICE_PARENT).expect("lista").len(), 3);

    // Un agente abierto sin encargo tampoco deja registro.
    let empty = Launch {
        agent: "opencode".to_string(),
        model: None,
        effort: None,
        cwd: None,
        name: Some("sin-encargo".to_string()),
        hc_name: None,
        task: String::new(),
    };
    let before = registry.list(VOICE_PARENT).expect("lista").len();
    assert!(register_launch(&herdr, &registry, &empty, &row).is_err());
    assert_eq!(registry.list(VOICE_PARENT).expect("lista").len(), before);
}
