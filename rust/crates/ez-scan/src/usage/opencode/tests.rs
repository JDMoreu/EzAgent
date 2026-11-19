//! Pruebas del lector de OpenCode con base y registro inventados.
//!
//! Se crean las dos tablas que existen en la base real (`session_message` de
//! v2 y `message` de versiones viejas) para probar la elección y el respaldo.

use serde_json::json;

use super::*;

const NOW: u64 = 1_767_355_200_000; // 2026-01-02T12:00:00Z
const AT_1130: i64 = 1_767_353_400_000;
const AT_1155: i64 = 1_767_354_900_000;
const AT_1030: i64 = 1_767_349_800_000;
const AT_0100: i64 = 1_767_315_600_000;

const FREE_LINE: &str = "timestamp=2026-01-02T11:00:00.000Z level=ERROR run=fake \
message=\"Failed to drain Session\" cause=\"AI.Error: Free usage limit reached. \
Free models work in OpenCode; subscribe to OpenCode Go to keep going.\"";
const GO_LINE_1130: &str = "timestamp=2026-01-02T11:00:00.000Z level=ERROR run=fake \
message=\"Failed to drain Session\" cause=\"AI.Error: OpenCode Go usage limit reached \
(5-hour, resets in 2h). To keep going now, turn on Extra Usage or upgrade to Go Plus.\"";
const GO_LINE_0900: &str = "timestamp=2026-01-02T09:00:00.000Z level=ERROR run=fake \
message=\"Failed to drain Session\" cause=\"AI.Error: OpenCode Go usage limit reached \
(5-hour, resets in 2h). To keep going now, turn on Extra Usage or upgrade to Go Plus.\"";

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

fn open(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ez-scan-opencode-{name}-{}", std::process::id()));
    std::fs::create_dir_all(root.join("opencode")).expect("carpeta");
    root
}

fn current_db(root: &std::path::Path) -> Connection {
    let connection = Connection::open(root.join("opencode").join("opencode.db")).expect("base");
    connection
        .execute_batch(
            "CREATE TABLE session_message (
                id TEXT, session_id TEXT, type TEXT, seq INTEGER,
                time_created INTEGER, time_updated INTEGER, data TEXT
            );",
        )
        .expect("tabla actual");
    connection
}

fn legacy_db(root: &std::path::Path) -> Connection {
    let connection = Connection::open(root.join("opencode").join("opencode.db")).expect("base");
    connection
        .execute_batch(
            "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .expect("tabla vieja");
    connection
}

fn insert(
    connection: &Connection,
    at: i64,
    provider: &str,
    model: &str,
    tokens: [i64; 4],
    cost: f64,
) {
    let data = json!({
        "model": {"providerID": provider, "id": model, "variant": "max"},
        "tokens": {
            "input": tokens[0], "output": tokens[1], "reasoning": 0,
            "cache": {"read": tokens[2], "write": tokens[3]}
        },
        "cost": cost,
        "time": {"created": at}
    });
    connection
        .execute(
            "INSERT INTO session_message
             (id, session_id, type, seq, time_created, time_updated, data)
             VALUES (?1, 's', 'assistant', 0, ?2, ?2, ?3)",
            rusqlite::params![format!("m-{at}-{model}"), at, data.to_string()],
        )
        .expect("fila");
}

fn insert_legacy(
    connection: &Connection,
    at: i64,
    provider: &str,
    model: &str,
    tokens: [i64; 4],
    cost: f64,
) {
    let data = json!({
        "providerID": provider,
        "modelID": model,
        "tokens": {
            "input": tokens[0], "output": tokens[1], "reasoning": 0,
            "cache": {"read": tokens[2], "write": tokens[3]}
        },
        "cost": cost
    });
    connection
        .execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, 's', ?2, ?3)",
            rusqlite::params![format!("old-{at}"), at, data.to_string()],
        )
        .expect("fila vieja");
}

fn log(root: &std::path::Path, lines: &[&str]) {
    let folder = root.join("opencode").join("log");
    std::fs::create_dir_all(&folder).expect("carpeta del registro");
    std::fs::write(
        folder.join("opencode.log"),
        format!("{}\n", lines.join("\n")),
    )
    .expect("registro");
}

