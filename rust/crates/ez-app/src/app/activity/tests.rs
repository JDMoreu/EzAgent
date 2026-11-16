//! Pruebas de tareas, cadenas y actividad (F6-07).

use std::sync::atomic::Ordering;

use ez_core::config::VoiceConfig;
use ez_core::pytime::timestamp;
use ez_herdr::activity::{chain_key, snapshot};
use serde_json::{json, Map, Value};

use crate::app::core::{AppEvent, VoiceApp};
use crate::app::states::tests::support::{drain_events, Fixture};

fn setup() -> Fixture {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    Fixture::with_config(cfg)
}

fn chain(source: &str, agent: &str) -> Map<String, Value> {
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!(source));
    spec.insert("agent".to_string(), json!(agent));
    spec.insert("task".to_string(), json!("revisar"));
    spec
}

fn item_keys(app: &VoiceApp) -> Vec<String> {
    let log = super::log::activity_log(app);
    snapshot(app, Some(&log))
        .into_iter()
        .map(|item| item.key)
        .collect()
}

#[test]
fn remember_task_writes_the_registry() {
    let fixture = setup();
    fixture.app.remember_task("t1", "claude", "arreglar el bug");
    let row = fixture
        .app
        .inner
        .agents
        .tasks
        .get("t1")
        .expect("tarea guardada");
    assert_eq!(row.get("agent"), Some(&json!("claude")));
    assert_eq!(row.get("task"), Some(&json!("arreglar el bug")));
}

#[test]
fn stage_label_prefers_the_name_and_falls_back_to_the_agent() {
    let fixture = setup();
    let mut spec = Map::new();
    spec.insert("name".to_string(), json!("Revisión"));
    spec.insert("agent".to_string(), json!("claude"));
    assert_eq!(fixture.app.stage_label(&spec), "Revisión");
    spec.remove("name");
    assert_eq!(fixture.app.stage_label(&spec), "Claude Code");
    spec.insert("agent".to_string(), json!("raro"));
    assert_eq!(fixture.app.stage_label(&spec), "raro");
}

#[test]
fn stage_notice_only_speaks_with_two_stages() {
    let fixture = setup();
    fixture.app.stage_notice(&["A".to_string()]);
    assert!(fixture.speaker.spoken.lock().is_empty());
    fixture
        .app
        .stage_notice(&["A".to_string(), "B".to_string()]);
    crate::app::states::tests::support::wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::SpeakDone(_))
    });
    assert_eq!(
        *fixture.speaker.spoken.lock(),
        vec!["Terminó «A». Ahora empieza «B».".to_string()]
    );
}

