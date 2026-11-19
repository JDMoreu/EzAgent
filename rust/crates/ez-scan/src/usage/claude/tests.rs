//! Pruebas del lector de Claude Code con sesiones inventadas.

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

fn home(root: &std::path::Path) {
    let path = root
        .join(".claude")
        .join("projects")
        .join("-fake")
        .join("sesion.jsonl");
    std::fs::create_dir_all(path.parent().expect("carpeta")).expect("carpeta");
    let inside = json!({
        "type": "assistant",
        "timestamp": "2026-01-02T11:30:00.000Z",
        "message": {"model": "modelo-inventado", "usage": {
            "input_tokens": 100, "output_tokens": 50,
            "cache_read_input_tokens": 20, "cache_creation_input_tokens": 10,
            "fallback_credit": null
        }}
    });
    let outside = json!({
        "type": "assistant",
        "timestamp": "2026-01-02T01:00:00.000Z",
        "message": {"usage": {"input_tokens": 999, "output_tokens": 999}}
    });
    let fallback = json!({
        "type": "assistant",
        "timestamp": "2026-01-02T11:40:00.000Z",
        "message": {"model": "modelo-inventado", "usage": {
            "input_tokens": 0, "output_tokens": 0,
            "fallback_credit": {"used": true}
        }}
    });
    std::fs::write(&path, format!("{inside}\n{outside}\n{fallback}\n")).expect("sesión");
    std::fs::write(
        root.join(".claude.json"),
        serde_json::to_string(&json!({
            "oauthAccount": {
                "organizationType": "claude_pro",
                "billingType": "stripe_subscription",
                "emailAddress": "no-leer@example.invalid",
                "fullName": "No Leer"
            }
        }))
        .expect("json"),
    )
    .expect("config");
}

#[test]
fn suma_solo_lo_de_la_ventana_y_detecta_el_respaldo() {
    let root = std::env::temp_dir().join(format!("ez-scan-claude-{}", std::process::id()));
    home(&root);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    let tokens = reading.tokens.expect("tokens");
    assert_eq!(tokens.input, 100);
    assert_eq!(tokens.output, 50);
    assert_eq!(tokens.cache_read, 20);
    assert_eq!(tokens.cache_write, 10);
    assert_eq!(tokens.total, 180);
    assert_eq!(reading.messages, Some(2));
    assert_eq!(reading.models.len(), 1);
    assert_eq!(reading.models[0].model, "modelo-inventado");
    assert_eq!(
        reading.models[0].tokens.expect("tokens del modelo").total,
        180
    );
    assert!(reading.depleted.is_some());
    assert_eq!(reading.plan.as_deref(), Some("claude_pro"));
    assert_eq!(reading.observed_ms, Some(1_767_354_000_000));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_carpeta_de_sesiones_el_uso_queda_desconocido() {
    let root = std::env::temp_dir().join(format!("ez-scan-claude-none-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.tokens.is_none());
    assert!(!reading.warnings.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
