//! Pruebas de la delegación: el rechazo, el nombre, las etiquetas y el
//! resultado (§4–§7 del diseño).
//!
//! El doble de Herdr (`Fake`) registra cada argv y responde lo preparado; no
//! se ejecuta `hc` ni `herdr-app` reales.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{json, Map, Value};

use crate::driver::Choice;
use crate::herdr::{AgentLaunch, Completed, Herdr, HerdrRuntime, LaunchSupport, RunError};
use crate::subagents::{DelegateState, Registry, TaskState, WatcherSetup};

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

    fn name_taken(name: &str) -> Self {
        Self {
            rc: 1,
            stdout: json!({
                "error": {
                    "kind": "name_taken",
                    "message": format!("a live terminal is already named \"{name}\""),
                }
            })
            .to_string(),
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
    /// `hc` no está instalado.
    missing: bool,
}

impl Fake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(FakeState::default()),
            missing: false,
        })
    }

    fn without_hc() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(FakeState::default()),
            missing: true,
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

    fn argv_of(&self, program: &str, sub: &str) -> Option<Vec<String>> {
        self.calls().into_iter().find(|call| {
            let stem = Path::new(call.first().map(String::as_str).unwrap_or(""))
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            stem == program && call.get(1).map(String::as_str) == Some(sub)
        })
    }

    fn count_of(&self, program: &str, sub: &str) -> usize {
        self.calls()
            .into_iter()
            .filter(|call| {
                let stem = Path::new(call.first().map(String::as_str).unwrap_or(""))
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                stem == program && call.get(1).map(String::as_str) == Some(sub)
            })
            .count()
    }
}

/// La ruta de la llamada: `hc:<sub>` o `app:<acción>`.
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
        if self.missing {
            return None;
        }
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

/// El lanzador falso: siempre el mismo comando y deja el encargo anotado.
#[derive(Clone, Default)]
struct Launcher {
    task: Arc<Mutex<String>>,
    /// Si el encargo se entregó (por omisión, sí).
    sent: Option<bool>,
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
        launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        *self.task.lock().expect("tarea") = task.to_string();
        json!({
            "ok": true,
            "task_sent": self.sent.unwrap_or(true),
            "warnings": launch.warnings,
        })
        .as_object()
        .cloned()
        .expect("objeto")
    }
}

/// Configuración con las rutas de Herdr y una carpeta de trabajo.
fn config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hc_path", json!("hc"));
    cfg.set("herdr_app_path", json!("herdr-app"));
    cfg.set("default_cwd", json!("C:\\ws"));
    cfg
}

fn herdr_de(fake: &Arc<Fake>) -> Herdr {
    let runtime: Arc<dyn HerdrRuntime> = fake.clone();
    Herdr::with_runtime(config(), runtime)
}

/// El pedido de siempre: `developer` que delega en un OpenCode.
fn request() -> DelegateRequest {
    DelegateRequest {
        role: "developer".to_string(),
        agent: "opencode".to_string(),
        brief: "arregla el parser".to_string(),
        parent: "t6h".to_string(),
        ..DelegateRequest::default()
    }
}

/// La validación del catálogo: todo válido, sin avisos.
fn accepted(_agent: &str, model: Option<&str>, effort: Option<&str>) -> Result<Choice, String> {
    Ok(Choice {
        key: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
        warnings: Vec::new(),
    })
}

/// La respuesta de `hc run` que crea la terminal `t12`.
fn hc_run_ok() -> Reply {
    Reply::json(&json!({
        "address": "t12",
        "name": "dev1-opencode",
        "parent": "t6h",
        "labels": {"agent": "opencode"},
        "state": "live",
    }))
}

/// La respuesta de `hc get` con el padre y el nombre reales.
fn hc_get_ok(name: &str) -> Reply {
    Reply::json(&json!({
        "address": "t12",
        "name": name,
        "parent": "t6h",
        "labels": {"agent": "opencode", "ez.role": "developer"},
        "state": "live",
    }))
}

/// El valor que sigue a una bandera del argv.
fn arg_after(argv: &[String], flag: &str) -> Option<String> {
    argv.iter()
        .position(|part| part == flag)
        .and_then(|index| argv.get(index + 1))
        .cloned()
}

// ------------------------------------------------------------------- rechazos

