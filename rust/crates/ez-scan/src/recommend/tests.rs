//! Pruebas de la recomendación con informes y catálogos fabricados.

use ez_core::config::VoiceConfig;

use super::*;
use crate::usage::{AgentUsage, ModelUsage, Sources};

fn agent(key: &str, label: &str, state: UsageState) -> AgentUsage {
    AgentUsage {
        key: key.to_string(),
        label: label.to_string(),
        installed: true,
        window_hours: 5.0,
        used_percent: None,
        observed_window_hours: None,
        tokens: None,
        messages: None,
        cost_usd: None,
        models: Vec::new(),
        limit: None,
        resets_ms: None,
        reset_text: None,
        plan: None,
        priority: None,
        state,
        observed_ms: None,
        sources: Sources::default(),
        warnings: Vec::new(),
    }
}

fn model(name: &str, state: UsageState) -> ModelUsage {
    ModelUsage {
        model: name.to_string(),
        tokens: None,
        messages: None,
        cost_usd: None,
        state,
    }
}

fn report(agents: Vec<AgentUsage>) -> UsageReport {
    UsageReport {
        generated_ms: 0,
        agents,
        warnings: Vec::new(),
    }
}

fn request(role: &str) -> Request<'_> {
    Request {
        role,
        ..Request::default()
    }
}

fn choose_for(cfg: &VoiceConfig, report: &UsageReport, role: &str) -> Recommendation {
    choose(
        cfg,
        report,
        &Catalog::factory(),
        &request(role),
        &Pressure::default(),
    )
    .expect("recomendación")
}

#[test]
fn el_papel_fija_el_nivel_y_el_esfuerzo() {
    assert_eq!(required_level("planner", Size::M), Level::Alto);
    assert_eq!(required_level("plan_reviewer", Size::M), Level::Alto);
    assert_eq!(required_level("qa_validator", Size::M), Level::Alto);
    assert_eq!(required_level("qa", Size::M), Level::Medio);
    assert_eq!(required_level("otro", Size::M), Level::Medio);
    assert_eq!(required_level("developer", Size::M), Level::Medio);
    assert_eq!(required_level("developer", Size::L), Level::Alto);
    assert_eq!(desired_effort("planner"), "high");
    assert_eq!(desired_effort("developer"), "medium");
    assert_eq!(desired_effort("qa"), "medium");
}

#[test]
fn developer_elige_el_modelo_medio_y_el_esfuerzo_mas_cercano() {
    let cfg = VoiceConfig::new();
    let report = report(vec![
        agent("opencode", "OpenCode", UsageState::Libre),
        agent("claude", "Claude Code", UsageState::Libre),
    ]);
    let recommendation = choose_for(&cfg, &report, "developer");
    assert_eq!(recommendation.agent, "opencode");
    assert_eq!(recommendation.model, "opencode-go/deepseek-v4.1-flash");
    assert_eq!(recommendation.effort.as_deref(), Some("high"));
    assert!(
        recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("no tiene el esfuerzo medium")),
        "{:?}",
        recommendation.reasons
    );
}

#[test]
fn un_modelo_gratuito_agotado_no_descarta_el_de_plan() {
    let cfg = VoiceConfig::new();
    let catalog = Catalog {
        entries: vec![
            ModelEntry {
                agent: "opencode".into(),
                model: "glm-5.3".into(),
                level: Level::Medio,
                cost: Cost::Gratis,
                family: "zhipu".into(),
            },
            ModelEntry {
                agent: "opencode".into(),
                model: "deepseek-v4.1-flash".into(),
                level: Level::Medio,
                cost: Cost::Plan,
                family: "deepseek".into(),
            },
        ],
        warnings: Vec::new(),
    };
    let mut opencode = agent("opencode", "OpenCode", UsageState::Libre);
    opencode.models = vec![
        model("opencode/glm-5.3", UsageState::Agotado),
        model("opencode-go/deepseek-v4.1-flash", UsageState::Libre),
    ];
    let recommendation = choose(
        &cfg,
        &report(vec![opencode]),
        &catalog,
        &request("developer"),
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.model, "opencode-go/deepseek-v4.1-flash");
    assert_eq!(recommendation.cost, Cost::Plan);
}

#[test]
fn cerca_pierde_contra_libre_aunque_sea_mas_barato() {
    let cfg = VoiceConfig::new();
    let catalog = Catalog {
        entries: vec![
            ModelEntry {
                agent: "opencode".into(),
                model: "deepseek-v4.1-flash".into(),
                level: Level::Medio,
                cost: Cost::Gratis,
                family: "deepseek".into(),
            },
            ModelEntry {
                agent: "claude".into(),
                model: "sonnet".into(),
                level: Level::Medio,
                cost: Cost::Plan,
                family: "anthropic".into(),
            },
        ],
        warnings: Vec::new(),
    };
    let mut opencode = agent("opencode", "OpenCode", UsageState::Cerca);
    opencode.models = vec![model("opencode-go/deepseek-v4.1-flash", UsageState::Cerca)];
    let mut claude = agent("claude", "Claude Code", UsageState::Libre);
    claude.models = vec![model("claude/sonnet", UsageState::Libre)];
    let recommendation = choose(
        &cfg,
        &report(vec![opencode, claude]),
        &catalog,
        &request("developer"),
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.agent, "claude");
    assert_eq!(recommendation.model, "sonnet");
}

