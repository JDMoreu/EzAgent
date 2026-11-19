//! Lectores de uso con una carpeta de usuario inventada (`tests/fixtures/usage`).
//!
//! Son datos de ejemplo escritos a mano: fechas de 2026-01-02, modelos y
//! sesiones falsas. Ninguna prueba toca la carpeta real de la persona.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_scan::usage::{self, Priority, UsageDirs, UsageState};
use serde_json::json;

const NOW: u64 = 1_767_355_200_000; // 2026-01-02T12:00:00Z

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("usage")
}

fn temp(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("ez-scan-usage-e2e-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("carpeta temporal");
    path
}

fn dirs(temp_root: &Path) -> UsageDirs {
    let home = fixtures().join("home");
    UsageDirs {
        home: home.clone(),
        appdata: fixtures().join("appdata").join("EzAgent"),
        codex: home.join(".codex"),
        minimax: temp_root.join("minimax"),
        opencode_data: home.join(".local").join("share").join("opencode"),
        subagents: temp_root.join("subagentes"),
    }
}

fn config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set(
        "agents",
        json!({
            "claude": ["claude"],
            "opencode": ["opencode"],
            "mcode": ["mcode"],
            "minimax": ["mcode"],
            "codex": ["codex"],
            "qwen": ["qwen"],
            "pi": ["pi"]
        }),
    );
    cfg.set(
        "agent_profiles",
        json!({
            "claude": {"label": "Claude Code"},
            "opencode": {"label": "OpenCode"},
            "mcode": {"label": "MiniMax Code"},
            "minimax": {"label": "MiniMax (mcode)"},
            "codex": {"label": "Codex"},
            "qwen": {"label": "Qwen Code"},
            "pi": {"label": "Pi"}
        }),
    );
    cfg
}

fn agent<'a>(report: &'a usage::UsageReport, key: &str) -> &'a usage::AgentUsage {
    report
        .agents
        .iter()
        .find(|agent| agent.key == key)
        .unwrap_or_else(|| panic!("falta {key}: {report:#?}"))
}

#[test]
fn lee_los_ejemplos_de_cada_agente_sin_tocar_la_carpeta_real() {
    let temp_root = temp("lectores");
    let dirs = dirs(&temp_root);
    let planes = usage::load_planes(&dirs);
    assert!(planes.warnings.is_empty(), "{:?}", planes.warnings);
    let report = usage::report_at(&config(), &dirs, &planes, NOW, &|program| program != "qwen");
    assert_eq!(report.agents.len(), 6, "{report:#?}");

    let claude = agent(&report, "claude");
    assert_eq!(claude.window_hours, 10.0);
    assert_eq!(claude.priority, Some(Priority::Primero));
    assert_eq!(claude.plan.as_deref(), Some("claude_max"));
    assert_eq!(claude.tokens.expect("tokens").total, 180);
    assert_eq!(claude.messages, Some(1));
    assert_eq!(claude.state, UsageState::Libre);
    assert!(claude
        .warnings
        .iter()
        .any(|warning| warning.contains("no coincide")));

    let codex = agent(&report, "codex");
    assert_eq!(codex.used_percent, Some(82.0));
    assert_eq!(codex.observed_window_hours, Some(5.0));
    assert_eq!(codex.plan.as_deref(), Some("plus"));
    assert_eq!(codex.state, UsageState::Cerca);
    assert_eq!(codex.reset_text.as_deref().map(str::len), Some(16));

    let opencode = agent(&report, "opencode");
    assert_eq!(opencode.state, UsageState::Agotado);
    assert_eq!(opencode.plan.as_deref(), Some("OpenCode Go"));
    assert!(opencode.sources.state.contains("OpenCode"), "{opencode:#?}");

    let pi = agent(&report, "pi");
    assert_eq!(pi.tokens.expect("tokens").total, 180);
    assert!((pi.cost_usd.expect("costo") - 0.3).abs() < 1e-9);
    assert_eq!(pi.state, UsageState::Desconocido);

    let mcode = agent(&report, "mcode");
    assert!(mcode.installed);
    assert!(mcode.tokens.is_none());
    assert_eq!(mcode.state, UsageState::Desconocido);
    assert!(!report.agents.iter().any(|agent| agent.key == "minimax"));

    let qwen = agent(&report, "qwen");
    assert!(!qwen.installed);
    assert!(qwen
        .warnings
        .iter()
        .any(|warning| warning.contains("no está instalado")));

    let json = serde_json::to_string(&report).expect("JSON");
    assert!(!json.contains("no-leer@example.invalid"), "{json}");
    assert!(!json.contains("Persona Inventada"), "{json}");
    let _ = std::fs::remove_dir_all(&temp_root);
}
