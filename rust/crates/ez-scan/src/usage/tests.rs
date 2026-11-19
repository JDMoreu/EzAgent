//! Pruebas del estado de la ventana, el catálogo y lo declarado.

use serde_json::{json, Value};

use super::*;

fn config(catalog: Value) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("agents", catalog);
    cfg.set(
        "agent_profiles",
        json!({
            "claude": {"label": "Claude Code"},
            "codex": {"label": "Codex"},
            "mcode": {"label": "MiniMax Code"},
            "qwen": {"label": "Qwen Code"}
        }),
    );
    cfg
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ez-scan-usage-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    root
}

fn dirs(root: &std::path::Path) -> UsageDirs {
    UsageDirs {
        home: root.to_path_buf(),
        appdata: root.to_path_buf(),
        codex: root.join("codex"),
        minimax: root.join("minimax"),
        opencode_data: root.join("opencode"),
        subagents: root.join("subagentes"),
    }
}

#[test]
fn el_estado_sale_del_porcentaje_de_la_ventana() {
    let mut reading = Reading {
        used_percent: Some(79.9),
        ..Reading::default()
    };
    assert_eq!(state_of(&reading, None).0, UsageState::Libre);
    reading.used_percent = Some(80.0);
    assert_eq!(state_of(&reading, None).0, UsageState::Cerca);
    reading.used_percent = Some(100.0);
    assert_eq!(state_of(&reading, None).0, UsageState::Agotado);
    reading.used_percent = None;
    assert_eq!(state_of(&reading, None).0, UsageState::Desconocido);
}

#[test]
fn lo_declarado_en_planes_manda_sobre_el_porcentaje() {
    let reading = Reading {
        used_percent: Some(99.0),
        tokens: Some(TokenUsage::new(100, 0, 0, 0)),
        ..Reading::default()
    };
    let (state, source) = state_of(&reading, Some(&UsageLimit::tokens(1_000.0)));
    assert_eq!(state, UsageState::Libre);
    assert!(source.contains("planes.toml"));

    let reading = Reading {
        messages: Some(9),
        ..Reading::default()
    };
    let (state, _) = state_of(&reading, Some(&UsageLimit::messages(10.0)));
    assert_eq!(state, UsageState::Cerca);
}

#[test]
fn un_aviso_vigente_gana_sobre_todo() {
    let reading = Reading {
        used_percent: Some(10.0),
        depleted: Some("vigilante: agotado hasta hoy".to_string()),
        ..Reading::default()
    };
    assert_eq!(state_of(&reading, None).0, UsageState::Agotado);
}

