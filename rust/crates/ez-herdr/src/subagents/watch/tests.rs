//! Pruebas del vigilante: `hc` y reloj falsos, sin Herdr ni esperas reales.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::subagents::{Record, Registry, TaskState};

use super::*;

/// El estado del doble de `hc`.
#[derive(Default)]
struct FakeState {
    /// El reloj falso, en milisegundos.
    now: u64,
    /// Cada llamada a `hc`.
    calls: Vec<Vec<String>>,
    /// La respuesta de `hc ls`.
    ls: String,
    /// La respuesta de `hc get`.
    get: String,
    /// La respuesta de `hc run` (código, `stdout` y `stderr`).
    run: Option<(i64, String, String)>,
    /// Si `hc send` falla.
    send_fails: bool,
    /// Los mensajes enviados al padre.
    sent: Vec<String>,
}

/// El doble de `hc` y del reloj.
#[derive(Default)]
struct Fake {
    state: Mutex<FakeState>,
}

impl Fake {
    fn new() -> Arc<Fake> {
        Arc::new(Self::default())
    }

    fn with_ls(self: &Arc<Self>, ls: &str) -> Arc<Fake> {
        self.state.lock().expect("estado").ls = ls.to_string();
        Arc::clone(self)
    }

    fn with_get(self: &Arc<Self>, get: &str) -> Arc<Fake> {
        self.state.lock().expect("estado").get = get.to_string();
        Arc::clone(self)
    }

    /// La respuesta de `hc run`.
    fn with_run(self: &Arc<Self>, code: i64, stdout: &str, stderr: &str) -> Arc<Fake> {
        self.state.lock().expect("estado").run =
            Some((code, stdout.to_string(), stderr.to_string()));
        Arc::clone(self)
    }

    fn send_fails(self: &Arc<Self>, fails: bool) -> Arc<Fake> {
        self.state.lock().expect("estado").send_fails = fails;
        Arc::clone(self)
    }

    fn sent(&self) -> Vec<String> {
        self.state.lock().expect("estado").sent.clone()
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.state.lock().expect("estado").calls.clone()
    }
}

impl WatchRuntime for Fake {
    fn hc(&self, args: &[String]) -> Result<HcOutput, String> {
        let mut state = self.state.lock().expect("estado");
        state.calls.push(args.to_vec());
        let sub = args.first().map(String::as_str).unwrap_or("");
        match sub {
            "ls" => Ok(HcOutput {
                code: 0,
                stdout: state.ls.clone(),
                stderr: String::new(),
            }),
            "get" => Ok(HcOutput {
                code: 0,
                stdout: state.get.clone(),
                stderr: String::new(),
            }),
            "run" => {
                let (code, stdout, stderr) =
                    state
                        .run
                        .clone()
                        .unwrap_or((0, "{}".to_string(), String::new()));
                Ok(HcOutput {
                    code,
                    stdout,
                    stderr,
                })
            }
            "send" => {
                if state.send_fails {
                    return Ok(HcOutput {
                        code: 1,
                        stdout: String::new(),
                        stderr: "hc send falló".to_string(),
                    });
                }
                let text = args.get(2).cloned().unwrap_or_default();
                state.sent.push(text);
                Ok(HcOutput {
                    code: 0,
                    stdout: "{}".to_string(),
                    stderr: String::new(),
                })
            }
            other => Err(format!("comando desconocido: {other}")),
        }
    }

    fn now_ms(&self) -> u64 {
        self.state.lock().expect("estado").now
    }

    fn sleep_ms(&self, ms: u64) {
        let mut state = self.state.lock().expect("estado");
        state.now += ms;
    }
}

/// Una carpeta temporal con nombre único.
fn temp_root(tag: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "ez-watch-{tag}-{}-{}",
        std::process::id(),
        epoch_ms()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("carpeta temporal");
    folder
}

