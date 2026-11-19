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
