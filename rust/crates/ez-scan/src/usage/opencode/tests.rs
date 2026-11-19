//! Pruebas del lector de OpenCode con base y registro inventados.

use rusqlite::Connection;

use super::*;

const NOW: u64 = 1_767_355_200_000; // 2026-01-02T12:00:00Z
const LOG_LINE: &str = "timestamp=2026-01-02T11:00:00.000Z level=ERROR run=fake \
message=\"Failed to drain Session\" cause=\"AI.Error: OpenCode Go usage limit reached \
(5-hour, resets in 1h 30m). To keep going now, turn on Extra Usage or upgrade to Go Plus.\"";
const FREE_LINE: &str = "timestamp=2026-01-02T11:00:00.000Z level=ERROR run=fake \
message=\"Failed to drain Session\" cause=\"AI.Error: Free usage limit reached. \
Free models work in OpenCode; subscribe to OpenCode Go to keep going.\"";

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

fn database(root: &std::path::Path) {
    let folder = root.join("opencode");
    std::fs::create_dir_all(&folder).expect("carpeta");
    let connection = Connection::open(folder.join("opencode.db")).expect("base");
    connection
        .execute_batch(
            "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);
             CREATE TABLE session (id TEXT, time_updated INTEGER);",
        )
        .expect("tablas");
    connection
        .execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, 's', ?2, ?3)",
            rusqlite::params![
                "m1",
                1_767_353_400_000i64,
                r#"{"tokens":{"input":10,"output":20,"cache":{"read":30,"write":40}},"cost":0.5}"#
            ],
        )
        .expect("mensaje");
    connection
        .execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, 's', ?2, ?3)",
            rusqlite::params![
                "m2",
                1_767_315_600_000i64,
                r#"{"tokens":{"input":999},"cost":9.0}"#
            ],
        )
        .expect("mensaje viejo");
    connection
        .execute(
            "INSERT INTO session (id, time_updated) VALUES ('s', ?1)",
            [1_767_354_600_000i64],
        )
        .expect("sesión");
}

fn log(root: &std::path::Path, line: &str) {
    let folder = root.join("opencode").join("log");
    std::fs::create_dir_all(&folder).expect("carpeta");
    std::fs::write(folder.join("opencode.log"), format!("{line}\n")).expect("registro");
}

#[test]
fn suma_los_mensajes_de_la_ventana() {
    let root = std::env::temp_dir().join(format!("ez-scan-opencode-{}", std::process::id()));
    database(&root);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    let tokens = reading.tokens.expect("tokens");
    assert_eq!(tokens.total, 100);
    assert_eq!(reading.messages, Some(1));
    assert!((reading.cost_usd.expect("costo") - 0.5).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_aviso_del_registro_marca_agotado_con_reinicio() {
    let root = std::env::temp_dir().join(format!("ez-scan-opencode-log-{}", std::process::id()));
    log(&root, LOG_LINE);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_some());
    assert_eq!(reading.plan.as_deref(), Some("OpenCode Go"));
    assert_eq!(reading.resets_ms, Some(1_767_357_000_000));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_aviso_vencido_no_cuenta() {
    let root = std::env::temp_dir().join(format!("ez-scan-opencode-old-{}", std::process::id()));
    log(&root, LOG_LINE);
    let later = 1_767_376_800_000; // 18:00Z
    let reading = read(&dirs(&root), &Window::new(later, 5.0), later);
    assert!(reading.depleted.is_none());
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("ya venció")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_aviso_gratuito_no_trae_reinicio() {
    let notice = parse_notice(FREE_LINE).expect("aviso");
    assert_eq!(notice.plan.as_deref(), Some("Free"));
    assert!(notice.reset_ms.is_none());
}

#[test]
fn las_cuenta_atras_largas_se_entienden() {
    assert_eq!(
        countdown_ms("10d 3h"),
        Some(10 * 86_400_000 + 3 * 3_600_000)
    );
    assert_eq!(countdown_ms("4h 46m"), Some(4 * 3_600_000 + 46 * 60_000));
    assert_eq!(countdown_ms("sin números"), None);
}
