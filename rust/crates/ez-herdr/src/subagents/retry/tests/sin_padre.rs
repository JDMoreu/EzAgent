//! Reintento de un hijo de la app: el padre no es una terminal.

use super::*;

#[test]
fn un_padre_sin_terminal_lanza_sin_parent_y_enlaza_el_registro() {
    let root = temp_root("sin-padre");
    let registry = Registry::at(&root);
    let old = seed_blocked_at(&registry, "voz", "app1-opencode", "t11");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        // `voz` no existe como terminal: `hc get` lo dice con not_found.
        .route("hc:get", vec![not_found("voz")])
        .route(
            "app:tab.new",
            vec![Reply::json(&json!({
                "terminal": "t20",
                "address": "t20",
                "tab": "tab20",
            }))],
        )
        .route("hc:send", vec![Reply::json(&json!({"ok": true}))]);
    let herdr = herdr_of(&fake);
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request_at("voz", old),
        &registry,
    )
    .expect("reintento");

    assert_eq!(outcome.row["parent"], "voz");
    assert_eq!(outcome.row["retry_of"], "app1-opencode");
    assert_eq!(
        fake.count_of("run"),
        0,
        "un padre sin terminal no usa `hc run --parent`"
    );
    for call in fake.calls() {
        assert!(
            !call.iter().any(|arg| arg == "--parent"),
            "ninguna llamada lleva --parent: {call:?}"
        );
    }
    let new = registry
        .get("voz", "app1-opencode-r1")
        .expect("leer")
        .expect("nuevo");
    assert_eq!(new.parent, "voz");
    assert_eq!(new.retry_of.as_deref(), Some("app1-opencode"));
    assert_eq!(new.terminal, "t20");
    assert_eq!(new.state, TaskState::InProgress);
    let saved_old = registry
        .get("voz", "app1-opencode")
        .expect("leer")
        .expect("viejo");
    assert_eq!(saved_old.state, TaskState::Failed);
    let _ = std::fs::remove_dir_all(&root);
}