#[test]
fn lee_la_tabla_actual_y_desglosa_por_modelo() {
    let root = open("actual");
    let connection = current_db(&root);
    insert(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 50, 20, 10],
        0.5,
    );
    insert(
        &connection,
        AT_1155,
        "opencode",
        "hy4-flash-free",
        [1, 2, 3, 4],
        0.0,
    );
    insert(
        &connection,
        AT_0100,
        "opencode-go",
        "deepseek-v4.1-flash",
        [999, 999, 999, 999],
        9.0,
    );
    drop(connection);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    let tokens = reading.tokens.expect("tokens");
    assert_eq!(tokens.total, 190);
    assert_eq!(reading.messages, Some(2));
    assert_eq!(reading.models.len(), 2, "{:?}", reading.models);
    assert_eq!(reading.models[0].model, "opencode-go/deepseek-v4.1-flash");
    assert_eq!(reading.models[0].tokens.expect("tokens").total, 180);
    assert_eq!(reading.observed_ms, Some(1_767_354_900_000));
    assert!(reading
        .warnings
        .iter()
        .all(|warning| !warning.contains("no se actualiza")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_aviso_free_no_agota_el_plan_go() {
    let root = open("free");
    let connection = current_db(&root);
    insert(
        &connection,
        AT_1030,
        "opencode",
        "hy4-flash-free",
        [10, 20, 30, 40],
        0.0,
    );
    insert(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 50, 20, 10],
        0.0,
    );
    drop(connection);
    log(&root, &[FREE_LINE]);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_none(), "{reading:#?}");
    let free = reading
        .models
        .iter()
        .find(|model| model.model.ends_with("hy4-flash-free"))
        .expect("modelo gratuito");
    assert_eq!(free.state, UsageState::Agotado);
    let video = reading
        .models
        .iter()
        .find(|model| model.model.starts_with("opencode-go/"))
        .expect("modelo go");
    assert_ne!(video.state, UsageState::Agotado);
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("está agotado; el agente no")));
    assert_eq!(reading.plan.as_deref(), Some("OpenCode Go"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_aviso_go_con_reinicio_vencido_no_cuenta() {
    let root = open("vencido");
    let connection = current_db(&root);
    insert(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 50, 20, 10],
        0.0,
    );
    drop(connection);
    log(&root, &[GO_LINE_0900]);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_none(), "{reading:#?}");
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("ya no cuenta")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_uso_posterior_del_mismo_plan_vence_el_aviso() {
    let root = open("posterior");
    let connection = current_db(&root);
    insert(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 50, 20, 10],
        0.0,
    );
    drop(connection);
    log(&root, &[GO_LINE_1130]);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_none(), "{reading:#?}");
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("ya no cuenta")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_plan_usado_hace_poco_no_agota_el_agente() {
    let root = open("plan-reciente");
    let connection = current_db(&root);
    connection
        .execute_batch("CREATE TABLE session_v2 (id TEXT, model TEXT, time_updated INTEGER);")
        .expect("tabla de sesiones");
    connection
        .execute(
            "INSERT INTO session_v2 (id, model, time_updated) VALUES ('s', ?1, ?2)",
            rusqlite::params![
                r#"{"providerID":"opencode-go","id":"deepseek-v4.1-flash"}"#,
                NOW as i64 - 3_600_000
            ],
        )
        .expect("sesión reciente");
    insert(
        &connection,
        AT_1030,
        "opencode",
        "hy4-flash-free",
        [10, 20, 30, 40],
        0.0,
    );
    drop(connection);
    log(&root, &[FREE_LINE]);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_none(), "{reading:#?}");
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("está agotado; el agente no")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn todos_los_modelos_agotados_marcan_el_agente() {
    let root = open("todos");
    let connection = current_db(&root);
    insert(
        &connection,
        AT_1030,
        "opencode",
        "hy4-flash-free",
        [10, 20, 30, 40],
        0.0,
    );
    drop(connection);
    log(&root, &[FREE_LINE]);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.depleted.is_some(), "{reading:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn prefiere_la_tabla_mas_reciente_y_conserva_la_vieja() {
    let root = open("dos-tablas");
    let connection = current_db(&root);
    connection
        .execute_batch(
            "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .expect("tabla vieja");
    insert(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 0, 0, 0],
        0.0,
    );
    insert_legacy(
        &connection,
        AT_0100,
        "opencode-go",
        "modelo-viejo",
        [999, 0, 0, 0],
        0.0,
    );
    drop(connection);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert_eq!(reading.tokens.expect("tokens").total, 100);
    assert_eq!(reading.models[0].model, "opencode-go/deepseek-v4.1-flash");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_esquema_viejo_sigue_sirviendo() {
    let root = open("viejo");
    let connection = legacy_db(&root);
    insert_legacy(
        &connection,
        AT_1130,
        "opencode-go",
        "deepseek-v4.1-flash",
        [100, 50, 20, 10],
        0.5,
    );
    drop(connection);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert_eq!(reading.tokens.expect("tokens").total, 180);
    assert_eq!(reading.models[0].model, "opencode-go/deepseek-v4.1-flash");
    assert!(reading.depleted.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_base_avisa_y_no_inventa() {
    let root = open("sin-base");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.tokens.is_none());
    assert!(reading
        .warnings
        .iter()
        .any(|warning| warning.contains("OpenCode")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_aviso_gratuito_no_trae_reinicio() {
    let notice = parse_notice(FREE_LINE).expect("aviso");
    assert_eq!(notice.plan, Plan::Free);
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
