//! Pruebas de `ez retry`: nombres, registros enlazados y terminal vieja.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{json, Map, Value};

use crate::driver::Choice;
use crate::herdr::{AgentLaunch, Completed, Herdr, HerdrRuntime, LaunchSupport, RunError};
use crate::subagents::{Change, Record, Registry, TaskState};

use super::*;

/// Respuesta preparada de un proceso.
#[derive(Clone)]
struct Reply {
    rc: i64,
    stdout: String,
    stderr: String,
}

impl Reply {
    fn json(value: &Value) -> Self {
        Self {
            rc: 0,
            stdout: value.to_string(),
            stderr: String::new(),
        }
    }

    fn error(message: &str) -> Self {
        Self {
            rc: 1,
            stdout: json!({"error": {"kind": "name_taken", "message": message}}).to_string(),
            stderr: String::new(),
        }
    }
}

#[derive(Default)]
struct FakeState {
    calls: Vec<Vec<String>>,
    routes: HashMap<String, Vec<Reply>>,
}

/// Doble de `HerdrRuntime`: registra los argv y reparte respuestas por ruta.
struct Fake {
    inner: Mutex<FakeState>,
}

impl Fake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(FakeState::default()),
        })
    }

    fn route(self: &Arc<Self>, key: &str, replies: Vec<Reply>) -> Arc<Self> {
        self.inner
            .lock()
            .expect("estado")
            .routes
            .insert(key.to_string(), replies);
        Arc::clone(self)
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.inner.lock().expect("estado").calls.clone()
    }

    fn count_of(&self, sub: &str) -> usize {
        self.calls()
            .into_iter()
            .filter(|call| call.get(1).map(String::as_str) == Some(sub))
            .count()
    }

    fn argv_of(&self, sub: &str) -> Option<Vec<String>> {
        self.calls()
            .into_iter()
            .find(|call| call.get(1).map(String::as_str) == Some(sub))
    }
}

fn route_of(argv: &[String]) -> String {
    let stem = Path::new(argv.first().map(String::as_str).unwrap_or(""))
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if stem == "hc" {
        format!("hc:{}", argv.get(1).cloned().unwrap_or_default())
    } else {
        format!("app:{}", argv.get(2).cloned().unwrap_or_default())
    }
}

impl HerdrRuntime for Fake {
    fn run(&self, argv: &[String], _timeout: f64, _anonymous: bool) -> Result<Completed, RunError> {
        let mut state = self.inner.lock().expect("estado");
        state.calls.push(argv.to_vec());
        let key = route_of(argv);
        let reply = match state.routes.get_mut(&key) {
            Some(replies) if !replies.is_empty() => {
                if replies.len() == 1 {
                    replies[0].clone()
                } else {
                    replies.remove(0)
                }
            }
            _ => Reply {
                rc: 0,
                stdout: "{}".to_string(),
                stderr: String::new(),
            },
        };
        Ok(Completed {
            stdout: reply.stdout,
            stderr: reply.stderr,
            returncode: reply.rc,
        })
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        Some(PathBuf::from(program))
    }

    fn monotonic(&self) -> f64 {
        1000.0
    }

    fn next_id(&self) -> String {
        "abcd1234".to_string()
    }

    fn spawn(&self, _name: &str, task: Box<dyn FnOnce() + Send + 'static>) -> bool {
        task();
        true
    }
}

/// El lanzador falso: deja el mensaje recibido y dice que se entregó.
#[derive(Clone, Default)]
struct Launcher {
    task: Arc<Mutex<String>>,
}

impl LaunchSupport for Launcher {
    fn agent_launch(
        &self,
        _agent: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> AgentLaunch {
        AgentLaunch {
            argv: Some(vec!["opencode".to_string()]),
            ..AgentLaunch::default()
        }
    }

    fn wait_shell_prompt(&self, _address: &str) -> bool {
        true
    }

    fn finish_launch(
        &self,
        _address: &str,
        _key: &str,
        _launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        *self.task.lock().expect("tarea") = task.to_string();
        json!({"ok": true, "task_sent": true, "warnings": []})
            .as_object()
            .cloned()
            .expect("objeto")
    }
}

fn config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hc_path", json!("hc"));
    cfg.set("herdr_app_path", json!("herdr-app"));
    cfg.set("default_cwd", json!("C:\\ws"));
    cfg
}

fn herdr_of(fake: &Arc<Fake>) -> Herdr {
    let runtime: Arc<dyn HerdrRuntime> = fake.clone();
    Herdr::with_runtime(config(), runtime)
}