#[test]
fn reservar_va_al_final() {
    let cfg = VoiceConfig::new();
    let mut opencode = agent("opencode", "OpenCode", UsageState::Libre);
    opencode.priority = Some(Priority::Reservar);
    let mut claude = agent("claude", "Claude Code", UsageState::Libre);
    claude.priority = Some(Priority::Normal);
    let recommendation = choose_for(&cfg, &report(vec![opencode, claude]), "developer");
    assert_eq!(recommendation.agent, "claude");
    assert_eq!(recommendation.model, "sonnet");
}

#[test]
fn qa_validator_usa_una_familia_distinta_al_ultimo_developer() {
    let cfg = VoiceConfig::new();
    let report = report(vec![
        agent("opencode", "OpenCode", UsageState::Libre),
        agent("claude", "Claude Code", UsageState::Libre),
    ]);
    let request = Request {
        role: "qa_validator",
        last_developer_model: Some("opencode-go/deepseek-v4.1-flash"),
        ..Request::default()
    };
    let recommendation = choose(
        &cfg,
        &report,
        &Catalog::factory(),
        &request,
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.agent, "claude");
    assert_eq!(recommendation.model, "opus");
    assert_eq!(recommendation.family, "anthropic");
}

#[test]
fn sin_otra_familia_avisa_y_usa_la_del_developer() {
    let cfg = VoiceConfig::new();
    let mut catalog = Catalog::factory();
    catalog
        .entries
        .retain(|entry| entry.agent == "opencode" && entry.family == "deepseek");
    let request = Request {
        role: "qa_validator",
        last_developer_model: Some("deepseek-v4.1-flash"),
        ..Request::default()
    };
    let recommendation = choose(
        &cfg,
        &report(vec![agent("opencode", "OpenCode", UsageState::Libre)]),
        &catalog,
        &request,
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.family, "deepseek");
    assert!(
        recommendation
            .warnings
            .iter()
            .any(|warning| warning.contains("familia distinta")),
        "{:?}",
        recommendation.warnings
    );
}

#[test]
fn con_recursos_justos_gana_el_de_menor_memoria_y_lo_avisa() {
    let cfg = VoiceConfig::new();
    let report = report(vec![
        agent("opencode", "OpenCode", UsageState::Libre),
        agent("claude", "Claude Code", UsageState::Libre),
    ]);
    let pressure = Pressure {
        tight: true,
        reason: Some("RAM libre 1000 MB (mínimo 2500)".to_string()),
        memory_mb: vec![("opencode".to_string(), 3_000), ("claude".to_string(), 100)],
    };
    let recommendation = choose(
        &cfg,
        &report,
        &Catalog::factory(),
        &request("developer"),
        &pressure,
    )
    .expect("recomendación");
    assert_eq!(recommendation.agent, "claude");
    assert!(
        recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("menos memoria") && reason.contains("100 MB")),
        "{:?}",
        recommendation.reasons
    );
    assert!(
        recommendation
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Recursos justos:")),
        "{:?}",
        recommendation.warnings
    );
}

#[test]
fn el_tipo_o_el_modelo_pedidos_restringen_la_eleccion() {
    let cfg = VoiceConfig::new();
    let report = report(vec![
        agent("opencode", "OpenCode", UsageState::Libre),
        agent("claude", "Claude Code", UsageState::Libre),
    ]);
    let request = Request {
        role: "developer",
        agent: Some("claude"),
        ..Request::default()
    };
    let recommendation = choose(
        &cfg,
        &report,
        &Catalog::factory(),
        &request,
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.agent, "claude");

    let request = Request {
        role: "developer",
        model: Some("opencode-go/deepseek-v4.1-flash"),
        ..Request::default()
    };
    let recommendation = choose(
        &cfg,
        &report,
        &Catalog::factory(),
        &request,
        &Pressure::default(),
    )
    .expect("recomendación");
    assert_eq!(recommendation.model, "opencode-go/deepseek-v4.1-flash");
}

#[test]
fn sin_agentes_instalados_lo_dice() {
    let cfg = VoiceConfig::new();
    let mut unused = agent("opencode", "OpenCode", UsageState::Desconocido);
    unused.installed = false;
    let error = choose(
        &cfg,
        &report(vec![unused]),
        &Catalog::factory(),
        &request("developer"),
        &Pressure::default(),
    )
    .expect_err("sin agentes");
    assert!(error.contains("ningún agente"), "{error}");
}
