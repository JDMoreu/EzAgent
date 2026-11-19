use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{json, Map, Value};

use super::*;
use crate::herdr::HerdrError;
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

fn mcode_models() -> Vec<String> {
    vec!["minimax/MiniMax-M3.1-Flash".to_string()]
}

fn no_codex() -> Map<String, Value> {
    Map::new()
}

fn codex_catalog() -> Map<String, Value> {
    json!({"gpt-5": ["low", "high"]})
        .as_object()
        .cloned()
        .expect("objeto")
}

fn no_default_model() -> String {
    String::new()
}

fn kimi_default_model() -> String {
    "opencode-go/kimi-k3".to_string()
}

fn no_state(_model: &str, _effort: &str) -> Map<String, Value> {
    Map::new()
}

fn no_effort(_effort: &str) -> bool {
    false
}

fn installed(_program: &str) -> bool {
    true
}

fn absent(_program: &str) -> bool {
    false
}

fn driver_with(
    which: fn(&str) -> bool,
    mcode: fn() -> Vec<String>,
    codex: fn() -> Map<String, Value>,
    default_model: fn() -> String,
    state: fn(&str, &str) -> Map<String, Value>,
    effort: fn(&str) -> bool,
) -> AgentDriver<NoHost> {
    driver_configured(
        VoiceConfig::new(),
        which,
        mcode,
        codex,
        default_model,
        state,
        effort,
    )
}

fn driver_configured(
    cfg: VoiceConfig,
    which: fn(&str) -> bool,
    mcode: fn() -> Vec<String>,
    codex: fn() -> Map<String, Value>,
    default_model: fn() -> String,
    state: fn(&str, &str) -> Map<String, Value>,
    effort: fn(&str) -> bool,
) -> AgentDriver<NoHost> {
    let hooks = LaunchHooks {
        discover_mcode_models: mcode,
        discover_codex_models: codex,
        opencode_default_model: default_model,
        prepare_opencode_state: state,
        set_mcode_effort: effort,
        which: Arc::new(which),
    };
    AgentDriver::with_hooks(NoHost, cfg, hooks)
}

fn driver_which(
    cfg: VoiceConfig,
    which: impl Fn(&str) -> bool + Send + Sync + 'static,
) -> AgentDriver<NoHost> {
    let hooks = LaunchHooks {
        discover_mcode_models: mcode_models,
        discover_codex_models: codex_catalog,
        opencode_default_model: no_default_model,
        prepare_opencode_state: no_state,
        set_mcode_effort: no_effort,
        which: Arc::new(which),
    };
    AgentDriver::with_hooks(NoHost, cfg, hooks)
}

fn full_driver() -> AgentDriver<NoHost> {
    driver_with(
        installed,
        mcode_models,
        codex_catalog,
        no_default_model,
        no_state,
        no_effort,
    )
}

fn record_state(model: &str, effort: &str) -> Map<String, Value> {
    PREPARED
        .lock()
        .expect("estado")
        .push((model.to_string(), effort.to_string()));
    Map::new()
}

fn record_effort(effort: &str) -> bool {
    EFFORTS.lock().expect("esfuerzos").push(effort.to_string());
    true
}

#[test]
fn los_seis_tipos_del_catalogo_validan() {
    let driver = full_driver();
    let cases = [
        ("claude", Some("sonnet"), Some("low"), "Claude Code"),
        (
            "opencode",
            Some("deepseek-v4.1-flash"),
            Some("high"),
            "OpenCode",
        ),
        ("qwen", Some("qwen3-coder-plus"), None, "Qwen Code"),
        ("codex", Some("gpt-5"), Some("high"), "Codex"),
        (
            "mcode",
            Some("minimax-m3.1-flash"),
            Some("high"),
            "MiniMax Code",
        ),
        ("pi", Some("prov/modelo"), Some("medium"), "Pi"),
    ];
    for (agent, model, effort, label) in cases {
        let choice = driver
            .validate_choice(agent, model, effort)
            .unwrap_or_else(|error| panic!("{agent}: {error}"));
        assert_eq!(choice.key, agent);
        assert_eq!(choice.label, label);
        assert_eq!(choice.model.as_deref(), model);
        assert_eq!(choice.effort.as_deref(), effort);
    }
    let pi = driver
        .validate_choice("pi", Some("prov/modelo"), Some("medium"))
        .expect("pi");
    assert_eq!(
        pi.warnings,
        vec!["El catálogo de Pi no lista modelos; uso «prov/modelo» sin verificar."]
    );
}

#[test]
fn opencode_acepta_el_prefijo_del_modelo() {
    let choice = full_driver()
        .validate_choice(
            "opencode",
            Some("opencode-go/deepseek-v4.1-flash"),
            Some("max"),
        )
        .expect("opencode");
    assert_eq!(
        choice.model.as_deref(),
        Some("opencode-go/deepseek-v4.1-flash")
    );
    assert!(choice.warnings.is_empty());
}

#[test]
fn modelos_fuera_del_catalogo_se_rechazan() {
    let driver = full_driver();
    let cases = [
        (
            "claude",
            Some("gpt-5"),
            None,
            "Claude Code no tiene el modelo «gpt-5». Modelos: fable, opus, sonnet, haiku.",
        ),
        (
            "opencode",
            Some("deepseek-v4-pro"),
            Some("low"),
            "El modelo deepseek-v4-pro no tiene el esfuerzo 'low'. Valores válidos: high, max.",
        ),
        (
            "mcode",
            Some("nope"),
            None,
            "MiniMax Code no tiene el modelo «nope». Modelos: minimax/MiniMax-M3.1-Flash.",
        ),
        (
            "codex",
            Some("gpt-4"),
            None,
            "Codex no tiene el modelo 'gpt-4'. Modelos: gpt-5.",
        ),
    ];
    for (agent, model, effort, expected) in cases {
        assert_eq!(
            driver.validate_choice(agent, model, effort).unwrap_err(),
            expected
        );
    }
}

