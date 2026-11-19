//! Pruebas del lector de MiniMax Code con una SQLite inventada.

use rusqlite::Connection;

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

fn database(root: &std::path::Path) {
    let folder = root.join("minimax").join("v2").join("sqlite");
    std::fs::create_dir_all(&folder).expect("carpeta");
    let connection = Connection::open(folder.join("runtime-state.sqlite")).expect("base");
    connection
        .execute_batch(
            "CREATE TABLE local_runtime_token_usage (
                id INTEGER PRIMARY KEY, session_id TEXT, model TEXT, ts INTEGER,
                input_tokens INTEGER, output_tokens INTEGER, reasoning_tokens INTEGER,
                cache_read_tokens INTEGER, cache_write_tokens INTEGER, cost_usd REAL
            );",
        )
        .expect("tabla");
    connection
        .execute(
            "INSERT INTO local_runtime_token_usage
             (model, ts, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd)
             VALUES ('minimax/MiniMax-M3.1-Flash', ?1, 10, 20, 30, 40, 0.5)",
            [1_767_353_400_000i64],
        )
        .expect("fila de la ventana");
    connection
        .execute(
            "INSERT INTO local_runtime_token_usage
             (model, ts, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd)
             VALUES ('modelo-viejo', ?1, 999, 999, 999, 999, 9.0)",
            [1_767_315_600_000i64],
        )
        .expect("fila vieja");
}

#[test]
fn suma_las_filas_de_la_ventana() {
    let root = std::env::temp_dir().join(format!("ez-scan-mcode-{}", std::process::id()));
    database(&root);
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    let tokens = reading.tokens.expect("tokens");
    assert_eq!(tokens.total, 100);
    assert_eq!(reading.messages, Some(1));
    assert!((reading.cost_usd.expect("costo") - 0.5).abs() < 1e-9);
    assert_eq!(reading.models.len(), 1);
    assert_eq!(reading.models[0].model, "minimax/MiniMax-M3.1-Flash");
    assert_eq!(reading.models[0].tokens.expect("tokens del modelo").total, 100);
    assert_eq!(reading.observed_ms, Some(1_767_353_400_000));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_base_avisa_y_no_inventa() {
    let root = std::env::temp_dir().join(format!("ez-scan-mcode-none-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    let reading = read(&dirs(&root), &Window::new(NOW, 5.0), NOW);
    assert!(reading.tokens.is_none());
    assert!(reading.warnings[0].contains("MiniMax"));
    let _ = std::fs::remove_dir_all(&root);
}
