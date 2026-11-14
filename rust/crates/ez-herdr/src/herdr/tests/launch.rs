use super::super::actions::{launch_base, launch_outcome, RunAgent};
use super::super::handoff::{hex_id, is_handoff_file, save_context_in};
use super::super::*;
use ez_core::config::VoiceConfig;
use serde_json::json;

#[test]
fn resolve_agent_usa_aliarios_y_nombres() {
    let herdr = Herdr::new(VoiceConfig::new());
    assert_eq!(
        herdr.resolve_agent("claude code"),
        Some("claude".to_string())
    );
    assert_eq!(
        herdr.resolve_agent("OPENCODE"),
        Some("opencode".to_string())
    );
    assert_eq!(herdr.resolve_agent("nada"), None);
    assert_eq!(herdr.resolve_agent(""), None);
}

#[derive(Clone)]
struct NoLaunch;

impl LaunchSupport for NoLaunch {
    fn agent_launch(
        &self,
        _agent: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> AgentLaunch {
        AgentLaunch::default()
    }

    fn wait_shell_prompt(&self, _address: &str) -> bool {
        false
    }

    fn finish_launch(
        &self,
        _address: &str,
        _key: &str,
        _launch: &AgentLaunch,
        _task: &str,
    ) -> Map<String, Value> {
        Map::new()
    }
}

#[derive(Clone)]
struct Failing;

impl LaunchSupport for Failing {
    fn agent_launch(
        &self,
        _agent: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> AgentLaunch {
        AgentLaunch {
            error: "Falta el perfil.".to_string(),
            ..AgentLaunch::default()
        }
    }

    fn wait_shell_prompt(&self, _address: &str) -> bool {
        false
    }

    fn finish_launch(
        &self,
        _address: &str,
        _key: &str,
        _launch: &AgentLaunch,
        _task: &str,
    ) -> Map<String, Value> {
        Map::new()
    }
}

#[derive(Clone)]
struct Fin(bool);

impl LaunchSupport for Fin {
    fn agent_launch(
        &self,
        _agent: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> AgentLaunch {
        AgentLaunch::default()
    }

    fn wait_shell_prompt(&self, _address: &str) -> bool {
        false
    }

    fn finish_launch(
        &self,
        _address: &str,
        _key: &str,
        _launch: &AgentLaunch,
        _task: &str,
    ) -> Map<String, Value> {
        if self.0 {
            json!({"ok": true, "applied": [], "warnings": [], "task_sent": true})
                .as_object()
                .cloned()
                .expect("objeto")
        } else {
            json!({"ok": false, "error": "no arrancó"})
                .as_object()
                .cloned()
                .expect("objeto")
        }
    }
}

#[test]
fn error_de_agente_desconocido_lista_los_disponibles() {
    let herdr = Herdr::new(VoiceConfig::new());
    let result = herdr
        .run_agent(
            NoLaunch,
            RunAgent {
                agent: "desconocido",
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "Agente desconocido: 'desconocido'.");
    let available = result["available"].as_array().expect("lista");
    assert!(available.iter().any(|item| item == "claude"));
    assert!(available.iter().any(|item| item == "opencode"));
}

#[test]
fn el_error_del_lanzamiento_se_propaga() {
    let herdr = Herdr::new(VoiceConfig::new());
    let result = herdr
        .run_agent(
            Failing,
            RunAgent {
                agent: "claude",
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "Falta el perfil.");
}

fn base_de_prueba() -> Map<String, Value> {
    launch_base(
        "t1",
        "claude",
        Some("sonnet"),
        None,
        "C:\\work",
        &["claude".to_string()],
        Map::new(),
    )
}

#[test]
fn el_fin_correcto_mezcla_base_y_resultado() {
    let base = base_de_prueba();
    let launch = AgentLaunch::default();
    let result = launch_outcome(&Fin(true), "t1", "claude", &launch, "tarea", &base);
    assert_eq!(result["ok"], true);
    assert_eq!(result["address"], "t1");
    assert_eq!(result["agent"], "claude");
    assert_eq!(result["model"], "sonnet");
    assert_eq!(result["effort"], "");
    assert_eq!(result["task_sent"], true);
}

#[test]
fn el_fin_fallido_agrega_direccion_y_agente() {
    let base = base_de_prueba();
    let launch = AgentLaunch::default();
    let failed = launch_outcome(&Fin(false), "t1", "claude", &launch, "tarea", &base);
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["error"], "no arrancó");
    assert_eq!(failed["address"], "t1");
    assert_eq!(failed["agent"], "claude");
    assert!(!failed.contains_key("model"));
}

#[test]
fn save_context_escribe_y_conserva_lo_reciente() {
    let folder = std::env::temp_dir().join(format!(
        "ez-herdr-handoff-{}-{}",
        std::process::id(),
        hex_id()
    ));
    std::fs::create_dir_all(&folder).expect("carpeta");
    let old = folder.join("mensaje-viejo.md");
    std::fs::write(&old, b"viejo").expect("archivo");
    let other = folder.join("notas.md");
    std::fs::write(&other, b"otras").expect("archivo");
    let path = save_context_in(&folder, "contexto\nlargo").expect("guardado");
    assert!(path.ends_with(".md"));
    let text = std::fs::read_to_string(&path).expect("leer");
    assert_eq!(text, "contexto\r\nlargo");
    assert!(old.exists());
    assert!(other.exists());
    assert!(std::fs::read_dir(&folder).expect("dir").count() >= 3);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn nombres_de_handoff_reconocidos() {
    assert!(is_handoff_file("contexto-ab.md"));
    assert!(is_handoff_file("mensaje-ab.md"));
    assert!(!is_handoff_file("otro.md"));
    assert!(!is_handoff_file("contexto-ab.txt"));
    assert!(!is_handoff_file("contexto-.md.bak"));
}