/// Un registro en proceso del padre `t6h`.
fn record(name: &str, terminal: &str) -> Record {
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: "t6h".to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("deepseek-v4.1-flash".to_string()),
        effort: None,
        role: "developer".to_string(),
        cwd: "C:\\ws".to_string(),
        brief_head: "arregla el parser".to_string(),
        state: TaskState::InProgress,
        since_ms: epoch_ms(),
        history: Vec::new(),
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// El vigilante de siempre: sin margen y con padre vivo.
fn make_watcher(fake: &Arc<Fake>, registry: Registry) -> Watcher {
    let config = WatchConfig {
        parent: "t6h".to_string(),
        interval_ms: 3_000,
        idle_ms: DEFAULT_IDLE_MS,
        grace_ms: 0,
    };
    Watcher::new(config, registry, Arc::clone(fake) as Arc<dyn WatchRuntime>)
}

#[test]
fn una_terminal_terminada_sin_informe_queda_fallida_y_se_avisa() {
    let root = temp_root("fallida");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[{"address":"t12","state":"finished","exit_code":1}]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();
    assert_eq!(tick.failed, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Failed);
    let detail = saved.history.last().expect("cambio").detail.clone();
    assert_eq!(detail, "la terminal terminó sin informe (código 1)");
    assert_eq!(saved.history.last().expect("cambio").by, "vigilante");
    let sent = fake.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("falló"), "{}", sent[0]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_aviso_no_se_repite() {
    let root = temp_root("repite");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "t12",
            "el subagente terminó",
            |_| {},
        )
        .expect("cambio");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[{"address":"t12","state":"live"}]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();
    assert_eq!(tick.notified, vec!["dev1-opencode".to_string()]);
    assert_eq!(fake.sent().len(), 1);
    let tick = watcher.tick();
    assert!(tick.notified.is_empty());
    assert_eq!(fake.sent().len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_cambio_hecho_por_el_padre_no_se_avisa() {
    let root = temp_root("padre");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "t6h",
            "el padre lo terminó",
            |_| {},
        )
        .expect("cambio");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[{"address":"t12","state":"live"}]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();
    assert!(tick.notified.is_empty());
    assert!(fake.sent().is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_aviso_que_no_se_puede_mandar_se_reintenta() {
    let root = temp_root("reintento");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "t12",
            "el subagente terminó",
            |_| {},
        )
        .expect("cambio");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[{"address":"t12","state":"live"}]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .send_fails(true);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();
    assert!(tick.notified.is_empty());
    assert!(fake.sent().is_empty());
    // La segunda vuelta sí puede mandar el aviso.
    let fake = fake.send_fails(false);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();
    assert_eq!(tick.notified, vec!["dev1-opencode".to_string()]);
    assert_eq!(fake.sent().len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_vigilante_termina_cuando_el_padre_muere() {
    let root = temp_root("padre-muerto");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[]}"#)
        .with_get(r#"{"address":"t6h","state":"finished"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    assert_eq!(watcher.run(), WatchStop::ParentGone);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_vigilante_termina_cuando_no_queda_nada_pendiente() {
    let root = temp_root("inactivo");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let stop = watcher.run();
    assert_eq!(stop, WatchStop::Idle);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_vigilante_usa_la_etiqueta_del_padre_para_preguntar_a_hc() {
    let root = temp_root("ls");
    let fake = Fake::new()
        .with_ls(r#"{"terminals":[]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    let calls = fake.calls();
    let ls = calls
        .iter()
        .find(|call| call.first().map(String::as_str) == Some("ls"))
        .expect("hc ls");
    assert!(ls.contains(&"--all".to_string()));
    assert!(ls.contains(&"ez.parent=t6h".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

// ------------------------------------------------------------- arranque del vigilante

/// La preparación del vigilante de un padre.
fn setup() -> WatcherSetup {
    WatcherSetup {
        parent: "t6h".to_string(),
        root: PathBuf::from("C:\\registros"),
        hc: "hc".to_string(),
        ez: PathBuf::from("ez.exe"),
    }
}

/// El JSON de error que `hc` escribe en `stderr` cuando el nombre está ocupado.
const NAME_TAKEN: &str = r#"{"error":{"hint":"pick another name","kind":"name_taken","message":"a live terminal is already named \"ez-watch-t6h\""}}"#;

#[test]
fn un_vigilante_ya_corriendo_no_es_un_error() {
    let fake = Fake::new().with_run(1, "", NAME_TAKEN);
    assert_eq!(
        ensure_watcher(&*fake, &setup()),
        WatcherState::AlreadyRunning
    );
}

#[test]
fn un_vigilante_ya_corriendo_tampoco_lo_es_con_el_error_por_stdout() {
    let fake = Fake::new().with_run(1, NAME_TAKEN, "");
    assert_eq!(
        ensure_watcher(&*fake, &setup()),
        WatcherState::AlreadyRunning
    );
}

#[test]
fn el_error_por_stderr_dice_el_mensaje_y_no_el_json() {
    let fake = Fake::new().with_run(
        1,
        "",
        r#"{"error":{"kind":"invalid_params","message":"falta el padre"}}"#,
    );
    match ensure_watcher(&*fake, &setup()) {
        WatcherState::Failed(message) => assert_eq!(message, "falta el padre"),
        other => panic!("se esperaba un fallo: {other:?}"),
    }
}

#[test]
fn un_vigilante_nuevo_arranca() {
    let fake = Fake::new().with_run(0, r#"{"address":"t12"}"#, "");
    assert_eq!(ensure_watcher(&*fake, &setup()), WatcherState::Started);
}
