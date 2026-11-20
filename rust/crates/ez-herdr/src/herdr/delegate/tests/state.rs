//! El estado de la delegación: el registro del subagente y el vigilante.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::subagents::{DelegateState, Registry, TaskState, WatcherSetup};

use super::*;

/// Una carpeta temporal con nombre único para el registro.
fn temp_root(tag: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("ez-delegate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("carpeta temporal");
    folder
}

/// El estado de una delegación: el registro y, si se quiere, el vigilante.
fn state(root: &Path, watcher: bool) -> DelegateState {
    DelegateState {
        registry: Registry::at(root),
        watcher: watcher.then(|| WatcherSetup {
            parent: "t6h".to_string(),
            root: root.to_path_buf(),
            hc: "hc".to_string(),
            ez: PathBuf::from("ez.exe"),
        }),
    }
}

#[test]
fn la_delegacion_guarda_el_registro_del_subagente() {
    let root = temp_root("registro");
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let mut request = request();
    request.brief_path = Some("C:\\ws\\encargo.md".to_string());
    let result = herdr
        .delegate(
            Launcher::default(),
            &accepted,
            &request,
            Some(&state(&root, false)),
        )
        .expect("delegación");
    assert_eq!(result["state"], "en_proceso");
    let registry = Registry::at(&root);
    let record = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(record.state, TaskState::InProgress);
    assert_eq!(record.terminal, "t12");
    assert_eq!(record.agent, "opencode");
    assert_eq!(record.label, "OpenCode");
    assert_eq!(record.role, "developer");
    assert_eq!(record.brief_head, "arregla el parser");
    assert_eq!(record.brief_path.as_deref(), Some("C:\\ws\\encargo.md"));
    assert!(record.retry_of.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn si_el_encargo_no_llega_el_registro_queda_fallido() {
    let root = temp_root("fallido");
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let launcher = Launcher {
        sent: Some(false),
        ..Launcher::default()
    };
    let result = herdr
        .delegate(launcher, &accepted, &request(), Some(&state(&root, false)))
        .expect("delegación");
    assert_eq!(result["state"], "fallido");
    let registry = Registry::at(&root);
    let record = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(record.state, TaskState::Failed);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn la_delegacion_arranca_el_vigilante() {
    let root = temp_root("vigilante");
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok(), hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    herdr
        .delegate(
            Launcher::default(),
            &accepted,
            &request(),
            Some(&state(&root, true)),
        )
        .expect("delegación");
    let calls: Vec<Vec<String>> = fake
        .calls()
        .into_iter()
        .filter(|call| call.get(1).map(String::as_str) == Some("run"))
        .collect();
    assert_eq!(calls.len(), 2);
    let watch = &calls[1];
    assert!(watch.contains(&"--no-parent".to_string()));
    assert!(watch.contains(&"--no-notify".to_string()));
    assert!(watch.contains(&"--bg".to_string()));
    assert!(watch.contains(&"ez-watch-a-t6h".to_string()));
    assert!(watch.contains(&"ez.watch=t6h".to_string()));
    assert!(watch.contains(&"watch".to_string()));
    assert!(watch.contains(&"--parent".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_vigilante_que_ya_corre_no_es_un_error() {
    let root = temp_root("nombre");
    let fake = Fake::new()
        .route(
            "hc:run",
            vec![hc_run_ok(), Reply::name_taken("ez-watch-a-t6h")],
        )
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let result = herdr
        .delegate(
            Launcher::default(),
            &accepted,
            &request(),
            Some(&state(&root, true)),
        )
        .expect("delegación");
    assert_eq!(result["ok"], true);
    assert_eq!(result["warnings"], json!([]));
    let _ = std::fs::remove_dir_all(&root);
}
