//! Pruebas con la captura real de OpenCode (NV-1c): últimas 15 líneas con texto.

use chrono::TimeZone;

use super::*;

const NOW: u64 = 1_700_000_000_000;

// ------------------------------------------- la pantalla real de OpenCode (NV-1c)

/// La captura real de `hc read --lines 0` de OpenCode (121 líneas, con relleno).
const OPENCODE_SCREEN: &str = include_str!("fixtures/pantalla-opencode.txt");

/// La captura con `line` justo después de la conversación, antes del relleno.
fn opencode_with(line: &str) -> String {
    let mut lines: Vec<&str> = OPENCODE_SCREEN.lines().collect();
    let index = lines
        .iter()
        .position(|text| text.contains("91.0 tok/s"))
        .expect("la captura tiene la línea del modelo");
    lines.insert(index + 1, line);
    lines.join("\n")
}

/// La hora de `hasta` de la primera entrada de `uso.json`, como `HH:MM` local.
fn usage_hour(root: &std::path::Path) -> String {
    let usage = usage(root);
    let until = usage[0]["hasta"].as_u64().expect("hora de reinicio");
    chrono::Local
        .timestamp_millis_opt(i64::try_from(until).expect("milisegundos"))
        .single()
        .expect("fecha local")
        .format("%H:%M")
        .to_string()
}

#[test]
fn la_captura_real_con_limite_al_final_se_bloquea_con_la_hora() {
    let root = temp_root("captura-limite");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let screen = opencode_with("Free limit reached. Resets at 7:00 p.m.");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", &screen);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    assert_eq!(
        registry
            .get("t6h", "dev1-opencode")
            .expect("leer")
            .expect("registro")
            .state,
        TaskState::Blocked
    );
    assert_eq!(usage_hour(&root), "19:00");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn la_captura_real_en_la_primera_vuelta_todavia_no_bloquea() {
    let root = temp_root("captura-primera");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let screen = opencode_with("Free limit reached. Resets at 7:00 p.m.");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", &screen);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();

    assert!(tick.blocked.is_empty(), "hace falta una segunda vuelta");
    assert_eq!(
        registry
            .get("t6h", "dev1-opencode")
            .expect("leer")
            .expect("registro")
            .state,
        TaskState::InProgress
    );
    assert!(!root.join("uso.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn la_frase_con_veinte_lineas_de_texto_debajo_no_cuenta() {
    let root = temp_root("captura-veinte");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let screen = opencode_with(&format!(
        "Free limit reached. Resets at 7:00 p.m.\n{}",
        "trabajando en el arreglo\n".repeat(20)
    ));
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", &screen);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();

    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::InProgress);
    assert!(!root.join("uso.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn la_captura_real_sin_frase_y_estable_no_bloquea() {
    let root = temp_root("captura-sin-frase");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", OPENCODE_SCREEN);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();

    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::InProgress);
    assert!(!root.join("uso.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}
