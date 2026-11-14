use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use ez_core::pyvalue::text_or_empty;
use serde_json::{json, Map, Value};

use super::*;
use crate::herdr::{Herdr, HerdrError, LaunchSupport};
use crate::session::SessionHost;

static PREPARED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static EFFORTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone)]
struct NoHost;

impl SessionHost for NoHost {
    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, HerdrError> {
        Ok(Map::new())
    }

    fn send_agent(
        &self,
        _address: &str,
        _text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        Ok(Map::new())
    }

    fn keys_agent(
        &self,
        _address: &str,
        _keys: &[String],
    ) -> Result<Map<String, Value>, HerdrError> {
        Ok(Map::new())
    }

    fn read_agent(&self, _address: &str, _lines: Option<i64>) -> Result<String, HerdrError> {
        Ok(String::new())
    }
}

fn no_models() -> Vec<String> {
    Vec::new()
}

fn no_codex() -> Map<String, Value> {
    Map::new()
}

fn no_default_model() -> String {
    String::new()
}

fn no_state(_model: &str, _effort: &str) -> Map<String, Value> {
    Map::new()
}

fn no_effort(_effort: &str) -> bool {
    false
}

fn not_installed(_program: &str) -> bool {
    false
}

fn silent_hooks() -> LaunchHooks {
    LaunchHooks {
        discover_mcode_models: no_models,
        discover_codex_models: no_codex,
        opencode_default_model: no_default_model,
        prepare_opencode_state: no_state,
        set_mcode_effort: no_effort,
        which: not_installed,
    }
}

fn mcode_catalog() -> Vec<String> {
    vec!["minimax/MiniMax-M3.1-Flash".to_string()]
}

fn codex_catalog() -> Map<String, Value> {
    json!({"gpt-5": ["low", "high"]})
        .as_object()
        .cloned()
        .expect("objeto")
}

fn default_opencode_model() -> String {
    "prov/modelo".to_string()
}

fn record_state(model: &str, effort: &str) -> Map<String, Value> {
    PREPARED
        .lock()
        .expect("estado")
        .push((model.to_string(), effort.to_string()));
    json!({"XDG_STATE_HOME": "/estado"})
        .as_object()
        .cloned()
        .expect("objeto")
}

fn record_effort(effort: &str) -> bool {
    EFFORTS.lock().expect("esfuerzos").push(effort.to_string());
    true
}

fn driver_with(cfg: VoiceConfig, hooks: LaunchHooks) -> AgentDriver<NoHost> {
    AgentDriver::with_hooks(NoHost, cfg, hooks)
}

fn default_driver() -> AgentDriver<NoHost> {
    driver_with(VoiceConfig::new(), silent_hooks())
}

fn config_with(agents: Value, profiles: Value) -> VoiceConfig {
    let data = json!({"agents": agents, "agent_profiles": profiles})
        .as_object()
        .cloned()
        .expect("objeto");
    VoiceConfig::from_json(&data)
}

fn custom(config: Value) -> VoiceConfig {
    config_with(json!({"custom": ["custom"]}), json!({"custom": config}))
}

#[test]
fn agente_desconocido_lista_los_disponibles() {
    let launch = default_driver().agent_launch("nada", None, None);
    assert_eq!(
        launch.error,
        "Agente desconocido: 'nada'. Disponibles: claude, codex, mcode, minimax, opencode, pi, qwen."
    );
    assert_eq!(launch.argv, None);
}

#[test]
fn agente_sin_perfil_no_acepta_modelo() {
    let cfg = config_with(json!({"ghost": ["ghost"]}), json!({}));
    let launch = driver_with(cfg, silent_hooks()).agent_launch("ghost", Some("x"), None);
    assert_eq!(
        launch.error,
        "El agente 'ghost' no tiene perfil de modelo/esfuerzo. Configura agent_profiles en voice.json."
    );
}

