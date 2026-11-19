//! Pruebas de los subagentes: el nombre de `hc`, la ubicación y el
//! lanzamiento con padre.
//!
//! El doble de Herdr (`Fake`) registra cada argv y responde lo preparado; no
//! se ejecuta `hc` ni `herdr-app` reales.

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
    assert!(slug.chars().all(|character| character.is_ascii_alphanumeric()));
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

// ------------------------------------------------------------------ lanzamiento

#[test]
fn con_padre_el_lanzamiento_va_por_hc_run_con_parent() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let mut labels = Map::new();
    labels.insert("ez.role".to_string(), Value::from("developer"));
    labels.insert("ez.agent".to_string(), Value::from("opencode"));
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                parent: Some("t6h"),
                hc_name: Some("dev1-opencode"),
                labels: Some(labels),
                show: Placement::Tree,
                outlive_parent: true,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert_eq!(
        argv,
        vec![
            "hc".to_string(),
            "run".to_string(),
            "--json".to_string(),
            "--bg".to_string(),
            "--parent".to_string(),
            "t6h".to_string(),
            "--name".to_string(),
            "dev1-opencode".to_string(),
            "--cwd".to_string(),
            "C:\\ws".to_string(),
            "--no-notify".to_string(),
            "--outlive-parent".to_string(),
            "--label".to_string(),
            "agent=claude".to_string(),
            "--label".to_string(),
            "ez.agent=opencode".to_string(),
            "--label".to_string(),
            "ez.role=developer".to_string(),
            "--".to_string(),
            "claude".to_string(),
        ]
    );
    assert!(!argv.contains(&"--no-parent".to_string()));
    assert!(fake.app_calls("tab.new").is_empty());
}

#[test]
fn con_padre_el_entorno_usa_la_misma_envoltura() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let mut env = Map::new();
    env.insert("OPENCODE_CONFIG_CONTENT".to_string(), Value::from("{}"));
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                parent: Some("t6h"),
                env: Some(env),
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    let tail: Vec<String> = argv[argv.len() - 4..].to_vec();
    assert_eq!(tail[0], "powershell.exe");
    assert_eq!(tail[1], "-NoProfile");
    assert_eq!(tail[2], "-EncodedCommand");
    assert!(!tail[3].is_empty());
}

#[test]
fn sin_padre_el_lanzamiento_sigue_siendo_el_de_hoy() {
    let fake = Fake::new().route("hc:run", vec![hc_run_ok()]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                background: true,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert!(argv.contains(&"--no-parent".to_string()));
    assert!(!argv.contains(&"--parent".to_string()));
    // DECISION: `hc` rechaza la etiqueta legible; se le pasa su versión válida.
    assert_eq!(
        arg_after(&argv, "--name").as_deref(),
        Some("claude-voz"),
        "{argv:?}"
    );
}

// ------------------------------------------------------------------- ubicación

#[test]
fn en_el_arbol_no_se_llama_a_herdr_app() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tree");
    assert!(fake.app_calls("terminal.keep").is_empty());
    assert!(fake.app_calls("pane.focus").is_empty());
}

#[test]
fn en_pestana_se_llama_a_terminal_keep() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "tab": "t12"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    assert_eq!(result["tab"], "t12");
    let keep = fake
        .app_calls("terminal.keep")
        .first()
        .cloned()
        .expect("terminal.keep");
    assert_eq!(
        keep,
        vec![
            "herdr-app".to_string(),
            "do".to_string(),
            "terminal.keep".to_string(),
            "--terminal".to_string(),
            "t12".to_string(),
            "--place".to_string(),
            "tab".to_string(),
        ]
    );
}

#[test]
fn terminal_keep_reintenta_mientras_herdr_no_ve_la_terminal() {
    let fake = Fake::new()
        .ticking(5.0)
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![
                Reply::plain("no running terminal \"t12\""),
                Reply::plain("no running terminal \"t12\""),
                Reply::json(&json!({"ok": true, "tab": "t12"})),
            ],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    assert_eq!(fake.app_calls("terminal.keep").len(), 3);
}

#[test]
fn en_panel_se_enfoca_el_padre_antes_de_colocar() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route("app:pane.focus", vec![Reply::json(&json!({"ok": true}))])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "pane": "p1"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Right,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "right");
    assert_eq!(result["pane"], "p1");
    let focus = fake
        .app_calls("pane.focus")
        .first()
        .cloned()
        .expect("pane.focus");
    assert_eq!(
        focus,
        vec![
            "herdr-app".to_string(),
            "do".to_string(),
            "pane.focus".to_string(),
            "--terminal".to_string(),
            "t6h".to_string(),
        ]
    );
    let keep = fake
        .app_calls("terminal.keep")
        .first()
        .cloned()
        .expect("terminal.keep");
    assert_eq!(keep[6], "right");
}

#[test]
fn sin_panel_del_padre_el_subagente_abre_en_pestana() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route("app:pane.focus", vec![Reply::error("no hay panel")])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "tab": "t12"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Down,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings.iter().any(|warning| warning
        == "El agente padre no tiene panel; abrí el subagente en una pestaña."));
}

#[test]
fn un_error_de_herdr_deja_el_subagente_en_el_arbol() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route("app:terminal.keep", vec![Reply::error("herdr-app no responde")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    assert_eq!(result["placed"], "tree");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings.iter().any(|warning| warning
        == "Herdr no pudo ubicar el subagente (herdr-app no responde); \
           quedó en el árbol bajo su agente."));
}

#[test]
fn el_resultado_informa_el_padre_el_nombre_y_las_etiquetas() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["parent"], "t6h");
    assert_eq!(result["name"], "dev1-opencode");
    assert_eq!(result["placed"], "tree");
    assert_eq!(result["labels"]["agent"], "opencode");
    assert_eq!(result["labels"]["ez.role"], "developer");
}

#[test]
fn un_padre_distinto_del_pedido_se_avisa() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t9z")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["parent"], "t9z");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings
        .iter()
        .any(|warning| warning == "El padre de t12 es t9z, no t6h."));
}

#[test]
fn un_nombre_ocupado_vuelve_como_error() {
    let fake = Fake::new().route(
        "hc:run",
        vec![Reply::json(&json!({
            "error": {
                "kind": "name_taken",
                "message": "a live terminal is already named \"dev1-opencode\"",
            }
        }))],
    );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                hc_name: Some("dev1-opencode"),
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], false);
    assert_eq!(result["name_taken"], true);
    assert_eq!(
        result["error"],
        "a live terminal is already named \"dev1-opencode\""
    );
}
