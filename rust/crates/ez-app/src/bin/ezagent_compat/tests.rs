//! Prueba de ida y vuelta del arnés: lo que escribe cada `save-*` se puede
//! volver a leer con la API real. El arnés de Python comprueba lo mismo contra
//! el Python del `.venv`; esto cubre la mitad Rust sin ese entorno.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use serde_json::json;

fn temp_home(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = std::env::temp_dir().join(format!("ezcompat-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("carpeta temporal propia");
    dir
}

fn probe_flags(home: &Path, extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut flags = BTreeMap::new();
    flags.insert("home".to_string(), home.display().to_string());
    for (key, value) in extra {
        flags.insert((*key).to_string(), (*value).to_string());
    }
    flags
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("archivo escrito"))
        .expect("JSON válido")
}

#[test]
fn settings_and_lists_survive_a_roundtrip() {
    let home = temp_home("ajustes");
    std::fs::write(
        home.join("voice.json"),
        r#"{"ui_scale": 1.0, "clave_futura": {"x": 1}}"#,
    )
    .expect("voice.json inicial");
    save_voice(&home, &probe_flags(&home, &[("set", "ui_scale=1.25")])).expect("guardar");
    let saved = read_json(&home.join("voice.json"));
    assert_eq!(saved["ui_scale"], json!(1.25));
    assert_eq!(saved["clave_futura"]["x"], json!(1));
    assert_eq!(saved["hotkey"], json!("ctrl+alt+space"));

    save_tasks(&home).expect("tareas");
    let registry = TaskRegistry::new(home.join("tasks.json"));
    assert_eq!(registry.all().len(), 1);
    assert_eq!(registry.chains().len(), 1);

    save_journal(&home).expect("diario");
    let journal = Journal::new(Some(&home.join("journal"))).expect("diario de nuevo");
    let entries = journal.load_entries_value();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["kind"], json!("create"));

    save_memory(&home).expect("memoria");
    let store = memory_store(&home).expect("memoria de nuevo");
    assert!(store.count_messages().expect("cuenta") >= 1);
    assert!(store
        .read_markdown("memory")
        .contains("hecho de compatibilidad"));
}

#[test]
fn chat_rewrite_is_readable_by_a_fresh_store() {
    let home = temp_home("chat");
    let out = save_chat(&home).expect("guardar chat");
    assert_eq!(out["replaced"], json!(true));
    assert_eq!(out["resumed"], json!(true));
    let store = ChatStore::new(Some(chat_path(&home)));
    let texts: Vec<String> = store
        .recent(10)
        .iter()
        .map(|entry| entry.text.clone())
        .collect();
    assert!(texts
        .iter()
        .any(|text| text == "respuesta corregida de compatibilidad F7-03"));
    assert!(!texts
        .iter()
        .any(|text| text == "respuesta de compatibilidad F7-03"));
}

#[test]
fn board_written_by_the_probe_is_readable() {
    let home = temp_home("tablero");
    let saved = save_board(&home).expect("guardar tablero");
    let swarm = saved["swarm"].as_str().expect("id").to_string();
    let db = Db::open(home.join("swarm").join("board.db")).expect("tablero de nuevo");
    let info = db.get(&swarm).expect("consulta").expect("enjambre");
    assert_eq!(info.goal, "Compatibilidad F7-03");
    assert_eq!(db.members(&swarm).expect("miembros").len(), 1);
    assert_eq!(db.tasks(&swarm).expect("tareas").len(), 1);
    assert_eq!(db.history(&swarm, 0, 10).expect("mensajes").len(), 1);
}
