//! Avisos del vigilante a un `NoticeSink` con un padre que no es terminal.

use std::sync::Mutex;

use super::*;

/// Un `NoticeSink` que apunta lo entregado, como hará la app.
#[derive(Default)]
struct RecordingSink {
    delivered: Mutex<Vec<(Option<String>, String)>>,
}

impl NoticeSink for RecordingSink {
    fn deliver(&self, record: Option<&Record>, text: &str) -> bool {
        self.delivered
            .lock()
            .expect("sink")
            .push((record.map(|record| record.name.clone()), text.to_string()));
        true
    }
}

#[test]
fn un_padre_sin_terminal_avisa_al_sink_y_no_usa_hc_send() {
    let root = temp_root("sink");
    let registry = Registry::at(&root);
    registry
        .create(&record("app1-opencode", "t12"))
        .expect("registro");
    // El alta la hizo el padre: no es una novedad (como en la app real).
    registry
        .mark_notified("t6h", "app1-opencode", TaskState::InProgress)
        .expect("alta avisada");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[{"address":"t12","state":"finished","exit_code":1}]}"#)
        // Si el vigilante pidiera la vida del padre, esta respuesta lo dejaría
        // vivo; el modo sin-terminal no debe llamar a `hc get`.
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let sink = Arc::new(RecordingSink::default());
    let mut watcher = make_watcher(&fake, Registry::at(&root))
        .without_parent_terminal()
        .with_notice_sink(Arc::clone(&sink) as Arc<dyn NoticeSink>);
    let tick = watcher.tick();
    assert_eq!(tick.failed, vec!["app1-opencode".to_string()]);
    assert!(
        fake.calls()
            .iter()
            .all(|call| call.first().map(String::as_str) != Some("get")),
        "sin `hc get` cuando el padre no es una terminal"
    );
    assert!(fake.sent().is_empty(), "nada de `hc send`");
    let delivered = sink.delivered.lock().expect("sink").clone();
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert_eq!(delivered[0].0.as_deref(), Some("app1-opencode"));
    assert!(delivered[0].1.contains("falló"), "{}", delivered[0].1);
    let saved = registry
        .get("t6h", "app1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Failed);
    assert_eq!(saved.notified, Some(TaskState::Failed));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_limite_con_el_padre_sin_terminal_bloquea_y_avisa_al_sink() {
    let root = temp_root("sink-limite");
    let registry = Registry::at(&root);
    registry
        .create(&record("app1-opencode", "t12"))
        .expect("registro");
    // El alta la hizo el padre: no es una novedad (como en la app real).
    registry
        .mark_notified("t6h", "app1-opencode", TaskState::InProgress)
        .expect("alta avisada");
    let fake = Fake::new()
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"nadie","state":"live"}"#)
        .with_screen("t12", "Free limit reached. Resets at 7:00 p.m.");
    let sink = Arc::new(RecordingSink::default());
    let mut watcher = make_watcher(&fake, Registry::at(&root))
        .without_parent_terminal()
        .with_notice_sink(Arc::clone(&sink) as Arc<dyn NoticeSink>);
    // La primera vuelta solo apunta la pantalla: hace falta verla quieta.
    let first = watcher.tick();
    assert!(first.blocked.is_empty());
    assert!(sink.delivered.lock().expect("sink").is_empty());
    let tick = watcher.tick();
    assert_eq!(tick.blocked, vec!["app1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "app1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Blocked);
    assert_eq!(saved.notified, Some(TaskState::Blocked));
    assert!(saved
        .history
        .last()
        .expect("cambio")
        .detail
        .contains("límite de uso"));
    let delivered = sink.delivered.lock().expect("sink").clone();
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(delivered[0].1.contains("bloqueado"), "{}", delivered[0].1);
    assert!(fake.sent().is_empty(), "nada de `hc send`");
    let _ = std::fs::remove_dir_all(&root);
}