#[test]
fn un_rechazo_del_catalogo_no_crea_nada() {
    let fake = Fake::new();
    let herdr = herdr_de(&fake);
    let result = herdr.delegate(
        Launcher::default(),
        &|_, _, _| Err("No conozco el agente «nada».".to_string()),
        &request(),
        None,
    );
    match result {
        Err(DelegateError::Rejected(message)) => {
            assert_eq!(message, "No conozco el agente «nada».");
        }
        other => panic!("se esperaba un rechazo: {other:?}"),
    }
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

#[test]
fn sin_hc_no_se_delega() {
    let fake = Fake::without_hc();
    let herdr = herdr_de(&fake);
    let result = herdr.delegate(Launcher::default(), &accepted, &request(), None);
    match result {
        Err(DelegateError::Unavailable(message)) => {
            assert!(message.contains("hc"), "{message}");
        }
        other => panic!("se esperaba que no hubiera Herdr: {other:?}"),
    }
    assert!(fake.calls().is_empty());
}

// --------------------------------------------------------------------- nombre

#[test]
fn el_nombre_automatico_suma_los_subagentes_del_padre() {
    let fake = Fake::new()
        .route(
            "hc:ls",
            vec![Reply::json(&json!({
                "terminals": [{"address": "t1"}, {"address": "t2"}],
            }))],
        )
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev3-opencode")]);
    let herdr = herdr_de(&fake);
    let result = herdr
        .delegate(Launcher::default(), &accepted, &request(), None)
        .expect("delegación");
    assert_eq!(result["name"], "dev3-opencode");
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert_eq!(arg_after(&argv, "--name").as_deref(), Some("dev3-opencode"));
    let ls = fake.argv_of("hc", "ls").expect("hc ls");
    assert!(ls.contains(&"--all".to_string()));
    assert!(ls.contains(&"ez.parent=t6h".to_string()));
    assert!(ls.contains(&"ez.role=developer".to_string()));
}

#[test]
fn el_numero_del_nombre_cuenta_las_terminales_del_padre() {
    let fake = Fake::new()
        .route(
            "hc:ls",
            vec![Reply::json(&json!({
                "terminals": [{"address": "t1"}, {"address": "t2"}, {"address": "t3"}],
            }))],
        )
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    herdr
        .delegate(Launcher::default(), &accepted, &request(), None)
        .expect("delegación");
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert_eq!(arg_after(&argv, "--name").as_deref(), Some("dev4-opencode"));
}

#[test]
fn un_nombre_ocupado_prueba_el_siguiente() {
    let fake = Fake::new()
        .route(
            "hc:run",
            vec![Reply::name_taken("dev1-opencode"), hc_run_ok()],
        )
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let result = herdr
        .delegate(Launcher::default(), &accepted, &request(), None)
        .expect("delegación");
    assert_eq!(fake.count_of("hc", "run"), 2);
    assert_eq!(result["ok"], true);
}

#[test]
fn un_nombre_pedido_que_está_ocupado_es_un_rechazo() {
    let fake = Fake::new().route("hc:run", vec![Reply::name_taken("mio")]);
    let herdr = herdr_de(&fake);
    let mut request = request();
    request.name = Some("mio".to_string());
    let result = herdr.delegate(Launcher::default(), &accepted, &request, None);
    match result {
        Err(DelegateError::Rejected(message)) => {
            assert!(message.contains("already named"), "{message}");
        }
        other => panic!("se esperaba un rechazo: {other:?}"),
    }
    assert_eq!(fake.count_of("hc", "run"), 1);
}

// ------------------------------------------------------------ etiquetas y tarea

#[test]
fn el_hc_run_lleva_el_padre_y_las_etiquetas() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let mut request = request();
    request.model = Some("deepseek-v4.1-flash".to_string());
    request.effort = Some("high".to_string());
    request.outlive_parent = true;
    herdr
        .delegate(Launcher::default(), &accepted, &request, None)
        .expect("delegación");
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert_eq!(arg_after(&argv, "--parent").as_deref(), Some("t6h"));
    assert!(argv.contains(&"agent=opencode".to_string()));
    assert!(argv.contains(&"ez.agent=opencode".to_string()));
    assert!(argv.contains(&"ez.role=developer".to_string()));
    assert!(argv.contains(&"ez.parent=t6h".to_string()));
    assert!(argv.contains(&"ez.model=deepseek-v4.1-flash".to_string()));
    assert!(argv.contains(&"ez.effort=high".to_string()));
    assert!(argv.contains(&"--outlive-parent".to_string()));
    // DECISION: por omisión `hc` no avisa al padre (los avisos los da EzAgent).
    assert!(argv.contains(&"--no-notify".to_string()));
}

#[test]
fn sin_modelo_ni_esfuerzo_no_hay_etiquetas_de_ellos() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    herdr
        .delegate(Launcher::default(), &accepted, &request(), None)
        .expect("delegación");
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert!(!argv.iter().any(|part| part.starts_with("ez.model=")));
    assert!(!argv.iter().any(|part| part.starts_with("ez.effort=")));
}

#[test]
fn el_subagente_recibe_el_primer_mensaje() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let launcher = Launcher::default();
    let task = Arc::clone(&launcher.task);
    herdr
        .delegate(launcher, &accepted, &request(), None)
        .expect("delegación");
    let sent = task.lock().expect("tarea").clone();
    assert!(sent.starts_with(
        "Eres un subagente de EzAgent con el papel «developer», lanzado por la terminal t6h."
    ));
    assert!(sent.ends_with("Encargo:\narregla el parser"));
}

// -------------------------------------------------------------------- resultado

#[test]
fn el_resultado_informa_lo_que_quedo() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let result = herdr
        .delegate(Launcher::default(), &accepted, &request(), None)
        .expect("delegación");
    assert_eq!(result["ok"], true);
    assert_eq!(result["terminal"], "t12");
    assert_eq!(result["name"], "dev1-opencode");
    assert_eq!(result["agent"], "opencode");
    assert_eq!(result["label"], "OpenCode");
    assert_eq!(result["role"], "developer");
    assert_eq!(result["parent"], "t6h");
    assert_eq!(result["show"], "tree");
    assert_eq!(result["placed"], "tree");
    assert_eq!(result["task_sent"], true);
    assert_eq!(result["warnings"], json!([]));
}

#[test]
fn los_avisos_del_catalogo_suman_a_los_del_lanzamiento() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("dev1-opencode")]);
    let herdr = herdr_de(&fake);
    let result = herdr
        .delegate(
            Launcher::default(),
            &|_agent, model, effort| {
                Ok(Choice {
                    key: "opencode".to_string(),
                    label: "OpenCode".to_string(),
                    model: model.map(str::to_string),
                    effort: effort.map(str::to_string),
                    warnings: vec!["El catálogo de OpenCode no lista modelos.".to_string()],
                })
            },
            &request(),
            None,
        )
        .expect("delegación");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0], "El catálogo de OpenCode no lista modelos.");
}

// ------------------------------------------------------------------- estado

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
    let result = herdr
        .delegate(
            Launcher::default(),
            &accepted,
            &request(),
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
        .route("hc:run", vec![hc_run_ok(), Reply::name_taken("ez-watch-a-t6h")])
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
