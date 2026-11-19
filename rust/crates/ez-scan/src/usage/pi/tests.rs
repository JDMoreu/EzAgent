//! Pruebas del lector de Pi con sesiones inventadas.

use serde_json::json;

use super::*;

const NOW: u64 = 1_767_355_200_000; // 2026-01-02T12:00:00Z

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
fn suma_tokens_y_costo_de_la_ventana() {
    let root = std::env::temp_dir().join(format!("ez-scan-pi-{}", std::process::id()));
    let path = root
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join("-fake")
        .join("2026-01-02T11-30-00_fake.jsonl");
    std::fs::create_dir_all(path.parent().expect("carpeta")).expect("carpeta");
    let inside = json!({
        "type": "message",
        "message": {
            "role": "assistant",
            "model": "modelo-inventado",
            "timestamp": 1_767_353_400_000u64,
            "usage": {
                "input": 100, "output": 50, "cacheRead": 20, "cacheWrite": 10,
                "reasoning": 5, "totalTokens": 185,
                "cost": {"input": 0.1, "output": 0.2, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.3}
            }
        }
    });
    let outside = json!({
        "type": "message",
        "message": {
            "timestamp": 1_767_315_600_000u64,
            "usage": {"input": 999, "output": 999, "cost": {"total": 9.0}}
        }
    });
    std::fs::write(&path, format!("{inside}\n{outside}\n")).expect("sesión");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    let tokens = reading.tokens.expect("tokens");
    assert_eq!(tokens.total, 180);
    assert_eq!(reading.messages, Some(1));
    assert!((reading.cost_usd.expect("costo") - 0.3).abs() < 1e-9);
    assert_eq!(reading.models.len(), 1);
    assert_eq!(reading.models[0].model, "modelo-inventado");
    assert!((reading.models[0].cost_usd.expect("costo del modelo") - 0.3).abs() < 1e-9);
    assert_eq!(reading.observed_ms, Some(1_767_353_400_000));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_sesiones_avisa() {
    let root = std::env::temp_dir().join(format!("ez-scan-pi-none-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.tokens.is_none());
    assert!(!reading.warnings.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
