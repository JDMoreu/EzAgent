//! Pruebas del vigilante, sus avisos y los traspasos (F6-05).

use ez_core::config::VoiceConfig;
use ez_core::pytime::timestamp;
use serde_json::{json, Map, Value};

use crate::app::core::{AppEvent, THINKING};
use crate::app::states::tests::support::{drain_events, Fixture};

fn setup() -> Fixture {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(false));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    // Ni la app ni el vigilante deben lanzar un `hc` real en las pruebas
    // (un nombre «hc.exe» ausente haría que Herdr intentara instalarlo).
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    Fixture::with_config(cfg)
}

fn prompt(events: Vec<AppEvent>) -> (String, String, String, String, String) {
    let event = events
        .into_iter()
        .find(|event| matches!(event, AppEvent::DonePrompt { .. }))
        .expect("done_prompt");
    match event {
        AppEvent::DonePrompt {
            address,
            agent,
            reason,
            output,
            task,
        } => (address, agent, reason, output, task),
        _ => unreachable!(),
    }
}

#[test]
fn should_announce_respects_mute_watched_and_announce_all() {
    let fixture = setup();
    let app = &fixture.app;
    assert!(!app.should_announce("t1"));
    app.inner.agents.watched.lock().insert("t1".to_string());
    assert!(app.should_announce("t1"));
    app.inner
        .config
        .cfg
        .write()
        .set("announce_all", json!(true));
    assert!(app.should_announce("otro"));
    *app.inner.session.mute_until.lock() = timestamp() + 60.0;
    assert!(!app.should_announce("t1"));
}

#[test]
fn watcher_events_for_unwatched_addresses_are_ignored() {
    let fixture = setup();
    fixture.app.on_watcher_event("t9", "claude", "done", "done");
    assert!(drain_events(&fixture.app).is_empty());
}

#[test]
fn a_lost_terminal_asks_before_claiming_success() {
    let fixture = setup();
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture.app.on_watcher_event("t1", "claude", "lost", "lost");
    let (address, agent, reason, output, task) = prompt(drain_events(&fixture.app));
    assert_eq!(address, "t1");
    assert_eq!(agent, "claude");
    assert_eq!(reason, "lost");
    assert_eq!(output, "");
    assert_eq!(
        task,
        "No pude confirmar que el agente terminara: su terminal desapareció."
    );
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
}

#[test]
fn a_lost_terminal_keeps_the_saved_task() {
    let fixture = setup();
    fixture
        .app
        .inner
        .agents
        .tasks
        .add("t1", "claude", "arreglar el bug", None)
        .expect("tarea");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture.app.on_watcher_event("t1", "claude", "lost", "lost");
    let (_, _, reason, _, task) = prompt(drain_events(&fixture.app));
    assert_eq!(reason, "lost");
    assert_eq!(task, "arreglar el bug");
}

#[test]
fn a_blocked_source_keeps_its_chains_and_watch() {
    let fixture = setup();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks.add("t1", "claude", "", None).expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t1"));
    spec.insert("agent".to_string(), json!("codex"));
    spec.insert("task".to_string(), json!("revisar"));
    tasks.add_chain(&spec).expect("cadena");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture
        .app
        .on_watcher_event("t1", "claude", "blocked", "blocked");
    let (_, _, reason, output, task) = prompt(drain_events(&fixture.app));
    assert_eq!(reason, "blocked");
    assert_eq!(output, "");
    assert_eq!(
        task,
        "La cadena sigue pendiente: el agente de origen está bloqueado."
    );
    assert!(fixture.app.inner.agents.watched.lock().contains("t1"));
    assert_eq!(tasks.chains_for("t1").len(), 1);
    assert!(tasks.get("t1").is_some());
}

#[test]
fn a_done_agent_asks_and_forgets_the_task() {
    let fixture = setup();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks.add("t1", "claude", "hecho", None).expect("tarea");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture.app.on_watcher_event("t1", "claude", "done", "done");
    let (address, agent, reason, output, task) = prompt(drain_events(&fixture.app));
    assert_eq!((address.as_str(), agent.as_str()), ("t1", "claude"));
    assert_eq!(reason, "done");
    assert_eq!(output, "");
    assert_eq!(task, "hecho");
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
    assert!(tasks.get("t1").is_none());
}

#[test]
fn a_failed_source_drops_its_chains_and_explains_why() {
    let fixture = setup();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks.add("t1", "claude", "", None).expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t1"));
    spec.insert("agent".to_string(), json!("codex"));
    tasks.add_chain(&spec).expect("cadena");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture
        .app
        .on_watcher_event("t1", "claude", "error", "error");
    let (_, _, reason, _, task) = prompt(drain_events(&fixture.app));
    assert_eq!(reason, "error");
    assert_eq!(
        task,
        "La cadena no se lanzó porque el agente de origen falló."
    );
    assert!(tasks.chains_for("t1").is_empty());
    assert!(tasks.get("t1").is_none());
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
}

