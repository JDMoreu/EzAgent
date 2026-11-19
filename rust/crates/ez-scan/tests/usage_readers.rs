//! Lectores de uso con una carpeta de usuario inventada (`tests/fixtures/usage`).
//!
//! Son datos de ejemplo escritos a mano: fechas de 2026-01-02, modelos y
//! sesiones falsas. Ninguna prueba toca la carpeta real de la persona.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_scan::usage::{self, Priority, UsageDirs, UsageState};
use rusqlite::Connection;
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
        opencode_data: temp_root.join("opencode"),
        subagents: temp_root.join("subagentes"),
    }
}

/// Base de OpenCode con el esquema actual y un aviso `Free` vigente, pero con
/// uso de `opencode-go` posterior: el agente no debe quedar agotado.
fn opencode(temp_root: &Path) {
    let folder = temp_root.join("opencode");
    std::fs::create_dir_all(folder.join("log")).expect("carpeta de OpenCode");
    let connection = Connection::open(folder.join("opencode.db")).expect("base de OpenCode");
    connection
        .execute_batch(
            "CREATE TABLE session_message (
                id TEXT, session_id TEXT, type TEXT, seq INTEGER,
                time_created INTEGER, time_updated INTEGER, data TEXT
            );",
        )
        .expect("tabla de OpenCode");
    for (at, provider, model, tokens) in [
        (
            1_767_349_800_000i64,
            "opencode",
            "hy4-flash-free",
            [10, 20, 30, 40],
        ),
        (
            1_767_353_400_000i64,
            "opencode-go",
            "deepseek-v4.1-flash",
            [100, 50, 20, 10],
        ),
    ] {
        let data = json!({
            "model": {"providerID": provider, "id": model},
            "tokens": {
                "input": tokens[0], "output": tokens[1], "reasoning": 0,
                "cache": {"read": tokens[2], "write": tokens[3]}
            },
            "cost": 0.0,
            "time": {"created": at}
        });
        connection
            .execute(
                "INSERT INTO session_message
                 (id, session_id, type, seq, time_created, time_updated, data)
                 VALUES (?1, 's', 'assistant', 0, ?2, ?2, ?3)",
                rusqlite::params![format!("m-{at}"), at, data.to_string()],
            )
            .expect("fila de OpenCode");
    }
    drop(connection);
    std::fs::write(
        folder.join("log").join("opencode.log"),
        "timestamp=2026-01-02T11:00:00.000Z level=ERROR run=fake \
         message=\"Failed to drain Session\" cause=\"AI.Error: Free usage limit reached. \
         Free models work in OpenCode; subscribe to OpenCode Go to keep going.\"\n",
    )
    .expect("registro de OpenCode");
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
    opencode(&temp_root);
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
    assert_ne!(opencode.state, UsageState::Agotado, "{opencode:#?}");
    assert_eq!(opencode.tokens.expect("tokens").total, 280);
    assert_eq!(opencode.messages, Some(2));
    assert_eq!(opencode.plan.as_deref(), Some("OpenCode Go"));
    assert!(
        opencode
            .models
            .iter()
            .any(|model| model.model.ends_with("hy4-flash-free")
                && model.state == UsageState::Agotado)
    );
    assert!(
        opencode
            .models
            .iter()
            .any(|model| model.model.starts_with("opencode-go/")
                && model.state != UsageState::Agotado)
    );
    assert!(opencode
        .warnings
        .iter()
        .all(|warning| !warning.contains("no se actualiza")));

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