#[test]
fn enqueue_chain_saves_and_watches_the_source() {
    let fixture = setup();
    fixture.app.enqueue_chain(&chain("t1", "codex"));
    let chains = fixture.app.inner.agents.tasks.chains_for("t1");
    assert_eq!(chains.len(), 1);
    let id = chains[0]
        .get("chain_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert_eq!(id.chars().count(), 32);
    assert!(fixture.app.inner.agents.watched.lock().contains("t1"));
}

#[test]
fn enqueue_chain_requires_source_and_agent() {
    let fixture = setup();
    let mut spec = Map::new();
    spec.insert("agent".to_string(), json!("codex"));
    spec.insert("task".to_string(), json!("revisar"));
    fixture.app.enqueue_chain(&spec);
    assert!(fixture.app.inner.agents.tasks.chains().is_empty());
    assert!(fixture.app.inner.agents.watched.lock().is_empty());
}

#[test]
fn launch_chains_without_pending_returns_zero() {
    let fixture = setup();
    assert_eq!(fixture.app.launch_chains("t1"), 0);
}

#[test]
fn watch_remembers_the_dictated_task_and_clears_old_activity() {
    let fixture = setup();
    if let Some(messenger) = fixture.app.inner.agents.messenger.lock().as_ref() {
        messenger.import_context(Vec::new(), "", "texto dictado");
    }
    super::log::activity_log(&fixture.app).record("t1", "", "viejo", "done");
    fixture.app.watch("t1");
    assert!(fixture.app.inner.agents.watched.lock().contains("t1"));
    assert_eq!(
        fixture
            .app
            .inner
            .agents
            .tasks
            .get("t1")
            .and_then(|row| row.get("task").cloned()),
        Some(json!("texto dictado"))
    );
    assert!(super::log::activity_log(&fixture.app).items().is_empty());
}

#[test]
fn mute_extends_the_quiet_window() {
    let fixture = setup();
    let before = timestamp();
    fixture.app.mute(5);
    assert!(*fixture.app.inner.session.mute_until.lock() >= before + 299.0);
    fixture.app.mute(-3);
    assert!(*fixture.app.inner.session.mute_until.lock() >= before + 59.0);
}

#[test]
fn news_are_unseen_until_seen_and_can_be_dismissed() {
    let fixture = setup();
    fixture
        .app
        .news_arrived("t1", "claude", "Claude Code", "done");
    let log = super::log::activity_log(&fixture.app);
    assert!(log.items().iter().any(|item| item.unseen));
    fixture.app.news_seen();
    assert!(log.items().iter().all(|item| !item.unseen));
    fixture.app.dismiss_activity("t1");
    assert!(log.items().is_empty());
}

#[test]
fn dismissing_a_watched_task_removes_it_and_its_chains() {
    let fixture = setup();
    fixture
        .app
        .inner
        .agents
        .tasks
        .add("t1", "claude", "hecho", None)
        .expect("tarea");
    fixture
        .app
        .inner
        .agents
        .tasks
        .add_chain(&chain("t1", "codex"))
        .expect("cadena");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture.app.dismiss_activity("t1");
    assert!(fixture.app.inner.agents.tasks.get("t1").is_none());
    assert!(fixture.app.inner.agents.tasks.chains_for("t1").is_empty());
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
}

#[test]
fn dismissing_a_waiting_chain_removes_only_that_chain() {
    let fixture = setup();
    fixture
        .app
        .inner
        .agents
        .tasks
        .add_chain(&chain("t1", "codex"))
        .expect("cadena");
    let spec = fixture.app.inner.agents.tasks.chains().remove(0);
    let key = chain_key(&spec);
    fixture.app.dismiss_activity(&key);
    assert!(fixture.app.inner.agents.tasks.chains().is_empty());
}

#[test]
fn open_activity_skips_chain_and_swarm_keys() {
    let fixture = setup();
    let before = fixture.app.inner.runtime.workers.lock().len();
    fixture.app.open_activity("");
    fixture.app.open_activity("chain:t1:codex:0");
    fixture.app.open_activity("swarm:abc");
    assert_eq!(fixture.app.inner.runtime.workers.lock().len(), before);
}

#[test]
fn the_activity_source_reports_watched_tasks_and_waiting_chains() {
    let fixture = setup();
    fixture
        .app
        .inner
        .agents
        .tasks
        .add("t1", "claude", "revisar", None)
        .expect("tarea");
    fixture
        .app
        .inner
        .agents
        .watched
        .lock()
        .insert("t1".to_string());
    fixture
        .app
        .inner
        .agents
        .tasks
        .add_chain(&chain("t2", "codex"))
        .expect("cadena");
    let keys = item_keys(&fixture.app);
    assert!(keys.contains(&"t1".to_string()));
    assert!(keys.iter().any(|key| key.starts_with("chain:t2:")));
}

#[test]
fn activity_tick_is_safe_after_shutdown() {
    let fixture = setup();
    fixture
        .app
        .news_arrived("t1", "claude", "Claude Code", "done");
    fixture
        .app
        .inner
        .session
        .shutdown_done
        .store(true, Ordering::SeqCst);
    fixture.app.activity_tick();
    assert!(fixture
        .app
        .inner
        .session
        .shutdown_done
        .load(Ordering::SeqCst));
}

#[test]
fn a_saved_task_without_a_terminal_is_forgotten() {
    let fixture = setup();
    let app = &fixture.app;
    app.inner
        .agents
        .tasks
        .add("t41", "", "una tarea de hace días", None)
        .expect("tarea");
    app.inner.agents.watched.lock().insert("t41".to_string());
    app.forget_gone("t41");
    assert!(app.inner.agents.tasks.get("t41").is_none());
    assert!(!app.inner.agents.watched.lock().contains("t41"));
    // No se avisa de nada: no terminó.
    assert!(app.events().is_empty());
}

#[test]
fn take_prelaunched_returns_empty_for_an_unknown_chain() {
    let fixture = setup();
    assert_eq!(fixture.app.take_prelaunched(&chain("t1", "codex")), "");
}

#[test]
fn start_prelaunch_skips_background_and_gui_targets() {
    let fixture = setup();
    let mut spec = chain("t1", "codex");
    spec.insert("chain_id".to_string(), json!("abc"));
    spec.insert("background".to_string(), json!(true));
    fixture.app.start_prelaunch(&spec);
    assert!(fixture.app.inner.agents.prelaunch.lock().is_empty());
    spec.remove("background");
    spec.insert("gui".to_string(), json!(true));
    fixture.app.start_prelaunch(&spec);
    assert!(fixture.app.inner.agents.prelaunch.lock().is_empty());
}

#[test]
fn watcher_events_from_a_new_task_are_not_announced_twice() {
    let fixture = setup();
    fixture.app.remember_task("t1", "claude", "algo");
    assert!(drain_events(&fixture.app).is_empty());
    assert!(fixture.app.inner.agents.tasks.get("t1").is_some());
}