#[test]
fn el_catalogo_no_repite_el_mismo_programa_y_respeta_los_planes() {
    let root = temp_root("catalogo");
    let cfg = config(json!({
        "mcode": ["mcode"],
        "minimax": ["mcode"],
        "codex": ["codex"],
        "qwen": ["qwen"]
    }));
    let planes = planes::parse(
        "[codex]\nplan = \"plus\"\nventana_horas = 10\nprioridad = \"reservar\"\nlimite = 100\n",
    );
    let report = report_at(&cfg, &dirs(&root), &planes, 1_000, &|program| {
        program != "qwen"
    });
    assert_eq!(report.agents.len(), 3, "{report:#?}");
    let codex = report
        .agents
        .iter()
        .find(|agent| agent.key == "codex")
        .expect("codex");
    assert_eq!(codex.label, "Codex");
    assert_eq!(codex.plan.as_deref(), Some("plus"));
    assert_eq!(codex.window_hours, 10.0);
    assert_eq!(codex.priority, Some(Priority::Reservar));
    assert_eq!(codex.state, UsageState::Desconocido);
    assert!(codex
        .warnings
        .iter()
        .any(|warning| warning.contains("tokens")));
    let qwen = report
        .agents
        .iter()
        .find(|agent| agent.key == "qwen")
        .expect("qwen");
    assert!(!qwen.installed);
    assert_eq!(qwen.state, UsageState::Desconocido);
    assert!(qwen
        .warnings
        .iter()
        .any(|warning| warning.contains("no está instalado")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn una_ventana_invalida_cae_en_cinco_horas() {
    let window = Window::new(10 * 3_600_000, 0.0);
    assert_eq!(window.hours, DEFAULT_WINDOW_HOURS);
    assert_eq!(window.start_ms, 5 * 3_600_000);
    assert_eq!(window.minutes(), 300);
}

#[test]
fn un_agotamiento_por_modelo_no_agota_el_agente() {
    let mut reading = Reading {
        usable_models: vec![
            "opencode-go/deepseek-v4.1-flash".to_string(),
            "opencode/hy4-flash-free".to_string(),
        ],
        ..Reading::default()
    };
    reading.models = vec![ModelUsage {
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        tokens: None,
        messages: None,
        cost_usd: None,
        state: UsageState::Desconocido,
    }];
    let depletion = depleted::Depletion {
        whole_agent: None,
        models: vec![depleted::ModelDepletion {
            model: "opencode/hy4-flash-free".to_string(),
        }],
    };
    let usage = assemble(
        "opencode",
        "OpenCode".to_string(),
        true,
        None,
        5.0,
        reading,
        depletion,
    );
    assert_ne!(usage.state, UsageState::Agotado, "{usage:#?}");
    let free = usage
        .models
        .iter()
        .find(|model| model.model == "opencode/hy4-flash-free")
        .expect("el modelo gratuito aparece aunque no tenga uso");
    assert_eq!(free.state, UsageState::Agotado);
    let deepseek = usage
        .models
        .iter()
        .find(|model| model.model == "opencode-go/deepseek-v4.1-flash")
        .expect("deepseek sigue en el informe");
    assert_ne!(deepseek.state, UsageState::Agotado);
}

#[test]
fn un_agotamiento_sin_modelo_agota_el_agente() {
    let depletion = depleted::Depletion {
        whole_agent: Some("vigilante: agotado sin reinicio conocido (uso.json)".to_string()),
        models: Vec::new(),
    };
    let usage = assemble(
        "opencode",
        "OpenCode".to_string(),
        true,
        None,
        5.0,
        Reading::default(),
        depletion,
    );
    assert_eq!(usage.state, UsageState::Agotado);
    assert!(usage.sources.state.contains("vigilante"), "{usage:#?}");
}

#[test]
fn el_agente_solo_se_agota_si_todos_sus_modelos_usables_lo_estan() {
    let mut reading = Reading {
        usable_models: vec!["opencode-go/deepseek-v4.1-flash".to_string()],
        ..Reading::default()
    };
    reading.models = vec![ModelUsage {
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        tokens: None,
        messages: None,
        cost_usd: None,
        state: UsageState::Desconocido,
    }];
    let only = depleted::Depletion {
        whole_agent: None,
        models: vec![depleted::ModelDepletion {
            model: "opencode-go/deepseek-v4.1-flash".to_string(),
        }],
    };
    let usage = assemble(
        "opencode",
        "OpenCode".to_string(),
        true,
        None,
        5.0,
        reading,
        only,
    );
    assert_eq!(usage.state, UsageState::Agotado);

    let mut reading = Reading {
        usable_models: vec![
            "opencode-go/deepseek-v4.1-flash".to_string(),
            "opencode/hy4-flash-free".to_string(),
        ],
        ..Reading::default()
    };
    reading.models = vec![ModelUsage {
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        tokens: None,
        messages: None,
        cost_usd: None,
        state: UsageState::Desconocido,
    }];
    let partial = depleted::Depletion {
        whole_agent: None,
        models: vec![depleted::ModelDepletion {
            model: "opencode-go/deepseek-v4.1-flash".to_string(),
        }],
    };
    let usage = assemble(
        "opencode",
        "OpenCode".to_string(),
        true,
        None,
        5.0,
        reading,
        partial,
    );
    assert_ne!(usage.state, UsageState::Agotado, "{usage:#?}");
}
