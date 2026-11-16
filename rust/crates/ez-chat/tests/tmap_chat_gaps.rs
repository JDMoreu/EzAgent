//! Conductas de `store.py` y `session.py` que los fixtures no ejercían (TMAP-1).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ez_chat::session::ChatModel;
use ez_chat::store::{ChatStore, Role, Source};
use serde_json::Value;

fn temp(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "ez-chat-tmap-{name}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("carpeta temporal");
    dir.join("chat.jsonl")
}

fn lines(path: &PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("historial")
        .lines()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect()
}

#[test]
fn every_record_carries_a_persisted_seq() {
    let path = temp("seq");
    let store = ChatStore::new(Some(path.clone()));
    store.append(Role::User, "hola", Source::Chat, None);
    store.new_session();
    store.resume(1);
    let records = lines(&path);
    let pairs: Vec<(Option<&str>, i64)> = records
        .iter()
        .map(|record| {
            (
                record["event"].as_str(),
                record["seq"].as_i64().unwrap_or(-1),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![(None, 1), (Some("session"), 2), (Some("resume"), 3)]
    );
}

#[test]
fn the_order_does_not_depend_on_a_clock_that_goes_backwards() {
    let path = temp("clock");
    let times = Arc::new(Mutex::new(vec![900.0, 800.0, 700.0, 600.0]));
    let clock_times = Arc::clone(&times);
    let store = ChatStore::with_clock(
        Some(path.clone()),
        Arc::new(move || clock_times.lock().expect("reloj").remove(0)),
    );
    store.append(Role::User, "uno", Source::Chat, None);
    let one = store.session();
    store.new_session();
    store.append(Role::User, "dos", Source::Chat, None);
    let two = store.session();
    assert!(store.resume(one));
    assert_eq!(store.entries(Some(one))[0].t, 900.0);
    let reloaded = ChatStore::new(Some(path));
    let order: Vec<i64> = reloaded
        .sessions()
        .iter()
        .map(|info| info.session)
        .collect();
    assert_eq!(order, vec![one, two]);
}

#[test]
fn old_files_without_seq_use_the_line_index() {
    let path = temp("legacy");
    let rows = [
        r#"{"v": 1, "t": 1.0, "session": 1, "role": "user", "source": "chat", "text": "uno"}"#,
        r#"{"v": 1, "t": 1.0, "event": "session", "session": 2}"#,
        r#"{"v": 1, "t": 1.0, "session": 2, "role": "user", "source": "chat", "text": "dos"}"#,
        r#"{"v": 1, "t": 1.0, "event": "resume", "session": 1}"#,
    ];
    fs::write(&path, rows.join("\n") + "\n").expect("archivo antiguo");
    let store = ChatStore::new(Some(path.clone()));
    let order: Vec<i64> = store.sessions().iter().map(|info| info.session).collect();
    assert_eq!(order, vec![1, 2]);
    store.append(Role::User, "tres", Source::Chat, None);
    let last = lines(&path).pop().expect("última línea");
    assert_eq!(last["seq"], 5);
}

#[test]
fn voice_turns_are_part_of_the_same_thread() {
    let model = ChatModel::new(None, false);
    model.add_user("qué hora es", Source::Voice);
    let reply = model.begin_assistant(Source::Voice);
    model.append(reply.id, "Son las tres.");
    model.finish(reply.id);
    let (_, messages) = model.snapshot();
    let pairs: Vec<(&str, &str)> = messages
        .iter()
        .map(|message| (message.role.as_str(), message.source.as_str()))
        .collect();
    assert_eq!(pairs, vec![("user", "voice"), ("assistant", "voice")]);
}