#[test]
fn modelo_y_esfuerzo_invalidos() {
    let driver = default_driver();
    assert_eq!(
        driver.agent_launch("claude", Some("-x"), None).error,
        "El modelo '-x' no es válido."
    );
    assert_eq!(
        driver.agent_launch("claude", None, Some("turbo")).error,
        "El esfuerzo 'turbo' no existe en claude. Valores válidos: low, medium, high, xhigh, max."
    );
}

#[test]
fn claude_lleva_modelo_y_esfuerzo_en_flags() {
    let launch = default_driver().agent_launch("claude", Some("sonnet"), Some("high"));
    assert_eq!(
        launch.argv,
        Some(
            ["claude", "--model", "sonnet", "--effort", "high"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
    assert_eq!(launch.error, "");
    assert!(launch.env.is_empty());
    assert!(launch.pending.is_empty());
    assert!(launch.warnings.is_empty());
}

#[test]
fn usa_los_valores_por_defecto_del_perfil() {
    let cfg = custom(json!({
        "default_model": "sonnet",
        "default_effort": "high",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
    }));
    let launch = driver_with(cfg, silent_hooks()).agent_launch("custom", None, None);
    assert_eq!(
        launch.argv,
        Some(
            ["custom", "--model", "sonnet", "--effort", "high"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
}

#[test]
fn entorno_con_prefijo_y_variante() {
    let cfg = custom(json!({
        "model_prefix": "p/",
        "env": {"OPENCODE_CONFIG_CONTENT": "{\"model\":\"{model_ref}{variant}\"}"},
        "env_args": ["--standalone"],
        "model_args": [],
    }));
    let launch = driver_with(cfg, silent_hooks()).agent_launch("custom", Some("m"), Some("high"));
    assert_eq!(
        launch.argv,
        Some(
            ["custom", "--standalone"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
    assert_eq!(
        launch.env["OPENCODE_CONFIG_CONTENT"],
        "{\"model\":\"p/m#high\"}"
    );
    assert_eq!(launch.env.len(), 1);
    assert!(launch.pending.is_empty());
}

#[test]
fn opencode_usa_el_modelo_por_defecto_para_su_estado() {
    PREPARED.lock().expect("estado").clear();
    let hooks = LaunchHooks {
        opencode_default_model: default_opencode_model,
        prepare_opencode_state: record_state,
        ..silent_hooks()
    };
    let driver = driver_with(VoiceConfig::new(), hooks);
    let launch = driver.agent_launch("opencode", None, Some("high"));
    assert_eq!(
        launch.argv,
        Some(
            ["opencode", "--standalone"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
    assert_eq!(
        launch.env["OPENCODE_CONFIG_CONTENT"],
        "{\"model\":\"prov/modelo#high\"}"
    );
    assert_eq!(launch.env["XDG_STATE_HOME"], "/estado");
    assert_eq!(
        PREPARED.lock().expect("estado").clone(),
        vec![("prov/modelo".to_string(), "high".to_string())]
    );
}

#[test]
fn opencode_sin_esfuerzo_no_pega_variante() {
    let driver = driver_with(VoiceConfig::new(), silent_hooks());
    let launch = driver.agent_launch("opencode", Some("deepseek-v4.1-flash"), None);
    assert_eq!(
        launch.env["OPENCODE_CONFIG_CONTENT"],
        "{\"model\":\"opencode-go/deepseek-v4.1-flash\"}"
    );
}

#[test]
fn mcode_resuelve_el_modelo_contra_su_catalogo() {
    EFFORTS.lock().expect("esfuerzos").clear();
    let hooks = LaunchHooks {
        discover_mcode_models: mcode_catalog,
        set_mcode_effort: record_effort,
        ..silent_hooks()
    };
    let driver = driver_with(VoiceConfig::new(), hooks);
    let launch = driver.agent_launch("mcode", Some("minimax-m3.1-flash"), Some("high"));
    assert_eq!(
        launch.argv,
        Some(
            ["mcode", "--model", "minimax/MiniMax-M3.1-Flash"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
    assert!(launch.warnings.is_empty());
    assert!(launch.pending.is_empty());
    assert_eq!(
        EFFORTS.lock().expect("esfuerzos").clone(),
        vec!["high".to_string()]
    );
}

#[test]
fn mcode_rechaza_modelos_fuera_del_catalogo() {
    let hooks = LaunchHooks {
        discover_mcode_models: mcode_catalog,
        ..silent_hooks()
    };
    let error = driver_with(VoiceConfig::new(), hooks)
        .agent_launch("mcode", Some("nope"), None)
        .error;
    assert_eq!(
        error,
        "mcode no tiene el modelo 'nope'. Modelos: minimax/MiniMax-M3.1-Flash."
    );
}

#[test]
fn mcode_avisa_si_no_pudo_fijar_el_esfuerzo() {
    let launch = default_driver().agent_launch("mcode", None, Some("high"));
    assert_eq!(launch.argv, Some(vec!["mcode".to_string()]));
    assert_eq!(
        launch.warnings,
        vec![
            "mcode no permite fijar el esfuerzo desde aquí; se abrió con su esfuerzo por defecto."
                .to_string()
        ]
    );
}

#[test]
fn codex_resuelve_modelo_y_esfuerzo() {
    let hooks = LaunchHooks {
        discover_codex_models: codex_catalog,
        ..silent_hooks()
    };
    let driver = driver_with(VoiceConfig::new(), hooks);
    let launch = driver.agent_launch("codex", Some("GPT-5"), Some("HIGH"));
    assert_eq!(
        launch.argv,
        Some(
            [
                "codex",
                "--model",
                "gpt-5",
                "-c",
                "model_reasoning_effort=HIGH"
            ]
            .iter()
            .map(|part| part.to_string())
            .collect()
        )
    );
    assert!(launch.pending.is_empty());
}

#[test]
fn codex_rechaza_modelo_y_esfuerzo() {
    let hooks = LaunchHooks {
        discover_codex_models: codex_catalog,
        ..silent_hooks()
    };
    let driver = driver_with(VoiceConfig::new(), hooks);
    assert_eq!(
        driver.agent_launch("codex", Some("gpt-4"), None).error,
        "Codex no tiene el modelo 'gpt-4'. Modelos: gpt-5."
    );
    assert_eq!(
        driver
            .agent_launch("codex", Some("gpt-5"), Some("ultra"))
            .error,
        "El esfuerzo 'ultra' no existe en Codex para gpt-5. Valores válidos: low, high."
    );
    assert_eq!(
        driver.agent_launch("codex", None, Some("ultra")).error,
        "El esfuerzo 'ultra' no existe en Codex. Valores válidos: low, high."
    );
}

#[test]
fn codex_sin_modelo_pega_solo_el_esfuerzo() {
    let hooks = LaunchHooks {
        discover_codex_models: codex_catalog,
        ..silent_hooks()
    };
    let launch = driver_with(VoiceConfig::new(), hooks).agent_launch("codex", None, Some("high"));
    assert_eq!(
        launch.argv,
        Some(
            ["codex", "-c", "model_reasoning_effort=high"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
}

#[test]
fn opencode_valida_el_esfuerzo_del_modelo() {
    let error = default_driver()
        .agent_launch("opencode", Some("glm-5.3"), Some("xhigh"))
        .error;
    assert_eq!(
        error,
        "El modelo glm-5.3 no tiene el esfuerzo 'xhigh'. Valores válidos: low, high, max."
    );
    let launch = default_driver().agent_launch("opencode", Some("glm-5.3"), Some("max"));
    assert_eq!(
        launch.env["OPENCODE_CONFIG_CONTENT"],
        "{\"model\":\"opencode-go/glm-5.3#max\"}"
    );
}

#[test]
fn lo_que_no_entra_en_flags_queda_pendiente() {
    let cfg = custom(json!({
        "model_args": [],
        "session_commands": {"model": [{"send": "/model {model}"}]},
    }));
    let driver = driver_with(cfg, silent_hooks());
    let launch = driver.agent_launch("custom", Some("sonnet"), None);
    assert_eq!(launch.argv, Some(vec!["custom".to_string()]));
    assert_eq!(launch.pending["model"], "sonnet");
    assert!(launch.warnings.is_empty());
    assert_eq!(
        driver.agent_argv("custom", Some("sonnet"), None),
        Some(vec!["custom".to_string()])
    );
}

#[test]
fn avisa_si_el_agente_no_permite_fijar_el_valor() {
    let cfg = custom(json!({"model_args": []}));
    let launch = driver_with(cfg, silent_hooks()).agent_launch("custom", Some("sonnet"), None);
    assert!(launch.pending.is_empty());
    assert_eq!(
        launch.warnings,
        vec![
            "custom no permite fijar el modelo desde aquí; se abrió con su modelo por defecto."
                .to_string()
        ]
    );
}

#[test]
fn swarm_args_salen_del_perfil() {
    let driver = default_driver();
    assert_eq!(
        driver.swarm_args("claude"),
        vec!["--allowedTools".to_string(), "Bash(ezs *)".to_string()]
    );
    assert_eq!(driver.swarm_args("codex"), vec!["--yolo".to_string()]);
    assert!(driver.swarm_args("nada").is_empty());
}

#[test]
fn catalogo_ordenado_con_flags() {
    let catalog = default_driver().agents_catalog();
    let keys: Vec<String> = catalog
        .iter()
        .map(|entry| text_or_empty(entry.get("agent")))
        .collect();
    assert_eq!(
        keys,
        vec!["claude", "codex", "mcode", "minimax", "opencode", "pi", "qwen"]
    );
    let claude = &catalog[0];
    assert_eq!(claude["label"], "Claude Code");
    assert_eq!(
        claude["models"],
        json!(["fable", "opus", "sonnet", "haiku"])
    );
    assert_eq!(claude["settable"], json!(["model", "effort"]));
    assert_eq!(claude["installed"], false);
    assert_eq!(claude["aliases"].as_array().expect("lista").len(), 4);
    let mcode = &catalog[2];
    assert_eq!(mcode["models"], json!([]));
    assert_eq!(mcode["settable"], json!(["model"]));
    assert_eq!(catalog[4]["settable"], json!(["model", "effort"]));
}

#[test]
fn catalogo_en_texto() {
    let text = default_driver().catalog_text();
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("# Agentes disponibles (datos del catálogo)")
    );
    let claude = lines.next().expect("claude");
    assert!(claude.starts_with("- claude (Claude Code); suena también «claude code», «cloud code», «clod code», «clode»; modelos: fable, opus, sonnet, haiku; esfuerzo: low, medium, high, xhigh, max; se puede fijar: modelo, esfuerzo; no se encuentra en el PATH; "));
    assert_eq!(text.lines().count(), 8);
}

#[test]
fn resuelve_aliarios_del_perfil() {
    let driver = default_driver();
    assert_eq!(
        driver.resolve_agent("claude code"),
        Some("claude".to_string())
    );
    assert_eq!(
        driver.resolve_agent("cloud code"),
        Some("claude".to_string())
    );
    assert_eq!(driver.resolve_agent("nada"), None);
}

#[test]
fn el_driver_es_launch_support_para_el_lanzador() {
    fn assert_support<T: LaunchSupport + Clone + Send + 'static>() {}
    assert_support::<AgentDriver<Arc<Herdr>>>();

    let herdr = Arc::new(Herdr::new(VoiceConfig::new()));
    let driver = AgentDriver::new(herdr, VoiceConfig::new());
    let launch = LaunchSupport::agent_launch(&driver, "claude", Some("sonnet"), None);
    assert_eq!(
        launch.argv,
        Some(
            ["claude", "--model", "sonnet"]
                .iter()
                .map(|part| part.to_string())
                .collect()
        )
    );
}