#[test]
fn esfuerzos_fuera_del_catalogo_se_rechazan() {
    let driver = full_driver();
    assert_eq!(
        driver
            .validate_choice("claude", Some("sonnet"), Some("ultra"))
            .unwrap_err(),
        "El esfuerzo 'ultra' no existe en claude. Valores válidos: low, medium, high, xhigh, max."
    );
    assert_eq!(
        driver
            .validate_choice("codex", Some("gpt-5"), Some("ultra"))
            .unwrap_err(),
        "El esfuerzo 'ultra' no existe en Codex para gpt-5. Valores válidos: low, high."
    );
    assert_eq!(
        driver
            .validate_choice("pi", Some("prov/modelo"), Some("ultra"))
            .unwrap_err(),
        "El esfuerzo 'ultra' no existe en pi. Valores válidos: off, minimal, low, medium, high, xhigh, max."
    );
}

#[test]
fn qwen_no_tiene_niveles_de_esfuerzo() {
    assert_eq!(
        full_driver()
            .validate_choice("qwen", Some("qwen3-coder-plus"), Some("medium"))
            .unwrap_err(),
        "Qwen Code no tiene niveles de esfuerzo."
    );
}

#[test]
fn alias_y_tipo_desconocido() {
    let driver = full_driver();
    assert_eq!(
        driver
            .validate_choice("open code", Some("deepseek-v4.1-flash"), None)
            .expect("alias")
            .key,
        "opencode"
    );
    assert_eq!(
        driver
            .validate_choice("cloud code", Some("sonnet"), None)
            .expect("alias")
            .key,
        "claude"
    );
    assert_eq!(
        driver.validate_choice("nada", None, None).unwrap_err(),
        "No conozco el agente «nada». Agentes del catálogo: claude, codex, mcode, minimax, opencode, pi, qwen."
    );
}

#[test]
fn programa_no_instalado() {
    let driver = driver_with(
        absent,
        mcode_models,
        codex_catalog,
        no_default_model,
        no_state,
        no_effort,
    );
    assert_eq!(
        driver.validate_choice("claude", None, None).unwrap_err(),
        "«claude» está en el catálogo pero no encuentro su programa (claude)."
    );
}

#[test]
fn catalogo_sin_descubrimiento_avisa() {
    let driver = driver_with(
        installed,
        no_models,
        no_codex,
        no_default_model,
        no_state,
        no_effort,
    );
    let mcode = driver
        .validate_choice("mcode", Some("minimax-m3.1-flash"), None)
        .expect("mcode");
    assert_eq!(
        mcode.warnings,
        vec!["No pude leer los modelos de MiniMax Code; uso «minimax-m3.1-flash» sin verificar."]
    );
    let codex = driver
        .validate_choice("codex", Some("gpt-5"), Some("high"))
        .expect("codex");
    assert_eq!(
        codex.warnings,
        vec!["No pude leer los modelos de Codex; uso «gpt-5» sin verificar."]
    );
}

#[test]
fn validar_no_escribe_configuracion() {
    PREPARED.lock().expect("estado").clear();
    EFFORTS.lock().expect("esfuerzos").clear();
    let driver = driver_with(
        installed,
        mcode_models,
        codex_catalog,
        no_default_model,
        record_state,
        record_effort,
    );
    driver
        .validate_choice("opencode", Some("deepseek-v4.1-flash"), Some("high"))
        .expect("opencode");
    driver
        .validate_choice("mcode", Some("minimax-m3.1-flash"), Some("high"))
        .expect("mcode");
    assert!(PREPARED.lock().expect("estado").is_empty());
    assert!(EFFORTS.lock().expect("esfuerzos").is_empty());
}

#[test]
fn esfuerzo_sin_modelo_usa_los_niveles_del_modelo_por_omision() {
    let driver = full_driver();
    assert!(driver
        .validate_choice("opencode", None, Some("high"))
        .is_ok());
    assert_eq!(
        driver
            .validate_choice("opencode", None, Some("ultra"))
            .unwrap_err(),
        "OpenCode no tiene el esfuerzo «ultra». Valores válidos: low, high, max, medium, xhigh, none."
    );

    let driver = driver_with(
        installed,
        mcode_models,
        codex_catalog,
        kimi_default_model,
        no_state,
        no_effort,
    );
    assert!(driver
        .validate_choice("opencode", None, Some("max"))
        .is_ok());
    assert_eq!(
        driver
            .validate_choice("opencode", None, Some("high"))
            .unwrap_err(),
        "El modelo kimi-k3 no tiene el esfuerzo 'high'. Valores válidos: max."
    );
}

#[test]
fn programa_se_busca_igual_que_al_lanzar() {
    let path = "C:/tmp/fake-ws-agent.exe";
    let mut cfg = VoiceConfig::new();
    cfg.set("agents", json!({"claude": [path]}));
    let driver = driver_which(cfg.clone(), move |program| program == path);
    assert_eq!(
        driver
            .validate_choice("claude", None, None)
            .expect("claude")
            .key,
        "claude"
    );

    let driver = driver_which(cfg, |program| program == "claude");
    assert_eq!(
        driver.validate_choice("claude", None, None).unwrap_err(),
        format!("«claude» está en el catálogo pero no encuentro su programa ({path}).")
    );
}
