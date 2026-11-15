use super::*;

fn temp_store(name: &str) -> (PathBuf, ChatStore) {
    let dir = std::env::temp_dir().join(format!("ez-chat-test-{name}-{}", std::process::id()));
    let path = dir.join("chat.jsonl");
    (path.clone(), ChatStore::new(Some(path)))
}

#[test]
fn entries_survive_a_restart() {
    let (path, store) = temp_store("restart");
    store.append(Role::User, "hola", Source::Chat, Some(1.0));
    store.append(Role::Assistant, "¡Hola!", Source::Chat, Some(2.0));
    let again = ChatStore::new(Some(path));
    assert_eq!(
        again
            .entries(None)
            .iter()
            .map(|entry| (entry.role, entry.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(Role::User, "hola"), (Role::Assistant, "¡Hola!")]
    );
    assert_eq!(again.session(), store.session());
}

#[test]
fn the_file_is_plain_jsonl_and_unicode_is_kept() {
    let (path, store) = temp_store("jsonl");
    store.append(
        Role::User,
        "añade café ☕ y ñandú",
        Source::Voice,
        Some(5.0),
    );
    let text = fs::read_to_string(path).expect("archivo legible");
    let record: Value = serde_json::from_str(text.trim()).expect("JSONL");
    assert_eq!(record["text"], "añade café ☕ y ñandú");
    assert_eq!(record["source"], "voice");
    assert!(!text.contains("\\u"));
}

#[test]
fn sessions_are_listed_newest_first_with_a_title() {
    let (_path, store) = temp_store("sessions");
    store.append(Role::Assistant, "bienvenida", Source::Chat, Some(1.0));
    store.append(
        Role::User,
        "resúmeme   el informe\ntrimestral de ventas por región y por canal de venta",
        Source::Chat,
        Some(2.0),
    );
    store.new_session();
    store.append(Role::User, "otra cosa", Source::Chat, Some(3.0));
    let sessions = store.sessions();
    assert_eq!(sessions[0].title, "otra cosa");
    assert_eq!(sessions[1].messages, 2);
    assert!(sessions[1]
        .title
        .starts_with("resúmeme el informe trimestral"));
    assert!(char_len(&sessions[1].title) <= 60);
    assert!(sessions[1].title.ends_with('…'));
}

#[test]
fn a_damaged_file_loads_what_it_can() {
    let dir = std::env::temp_dir().join(format!("ez-chat-test-damaged-{}", std::process::id()));
    let path = dir.join("chat.jsonl");
    fs::create_dir_all(&dir).expect("carpeta");
    let good =
        r#"{"v": 1, "t": 1.0, "session": 1, "role": "user", "source": "chat", "text": "ok"}"#;
    fs::write(
        &path,
        format!("{good}\nesto no es json\n{{\"session\": 1}}\n\n{good}\n"),
    )
    .expect("archivo");
    let store = ChatStore::new(Some(path));
    assert_eq!(
        store
            .entries(None)
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>(),
        vec!["ok", "ok"]
    );
}

#[test]
fn unknown_roles_and_sources_are_handled() {
    let dir = std::env::temp_dir().join(format!("ez-chat-test-roles-{}", std::process::id()));
    let path = dir.join("chat.jsonl");
    fs::create_dir_all(&dir).expect("carpeta");
    fs::write(
        &path,
        "{\"t\": 1, \"session\": 1, \"role\": \"hacker\", \"text\": \"x\"}\n\
         {\"t\": 2, \"session\": 1, \"role\": \"user\", \"source\": \"raro\", \"text\": \"y\"}",
    )
    .expect("archivo");
    let store = ChatStore::new(Some(path));
    let entries = store.entries(None);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (
            entries[0].role,
            entries[0].source.clone(),
            entries[0].text.as_str()
        ),
        (Role::User, Source::Chat, "y")
    );
}

#[test]
fn search_is_case_insensitive_and_newest_first() {
    let (_path, store) = temp_store("search");
    store.append(Role::User, "Presupuesto de marzo", Source::Chat, Some(1.0));
    store.append(
        Role::Assistant,
        "el PRESUPUESTO sube",
        Source::Chat,
        Some(2.0),
    );
    store.append(Role::User, "otra cosa", Source::Chat, Some(3.0));
    assert_eq!(
        store
            .search("presupuesto", 50)
            .iter()
            .map(|e| e.t)
            .collect::<Vec<_>>(),
        vec![2.0, 1.0]
    );
    assert!(store.search("", 50).is_empty());
    assert!(store.search("   ", 50).is_empty());
    assert_eq!(store.search("presupuesto", 1).len(), 1);
}

#[test]
fn replace_last_swaps_only_the_final_entry_of_that_role() {
    let (path, store) = temp_store("replace");
    store.append(Role::User, "una", Source::Chat, Some(1.0));
    store.append(Role::Assistant, "respuesta", Source::Chat, Some(2.0));
    store.append(Role::User, "frase cortada", Source::Voice, Some(3.0));
    assert!(!store.replace_last(Role::User, "una", "otra"));
    assert!(store.replace_last(Role::User, "frase cortada", "frase completa"));
    let again = ChatStore::new(Some(path));
    assert_eq!(
        again
            .entries(None)
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        vec!["una", "respuesta", "frase completa"]
    );
}

#[test]
fn as_messages_keeps_only_user_and_assistant() {
    let (_path, store) = temp_store("messages");
    store.append(Role::System, "nota", Source::Chat, None);
    store.append(Role::User, "a", Source::Chat, None);
    store.append(Role::Assistant, "b", Source::Chat, None);
    assert_eq!(
        as_messages(&store.entries(None)),
        vec![
            ModelMessage {
                role: Role::User,
                content: "a".into()
            },
            ModelMessage {
                role: Role::Assistant,
                content: "b".into()
            }
        ]
    );
}

#[test]
fn deleting_the_current_conversation_leaves_it_empty_but_current() {
    let (_path, store) = temp_store("delete");
    store.append(Role::User, "hola", Source::Chat, None);
    let current = store.session();
    assert!(store.delete_session(current));
    assert_eq!(store.session(), current);
    assert!(store.entries(None).is_empty());
    assert!(store.sessions().is_empty());
}

#[test]
fn resuming_an_old_conversation_makes_it_current() {
    let (path, store) = temp_store("resume");
    store.append(Role::User, "vieja", Source::Chat, None);
    let old = store.session();
    store.new_session();
    store.append(Role::User, "nueva", Source::Chat, None);
    assert!(store.resume(old));
    assert_eq!(ChatStore::new(Some(path)).session(), old);
    assert!(!store.resume(77));
}

#[test]
fn a_failing_disk_does_not_break_the_chat() {
    let dir = std::env::temp_dir().join(format!("ez-chat-test-disk-{}", std::process::id()));
    let blocker = dir.join("es-un-archivo");
    fs::create_dir_all(&dir).expect("carpeta");
    fs::write(&blocker, "x").expect("archivo");
    let store = ChatStore::new(Some(blocker.join("chat.jsonl")));
    let entry = store.append(Role::User, "sigue funcionando", Source::Chat, None);
    assert!(store.entries(None).contains(&entry));
}
