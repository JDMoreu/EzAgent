//! Pruebas de los subagentes: el nombre que acepta `hc` y la ubicación.
//!
//! El doble de Herdr (`Fake`) registra cada argv y responde lo preparado; no
//! se ejecuta `hc` ni `herdr-app` reales.

mod launch;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{json, Map, Value};

use crate::herdr::{
    hc_slug, AgentLaunch, Completed, Herdr, HerdrRuntime, LaunchSupport, Placement, RunAgent,
    RunError,
};

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
            stdout: json!({"error": {"message": message}}).to_string(),
            stderr: String::new(),
        }
    }

    fn plain(text: &str) -> Self {
        Self {
            rc: 1,
            stdout: String::new(),
            stderr: text.to_string(),
        }
    }

    /// El error de «nombre ocupado» como lo escribe `hc`: por `stderr`.
    fn name_taken_on_stderr(name: &str) -> Self {
        Self {
            rc: 1,
            stdout: String::new(),
            stderr: json!({
                "error": {
                    "kind": "name_taken",
                    "message": format!("a live terminal is already named \"{name}\""),
                }
            })
            .to_string(),
        }
    }
}

struct FakeState {
    calls: Vec<Vec<String>>,
    routes: HashMap<String, Vec<Reply>>,
    clock: f64,
    step: f64,
}

/// Doble de `HerdrRuntime`: registra los argv y reparte respuestas por ruta.
struct Fake {
    inner: Mutex<FakeState>,
}

impl Fake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(FakeState {
                calls: Vec::new(),
                routes: HashMap::new(),
                clock: 1000.0,
                step: 0.0,
            }),
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

    /// El reloj avanza con cada llamada (para que los reintentos no esperen).
    fn ticking(self: &Arc<Self>, step: f64) -> Arc<Self> {
        self.inner.lock().expect("estado").step = step;
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

    fn app_calls(&self, action: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|call| call.get(2).map(String::as_str) == Some(action))
            .collect()
    }
}

/// La ruta de la llamada: `hc:<sub>`, `app:ls` o `app:<acción>`.
fn route_of(argv: &[String]) -> String {
    let stem = Path::new(argv.first().map(String::as_str).unwrap_or(""))
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if stem == "hc" {
        format!("hc:{}", argv.get(1).cloned().unwrap_or_default())
    } else if argv.get(1).map(String::as_str) == Some("ls") {
        "app:ls".to_string()
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
        state.clock += state.step;
        Ok(Completed {
            stdout: reply.stdout,
            stderr: reply.stderr,
            returncode: reply.rc,
        })
    }

    /// El doble devuelve el mismo nombre: el argv se lee tal cual y nunca se
    /// busca el Herdr instalado en la máquina.
    fn which(&self, program: &str) -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from(program))
    }

    fn monotonic(&self) -> f64 {
        self.inner.lock().expect("estado").clock
    }

    fn next_id(&self) -> String {
        "abcd1234".to_string()
    }

    fn spawn(&self, _name: &str, task: Box<dyn FnOnce() + Send + 'static>) -> bool {
        task();
        true
    }
}

/// El driver de agente: siempre el mismo comando.
#[derive(Clone)]
struct Driver {
    argv: Vec<String>,
}

impl Default for Driver {
    fn default() -> Self {
        Self {
            argv: vec!["claude".to_string()],
        }
    }
}

impl LaunchSupport for Driver {
    fn agent_launch(
        &self,
        _agent: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> AgentLaunch {
        AgentLaunch {
            argv: Some(self.argv.clone()),
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
        _task: &str,
    ) -> Map<String, Value> {
        json!({
            "ok": true,
            "task_sent": true,
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
fn hc_get_ok(parent: &str) -> Reply {
    Reply::json(&json!({
        "address": "t12",
        "name": "dev1-opencode",
        "parent": parent,
        "labels": {"agent": "opencode", "ez.role": "developer"},
        "state": "live",
    }))
}

/// La respuesta de `hc get` de un padre que está en un espacio de Herdr.
fn hc_get_with_space(parent: &str, space: &str) -> Reply {
    Reply::json(&json!({
        "address": parent,
        "name": "padre",
        "parent": "t1",
        "labels": {"agent": "claude", "herdr.space": space},
        "state": "live",
    }))
}

/// La lista de Herdr con el subagente en una pestaña del espacio dado.
fn app_ls_with_tab(space: &str, tab: &str, terminal: &str) -> Reply {
    Reply::json(&json!({
        "spaces": [{
            "id": space,
            "tabs": [{
                "id": tab,
                "terminals": [{"terminal": terminal, "pane": "p1"}],
            }],
        }],
    }))
}

fn herdr_de(fake: &Arc<Fake>) -> Herdr {
    let runtime: Arc<dyn HerdrRuntime> = fake.clone();
    Herdr::with_runtime(config(), runtime)
}

/// El valor que sigue a una bandera del argv.
fn arg_after(argv: &[String], flag: &str) -> Option<String> {
    argv.iter()
        .position(|part| part == flag)
        .and_then(|index| argv.get(index + 1))
        .cloned()
}

// --------------------------------------------------------------------- nombre

#[test]
fn hc_slug_deja_un_nombre_que_hc_acepta() {
    assert_eq!(hc_slug("claude · voz"), "claude-voz");
    assert_eq!(hc_slug("Developer"), "developer");
    assert_eq!(hc_slug("Ángel Núñez"), "angel-nunez");
    assert_eq!(hc_slug(""), "agente");
    assert_eq!(hc_slug("..."), "agente");
    assert_eq!(hc_slug("a--b"), "a-b");
    assert_eq!(hc_slug("-abc-"), "abc");
    assert_eq!(hc_slug("_abc"), "abc");
    assert_eq!(hc_slug("dev1-opencode"), "dev1-opencode");
}

#[test]
fn hc_slug_escapa_los_nombres_con_forma_de_direccion() {
    assert_eq!(hc_slug("t7"), "a-t7");
    assert_eq!(hc_slug("t9n"), "a-t9n");
    assert_eq!(hc_slug("T7"), "a-t7");
    assert_eq!(hc_slug("terminal"), "terminal");
}

#[test]
fn hc_slug_recorta_a_32_caracteres() {
    let slug = hc_slug(&"x".repeat(40));
    assert_eq!(slug.len(), 32);
    assert!(slug
        .chars()
        .all(|character| character.is_ascii_alphanumeric()));
}

#[test]
fn placement_se_lee_y_se_escribe() {
    assert_eq!(Placement::parse("tree"), Some(Placement::Tree));
    assert_eq!(Placement::parse("TAB"), Some(Placement::Tab));
    assert_eq!(Placement::parse(" right "), Some(Placement::Right));
    assert_eq!(Placement::parse("down"), Some(Placement::Down));
    assert_eq!(Placement::parse("panel"), None);
    assert_eq!(Placement::default(), Placement::Tree);
    assert_eq!(Placement::Tree.as_str(), "tree");
    assert_eq!(Placement::Tab.as_str(), "tab");
    assert_eq!(Placement::Right.as_str(), "right");
    assert_eq!(Placement::Down.as_str(), "down");
}