#[test]
fn deliver_and_sent_maps_keep_their_entries() {
    let fixture = setup();
    let app = &fixture.app;
    app.deliver_answer("t1");
    assert!(app.deliver_set().lock().contains("t1"));
    app.sent_map()
        .lock()
        .insert("t1".to_string(), "hola".to_string());
    assert_eq!(app.sent_map().lock().get("t1"), Some(&"hola".to_string()));
    app.prelaunch_state()
        .lock()
        .insert("c1".to_string(), Map::new());
    assert!(app.prelaunch_state().lock().contains_key("c1"));
}

#[test]
fn resume_pending_tasks_watches_tasks_and_chain_sources() {
    let fixture = setup();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks.add("t1", "claude", "uno", None).expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t2"));
    spec.insert("agent".to_string(), json!("codex"));
    tasks.add_chain(&spec).expect("cadena");
    fixture.app.resume_pending_tasks();
    let watched = fixture.app.inner.agents.watched.lock();
    assert!(watched.contains("t1"));
    assert!(watched.contains("t2"));
}

#[test]
fn agent_label_prefers_the_profile_and_falls_back_to_the_name() {
    let fixture = setup();
    {
        let mut cfg = fixture.app.inner.config.cfg.write();
        cfg.set("agents", json!({"claude": ["claude"]}));
        cfg.set(
            "agent_profiles",
            json!({"claude": {"label": "Claude Code"}}),
        );
    }
    assert_eq!(fixture.app.agent_label("claude"), "Claude Code");
    assert_eq!(fixture.app.agent_label("desconocido"), "desconocido");
    assert_eq!(fixture.app.agent_label(""), "El agente");
}

#[test]
fn visible_agent_uses_the_agent_type_without_a_herdr_tab() {
    let fixture = setup();
    assert_eq!(fixture.app.visible_agent("t1", "claude"), "Claude Code");
}

#[test]
fn done_question_names_the_tab_and_keeps_flow_names() {
    let fixture = setup();
    let app = &fixture.app;
    assert_eq!(
        app.done_question("t1", "claude", "done", ""),
        "Oye, Claude Code terminó. ¿Quieres que te resuma qué hizo, \
         que te lea la salida o que te la muestre?"
    );
    assert_eq!(
        app.done_question("t1", "claude", "done", "todo el flujo: A → B"),
        "Oye, terminó todo el flujo: A → B. ¿Quieres que te lo resuma, \
         que te muestre el resultado o que haga algo más con él?"
    );
    assert_eq!(
        app.done_question("t1", "claude", "lost", ""),
        "Claude Code se cerró antes de que pudiera confirmar que terminó. \
         ¿Quieres que lo revise?"
    );
    assert_eq!(
        app.done_question("t1", "claude", "blocked", ""),
        "Claude Code necesita tu atención. ¿Quieres que te diga qué pasa o le das una instrucción?"
    );
}

#[test]
fn packet_for_passes_a_short_answer_untouched() {
    let fixture = setup();
    let packet = fixture.app.packet_for("  hecho  ");
    assert_eq!(packet.kind, "short");
    assert_eq!(packet.text, "hecho");
    assert_eq!(packet.spoken, "hecho");
}

#[test]
fn reading_a_missing_terminal_is_an_empty_answer() {
    let fixture = setup();
    assert!(fixture.app.read_full("t1").is_err());
    assert_eq!(fixture.app.read_answer("t1", "tarea"), "");
}

#[test]
fn done_answer_announces_the_composed_text() {
    let fixture = setup();
    *fixture.app.inner.session.state.write() = THINKING.to_string();
    fixture.app.handle_done_answer("t1", "claude", "hecho", "");
    let queue = fixture.app.inner.agents.announce_queue.lock();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].0, "Claude Code respondió: hecho");
    assert!(queue[0].1);
}

#[test]
fn a_long_answer_says_that_the_rest_is_in_the_terminal() {
    let fixture = setup();
    *fixture.app.inner.session.state.write() = THINKING.to_string();
    let answer = "a".repeat(400);
    fixture.app.handle_done_answer("t1", "claude", &answer, "");
    let queue = fixture.app.inner.agents.announce_queue.lock();
    let text = &queue[0].0;
    assert!(text.starts_with("Claude Code respondió: "));
    assert!(text.ends_with(" El resto está en su terminal."));
    assert!(text.contains('…'));
}

#[test]
fn done_prompt_queues_the_question() {
    let fixture = setup();
    *fixture.app.inner.session.state.write() = THINKING.to_string();
    fixture
        .app
        .handle_done_prompt("t1", "claude", "done", "", "todo el flujo: A → B");
    let queue = fixture.app.inner.agents.announce_queue.lock();
    assert_eq!(
        queue[0].0,
        "Oye, terminó todo el flujo: A → B. ¿Quieres que te lo resuma, \
         que te muestre el resultado o que haga algo más con él?"
    );
}
