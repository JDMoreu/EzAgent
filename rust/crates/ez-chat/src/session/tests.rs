use std::sync::Arc;
use std::thread;

use super::*;

fn temp_store(name: &str) -> Arc<ChatStore> {
    let path = std::env::temp_dir()
        .join(format!("ez-chat-session-{name}-{}", std::process::id()))
        .join("chat.jsonl");
    Arc::new(ChatStore::new(Some(path)))
}

#[test]
fn a_streamed_answer_grows_and_is_saved_when_it_finishes() {
    let store = temp_store("stream");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("resume el informe", Source::Chat);
    let answer = model.begin_assistant(Source::Chat);
    assert!(model.busy());
    for piece in ["# Resumen\n\n", "Las ventas ", "subieron."] {
        model.append(answer.id, piece);
    }
    let (_, messages) = model.snapshot();
    assert!(messages.last().expect("mensaje").streaming);
    assert_eq!(store.entries(None).len(), 1);
    model.finish(answer.id);
    assert!(!model.busy());
    let entries = store.entries(None);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].text, "# Resumen\n\nLas ventas subieron.");
}

#[test]
fn a_failed_answer_shows_the_reason_but_is_not_saved() {
    let store = temp_store("fail");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("hola", Source::Chat);
    let reply = model.begin_assistant(Source::Chat);
    model.fail(reply.id, "No pude conectar con el proveedor.");
    let (_, messages) = model.snapshot();
    assert!(messages.last().expect("mensaje").error);
    assert!(!model.busy());
    assert_eq!(store.entries(None).len(), 1);
}

#[test]
fn cancelling_keeps_what_arrived_or_drops_an_empty_answer() {
    let store = temp_store("cancel");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("a", Source::Chat);
    let partial = model.begin_assistant(Source::Chat);
    model.append(partial.id, "  Respuesta a medias ");
    model.cancel(partial.id);
    let empty = model.begin_assistant(Source::Chat);
    model.cancel(empty.id);
    let (_, messages) = model.snapshot();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "Respuesta a medias"]
    );
    assert!(!model.busy());
}

#[test]
fn new_session_clears_the_screen_but_keeps_the_history() {
    let store = temp_store("new-session");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("primera", Source::Chat);
    let old = store.session();
    model.new_session();
    assert!(model.snapshot().1.is_empty());
    model.add_user("segunda", Source::Chat);
    assert_eq!(
        model
            .sessions()
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        vec!["segunda", "primera"]
    );
    model.view_session(Some(old));
    assert_eq!(model.viewing(), Some(old));
    assert_eq!(model.snapshot().1[0].text, "primera");
    model.view_session(None);
    assert_eq!(model.snapshot().1[0].text, "segunda");
}

#[test]
fn switching_loads_that_conversation_and_refuses_while_busy() {
    let store = temp_store("switch");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("tema A", Source::Chat);
    let a = store.session();
    model.new_session();
    model.add_user("tema B", Source::Chat);
    assert!(model.switch_session(a));
    assert_eq!(model.snapshot().1[0].text, "tema A");
    let reply = model.begin_assistant(Source::Chat);
    assert!(!model.switch_session(a));
    model.fail(reply.id, "x");
}

#[test]
fn notes_are_visible_but_never_saved() {
    let store = temp_store("notes");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    model.add_user("hola", Source::Chat);
    let note = model.add_note("Conversación compactada.");
    assert_eq!(note.role, Role::System);
    assert_eq!(note.source, Source::Agent);
    let (_, messages) = model.snapshot();
    assert_eq!(messages.len(), 2);
    assert_eq!(store.entries(None).len(), 1);
}

#[test]
fn revising_user_updates_one_box_and_one_history_entry() {
    let store = temp_store("revise");
    let model = ChatModel::new(Some(Arc::clone(&store)), false);
    let cut = model.add_user("frase cortada", Source::Voice);
    assert!(model.revise_user(cut.id, "frase completa"));
    assert_eq!(model.snapshot().1[0].text, "frase completa");
    assert_eq!(store.entries(None)[0].text, "frase completa");
    model.add_user("otra", Source::Voice);
    assert!(!model.revise_user(cut.id, "tarde"));
}

#[test]
fn version_changes_with_every_change_and_snapshots_are_copies() {
    let model = ChatModel::new(None, false);
    let start = model.version();
    let message = model.add_user("hola", Source::Chat);
    assert!(model.version() > start);
    let (version, snapshot) = model.snapshot();
    model.append(message.id, "!");
    assert_eq!(snapshot[0].text, "hola");
    assert!(model.version() > version);
}

#[test]
fn simple_switches_only_bump_when_they_change() {
    let model = ChatModel::new(None, false);
    let version = model.version();
    model.set_voice_replies(true);
    assert!(model.voice_replies() && model.version() > version);
    let version = model.version();
    model.set_voice_replies(true);
    assert_eq!(model.version(), version);
    let version = model.version();
    model.set_mic_muted(true);
    assert!(model.mic_muted() && model.version() > version);
}

#[test]
fn confirmations_are_shown_and_cleared() {
    let model = ChatModel::new(None, false);
    model.ask_confirm("Mover 12 archivos a la papelera", true);
    assert!(model.confirm().is_some_and(|card| card.strong));
    model.clear_confirm();
    assert!(model.confirm().is_none());
}

#[test]
fn the_chat_works_without_a_store() {
    let model = ChatModel::new(None, false);
    model.add_user("hola", Source::Chat);
    let reply = model.begin_assistant(Source::Chat);
    model.append(reply.id, "ok");
    model.finish(reply.id);
    assert!(model.sessions().is_empty());
    assert_eq!(
        model
            .snapshot()
            .1
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        vec!["hola", "ok"]
    );
    assert_eq!(model.last_assistant_text(), "ok");
}

#[test]
fn concurrent_appends_do_not_lose_text() {
    let model = Arc::new(ChatModel::new(None, false));
    let reply = model.begin_assistant(Source::Chat);
    let mut handles = Vec::new();
    for letter in ["a", "b", "c", "d"] {
        let model = Arc::clone(&model);
        handles.push(thread::spawn(move || {
            for _ in 0..200 {
                model.append(reply.id, letter);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("hilo");
    }
    model.finish(reply.id);
    assert_eq!(model.snapshot().1[0].text.chars().count(), 800);
}
