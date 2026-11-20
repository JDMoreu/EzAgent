//! El disparo del reintento automático (§4) desde el vigilante.

use std::sync::Mutex;

use super::*;
use crate::subagents::AutoRetry;

/// Un gancho falso: anota los registros y devuelve el nombre preparado.
struct FakeAuto {
    result: Option<String>,
    calls: Mutex<Vec<String>>,
}

impl FakeAuto {
    fn new(result: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            result: result.map(str::to_string),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("llamadas").clone()
    }
}

impl AutoRetry for FakeAuto {
    fn auto_retry(&self, record: &Record, cause: &str) -> Result<Option<String>, String> {
        self.calls
            .lock()
            .expect("llamadas")
            .push(format!("{}|{cause}", record.name));
        Ok(self.result.clone())
    }
}

#[test]
fn un_limite_bloqueado_dispara_el_reintento_automatico() {
    let root = temp_root("auto-si");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(1_700_000_000_000)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "Free limit reached. Resets at 7:00 p.m.");
    let hook = FakeAuto::new(Some("dev1-opencode-r1"));
    let mut watcher = make_watcher(&fake, Registry::at(&root))
        .with_auto_retry(Arc::clone(&hook) as Arc<dyn AutoRetry>);
    watcher.tick();
    let tick = watcher.tick();
    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    assert_eq!(tick.retried, vec!["dev1-opencode-r1".to_string()]);
    assert_eq!(hook.calls().len(), 1);
    assert!(hook.calls()[0].starts_with("dev1-opencode|límite de uso"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_permiso_el_gancho_no_lanza_nada() {
    let root = temp_root("auto-no");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(1_700_000_000_000)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "Free limit reached. Resets at 7:00 p.m.");
    let hook = FakeAuto::new(None);
    let mut watcher = make_watcher(&fake, Registry::at(&root))
        .with_auto_retry(Arc::clone(&hook) as Arc<dyn AutoRetry>);
    watcher.tick();
    let tick = watcher.tick();
    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    assert!(tick.retried.is_empty());
    assert_eq!(hook.calls().len(), 1, "se preguntó, pero no se lanzó");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_dialogo_no_dispara_el_reintento_automatico() {
    let root = temp_root("auto-dialogo");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(1_700_000_000_000)
        .with_ls(&live_children("blocked"))
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let hook = FakeAuto::new(Some("dev1-opencode-r1"));
    let mut watcher = make_watcher(&fake, Registry::at(&root))
        .with_auto_retry(Arc::clone(&hook) as Arc<dyn AutoRetry>);
    let tick = watcher.tick();
    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    assert!(tick.retried.is_empty());
    assert!(hook.calls().is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