fn temp_root(tag: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "ez-retry-{tag}-{}-{}",
        std::process::id(),
        crate::subagents::epoch_ms()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("carpeta temporal");
    folder
}

fn old_record(name: &str, terminal: &str) -> Record {
    let now = crate::subagents::epoch_ms();
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: "t6h".to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("opencode/glm-5.3".to_string()),
        effort: Some("medium".to_string()),
        role: "developer".to_string(),
        cwd: "C:\\ws".to_string(),
        brief_head: "arregla el parser".to_string(),
        brief_path: Some("C:\\ws\\encargo.md".to_string()),
        retry_of: None,
        state: TaskState::Blocked,
        since_ms: now,
        history: vec![Change {
            state: TaskState::Blocked,
            at_ms: now,
            by: "vigilante".to_string(),
            detail: "límite de uso del plan gratuito (se reinicia a las 19:00)".to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// Crea el registro y lo deja `bloqueado` como lo haría el vigilante.
fn seed_blocked(registry: &Registry, name: &str, terminal: &str) -> Record {
    let mut record = old_record(name, terminal);
    record.state = TaskState::InProgress;
    record.history.clear();
    registry.create(&record).expect("registro viejo");
    registry
        .transition(
            "t6h",
            name,
            TaskState::Blocked,
            "vigilante",
            "límite de uso del plan gratuito (se reinicia a las 19:00)",
            |_| {},
        )
        .expect("bloqueo");
    registry.get("t6h", name).expect("leer").expect("registro")
}

fn accepted(_agent: &str, model: Option<&str>, effort: Option<&str>) -> Result<Choice, String> {
    Ok(Choice {
        key: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
        warnings: Vec::new(),
    })
}

fn run_ok(name: &str, terminal: &str) -> Reply {
    Reply::json(&json!({
        "address": terminal,
        "terminal": terminal,
        "name": name,
        "parent": "t6h",
        "state": "live",
    }))
}

fn get_state(terminal: &str, state: &str) -> Reply {
    Reply::json(&json!({
        "address": terminal,
        "state": state,
        "status": {"state": state},
    }))
}

fn request(old: Record) -> RetryRequest {
    RetryRequest {
        parent: "t6h".to_string(),
        old,
        agent: "opencode".to_string(),
        model: Some("opencode-go/deepseek-v4.1-flash".to_string()),
        effort: Some("high".to_string()),
        brief: "arregla el parser".to_string(),
        brief_path: Some("C:\\ws\\encargo.md".to_string()),
        cause: "límite de uso del plan gratuito (se reinicia a las 19:00)".to_string(),
        keep_terminal: false,
    }
}

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
fn un_reintento_manual_enlaza_los_registros() {
    let root = temp_root("manual");
    let registry = Registry::at(&root);
    let old = seed_blocked(&registry, "dev1-opencode", "t11");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route("hc:run", vec![run_ok("dev1-opencode-r1", "t20")])
        // La terminal vieja ya no está y la nueva queda viva.
        .route("hc:get", vec![get_state("t11", "finished")]);
    let herdr = herdr_of(&fake);
    let launcher = Launcher::default();
    let task = Arc::clone(&launcher.task);
    let outcome = retry(
        &herdr,
        launcher,
        &accepted,
        &request(old.clone()),
        &registry,
    )
    .expect("reintento");

    assert_eq!(outcome.row["name"], "dev1-opencode-r1");
    assert_eq!(outcome.row["retry_of"], "dev1-opencode");
    assert!(!outcome.closed_old);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("viejo");
    assert_eq!(saved.state, TaskState::Failed);
    assert_eq!(
        saved.history.last().expect("cambio").detail,
        "reemplazado por dev1-opencode-r1"
    );
    let new = registry
        .get("t6h", "dev1-opencode-r1")
        .expect("leer")
        .expect("nuevo");
    assert_eq!(new.state, TaskState::InProgress);
    assert_eq!(new.retry_of.as_deref(), Some("dev1-opencode"));
    assert_eq!(new.brief_path.as_deref(), Some("C:\\ws\\encargo.md"));
    assert_eq!(new.terminal, "t20");
    assert_eq!(
        new.model.as_deref(),
        Some("opencode-go/deepseek-v4.1-flash")
    );
    let message = task.lock().expect("tarea").clone();
    assert!(message.contains("Retomas el trabajo de dev1-opencode"));
    assert!(message.contains("Revisa `git status` y el registro"));
    assert!(message.ends_with("arregla el parser"), "{message}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn si_ya_existe_el_r1_el_nombre_es_r2() {
    let root = temp_root("r2");
    let registry = Registry::at(&root);
    seed_blocked(&registry, "dev1-opencode", "t11");
    let mut first = old_record("dev1-opencode-r1", "t12");
    first.state = TaskState::Failed;
    first.retry_of = Some("dev1-opencode".to_string());
    registry.create(&first).expect("r1");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route("hc:run", vec![run_ok("dev1-opencode-r2", "t21")])
        .route("hc:get", vec![get_state("t11", "finished")]);
    let herdr = herdr_of(&fake);
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request(old_record("dev1-opencode", "t11")),
        &registry,
    )
    .expect("reintento");
    assert_eq!(outcome.row["name"], "dev1-opencode-r2");
    assert!(registry
        .get("t6h", "dev1-opencode-r2")
        .expect("leer")
        .is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn la_terminal_vieja_se_cierra_despues_de_que_la_nueva_arranca() {
    let root = temp_root("cierra");
    let registry = Registry::at(&root);
    let old = seed_blocked(&registry, "dev1-opencode", "t11");
    let fake = Fake::new()
        .route(
            "hc:ls",
            vec![Reply::json(&json!({
                "terminals": [{"address": "t11", "state": "live"}]
            }))],
        )
        .route("hc:run", vec![run_ok("dev1-opencode-r1", "t20")])
        .route("hc:get", vec![get_state("t11", "live")])
        .route("hc:kill", vec![Reply::json(&json!({"ok": true}))]);
    let herdr = herdr_of(&fake);
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request(old),
        &registry,
    )
    .expect("reintento");
    assert!(outcome.closed_old);
    let calls: Vec<String> = fake
        .calls()
        .into_iter()
        .filter_map(|call| call.get(1).cloned())
        .collect();
    let run = calls.iter().position(|sub| sub == "run").expect("run");
    let kill = calls.iter().position(|sub| sub == "kill").expect("kill");
    assert!(run < kill, "{calls:?}");
    assert_eq!(
        fake.argv_of("kill")
            .expect("kill")
            .get(2)
            .map(String::as_str),
        Some("t11")
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn con_keep_la_terminal_vieja_no_se_toca() {
    let root = temp_root("keep");
    let registry = Registry::at(&root);
    let old = seed_blocked(&registry, "dev1-opencode", "t11");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route("hc:run", vec![run_ok("dev1-opencode-r1", "t20")])
        .route("hc:get", vec![get_state("t11", "live")]);
    let herdr = herdr_of(&fake);
    let mut retry_request = request(old);
    retry_request.keep_terminal = true;
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &retry_request,
        &registry,
    )
    .expect("reintento");
    assert!(!outcome.closed_old);
    assert_eq!(fake.count_of("kill"), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn si_la_nueva_no_arranca_el_viejo_no_cambia_de_estado() {
    let root = temp_root("falla");
    let registry = Registry::at(&root);
    let old = seed_blocked(&registry, "dev1-opencode", "t11");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route(
            "hc:run",
            vec![Reply {
                rc: 1,
                stdout: String::new(),
                stderr: "no arrancó".to_string(),
            }],
        )
        .route("hc:get", vec![get_state("t11", "live")]);
    let herdr = herdr_of(&fake);
    let error = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request(old),
        &registry,
    )
    .expect_err("debe fallar");
    assert_eq!(error.code(), 1);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("viejo");
    assert_eq!(saved.state, TaskState::Blocked);
    assert_eq!(fake.count_of("kill"), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_nombre_ocupado_a_la_carrera_prueba_el_siguiente() {
    let root = temp_root("carrera");
    let registry = Registry::at(&root);
    let old = seed_blocked(&registry, "dev1-opencode", "t11");
    let fake = Fake::new()
        .route("hc:ls", vec![Reply::json(&json!({"terminals": []}))])
        .route(
            "hc:run",
            vec![
                Reply::error("a live terminal is already named \"dev1-opencode-r1\""),
                run_ok("dev1-opencode-r2", "t20"),
            ],
        )
        .route("hc:get", vec![get_state("t11", "finished")]);
    let herdr = herdr_of(&fake);
    let outcome = retry(
        &herdr,
        Launcher::default(),
        &accepted,
        &request(old),
        &registry,
    )
    .expect("reintento");
    assert_eq!(outcome.row["name"], "dev1-opencode-r2");
    assert_eq!(fake.count_of("run"), 2);
    let _ = std::fs::remove_dir_all(&root);
}
