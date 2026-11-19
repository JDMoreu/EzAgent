//! Pruebas del lector de Codex con sesiones inventadas.

use serde_json::json;

use super::*;

const NOW: u64 = 1_767_355_200_000; // 2026-01-02T12:00:00Z
const RESET_SECONDS: i64 = 1_767_358_800; // 2026-01-02T13:00:00Z

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

fn session(root: &std::path::Path) -> std::path::PathBuf {
    let path = root
        .join("codex")
        .join("sessions")
        .join("2026")
        .join("01")
        .join("02")
        .join("rollout-2026-01-02T11-55-00-fake.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta");
    }
    path
}

fn lines() -> String {
    let first = json!({
        "timestamp": "2026-01-02T11:00:00.000Z",
        "type": "event_msg",
        "payload": {"type": "token_count", "rate_limits": {
            "primary": {"used_percent": 40.0, "window_minutes": 300, "resets_at": RESET_SECONDS},
            "secondary": {"used_percent": 10.0, "window_minutes": 10_080, "resets_at": RESET_SECONDS},
            "plan_type": "plus",
            "rate_limit_reached_type": null,
            "credits": {"has_credits": false, "unlimited": false}
        }}
    });
    let last = json!({
        "timestamp": "2026-01-02T11:55:00.000Z",
        "type": "event_msg",
        "payload": {"type": "token_count", "rate_limits": {
            "primary": {"used_percent": 85.0, "window_minutes": 300, "resets_at": RESET_SECONDS},
            "secondary": {"used_percent": 54.0, "window_minutes": 10_080, "resets_at": RESET_SECONDS},
            "plan_type": "plus",
            "rate_limit_reached_type": null,
            "credits": {"has_credits": false, "unlimited": false}
        }}
    });
    format!("{first}\n{last}\n")
}

#[test]
fn toma_el_ultimo_porcentaje_y_su_reinicio() {
    let root = std::env::temp_dir().join(format!("ez-scan-codex-{}", std::process::id()));
    std::fs::write(session(&root), lines()).expect("sesión");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert_eq!(reading.used_percent, Some(85.0));
    assert_eq!(reading.resets_ms, Some(1_767_358_800_000));
    assert_eq!(reading.plan.as_deref(), Some("plus"));
    assert_eq!(reading.observed_ms, Some(1_767_354_900_000));
    assert_eq!(reading.observed_window_hours, Some(5.0));
    assert!(reading.stale.is_none(), "{:?}", reading.stale);
    assert_eq!(reading.sources.state, "85.0 % observado de la ventana");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_dato_con_la_ventana_vencida_no_es_actual() {
    let root = std::env::temp_dir().join(format!("ez-scan-codex-old-{}", std::process::id()));
    std::fs::write(session(&root), lines()).expect("sesión");
    let later = 1_767_362_400_000; // 14:00Z
    let reading = read(&dirs(&root), &Window::new(later, 5.0), later);
    assert_eq!(reading.used_percent, Some(85.0));
    let stale = reading.stale.expect("dato viejo");
    assert!(stale.contains("ya reinició"), "{stale}");
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("dato de Codex")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_sesiones_avisa_y_no_inventa() {
    let root = std::env::temp_dir().join(format!("ez-scan-codex-none-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.used_percent.is_none());
    assert!(!reading.warnings.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
