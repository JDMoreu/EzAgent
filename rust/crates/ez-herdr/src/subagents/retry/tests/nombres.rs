//! Nombres de la serie `<nombre>-r<n>` y reintento de un fallido.

use super::*;

#[test]
fn el_nombre_continua_la_serie_y_se_recorta_a_32() {
    assert_eq!(retry_name("dev1-opencode", 1), "dev1-opencode-r1");
    assert_eq!(retry_name("dev1-opencode-r1", 2), "dev1-opencode-r2");
    assert_eq!(
        retry_name("qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq", 12)
            .chars()
            .count(),
        32
    );
}

#[test]
fn un_fallido_tambien_se_puede_reintentar() {
    let root = temp_root("fallido");
    let registry = Registry::at(&root);
    let mut old = old_record("dev1-opencode", "t11");
    old.state = TaskState::Failed;
    old.history.clear();
    registry.create(&old).expect("viejo fallido");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route("hc:run", vec![run_ok("dev1-opencode-r1", "t20")]);
    let herdr = herdr_of(&fake);
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request(old),
        &registry,
    )
    .expect("reintento");
    assert_eq!(outcome.row["name"], "dev1-opencode-r1");
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("viejo");
    assert_eq!(saved.state, TaskState::Failed);
    let new = registry
        .get("t6h", "dev1-opencode-r1")
        .expect("leer")
        .expect("nuevo");
    assert_eq!(new.retry_of.as_deref(), Some("dev1-opencode"));
    let _ = std::fs::remove_dir_all(&root);
}
