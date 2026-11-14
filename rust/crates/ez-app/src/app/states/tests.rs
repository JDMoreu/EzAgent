//! Pruebas de estados, voz, confirmación y avisos (F6-04).

pub(crate) mod support;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_core::metrics::TurnTimings;
use serde_json::{json, Value};

use super::super::core::state::Flag;
use super::super::core::{AppEvent, VoiceApp, IDLE, LISTENING, SPEAKING, THINKING};
use super::{confirmation_hint, round1};
use support::{wait_event, Fixture};

fn hidden_config() -> ez_core::config::VoiceConfig {
    let mut cfg = ez_core::config::VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(true));
    cfg
}

#[test]
fn listening_clears_the_echo_guards_and_shows_the_bar() {
    let fixture = Fixture::with_config(hidden_config());
    let app = &fixture.app;
    app.inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    app.inner.turn.post_tts_ignore.store(true, Ordering::SeqCst);
    app.set_state(LISTENING);
    assert_eq!(app.state(), LISTENING);
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(!app.inner.turn.post_tts_ignore.load(Ordering::SeqCst));
    assert!(!app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert!(!app
        .config()
        .get("hud_hidden")
        .is_some_and(|v| v == &json!(true)));
    assert!(fixture.saved_config().contains("hud_hidden"));
}

#[test]
fn idle_keeps_a_hidden_bar_hidden() {
    let fixture = Fixture::with_config(hidden_config());
    let app = &fixture.app;
    app.set_state(IDLE);
    assert_eq!(app.state(), IDLE);
    assert!(app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert!(fixture.saved_config().is_empty());
}

#[test]
fn set_idle_is_set_state_idle() {
    let fixture = Fixture::new();
    fixture.app.set_state(THINKING);
    fixture.app.set_idle();
    assert_eq!(fixture.app.state(), IDLE);
}

#[test]
fn bump_turn_increments_the_generation() {
    let fixture = Fixture::new();
    assert_eq!(fixture.app.bump_turn(), 1);
    assert_eq!(fixture.app.bump_turn(), 2);
}

#[test]
fn new_tts_stop_cancels_the_previous_ones() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let old = Arc::new(Flag::new());
    let busy = Arc::new(Flag::new());
    *app.inner.speech.tts_stop.lock() = Some(Arc::clone(&old));
    *app.inner.speech.busy_stop.lock() = Some(Arc::clone(&busy));
    let fresh = app.new_tts_stop();
    assert!(old.is_set());
    assert!(busy.is_set());
    assert!(!fresh.is_set());
    let stored = app.inner.speech.tts_stop.lock().clone();
    assert!(stored.is_some_and(|stop| Arc::ptr_eq(&stop, &fresh)));
}

#[test]
fn speak_advances_the_turn_and_announces_done() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.speak("Hola.", "announce");
    assert_eq!(app.state(), SPEAKING);
    assert_eq!(
        app.inner.speech.speaking_kind.read().as_deref(),
        Some("announce")
    );
    assert_eq!(app.inner.turn.turn_gen.load(Ordering::SeqCst), 1);
    let events = wait_event(app, |event| matches!(event, AppEvent::SpeakDone(1)));
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::SpeakDone(1))));
    assert_eq!(*fixture.speaker.spoken.lock(), ["Hola."]);
}

#[test]
fn speak_same_turn_keeps_the_generation() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.bump_turn();
    app.speak_same_turn("¿Confirmas?", "confirm");
    assert_eq!(app.inner.turn.turn_gen.load(Ordering::SeqCst), 1);
    assert_eq!(
        app.inner.speech.speaking_kind.read().as_deref(),
        Some("confirm")
    );
    wait_event(app, |event| matches!(event, AppEvent::SpeakDone(1)));
    assert_eq!(*fixture.speaker.spoken.lock(), ["¿Confirmas?"]);
}

#[test]
fn speak_ignores_empty_text() {
    let fixture = Fixture::new();
    fixture.app.speak("", "announce");
    assert_eq!(fixture.app.state(), IDLE);
    assert!(fixture.speaker.spoken.lock().is_empty());
}

#[test]
fn announcement_reopens_the_call_and_speaks() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.queue_announcement("Terminó el agente.", true);
    assert!(app.inner.session.in_call.load(Ordering::SeqCst));
    assert!(app.inner.agents.announce_queue.lock().is_empty());
    assert_eq!(app.state(), SPEAKING);
    assert_eq!(
        app.inner.speech.speaking_kind.read().as_deref(),
        Some("announce")
    );
    wait_event(app, |event| matches!(event, AppEvent::SpeakDone(2)));
    assert_eq!(*fixture.speaker.spoken.lock(), ["Terminó el agente."]);
}

#[test]
fn announcement_waits_while_the_user_speaks_or_thinks() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner
        .session
        .user_speaking
        .store(true, Ordering::SeqCst);
    app.queue_announcement("Aviso.", false);
    assert_eq!(app.inner.agents.announce_queue.lock().len(), 1);
    app.inner
        .session
        .user_speaking
        .store(false, Ordering::SeqCst);
    app.set_state(THINKING);
    app.pump_announcements();
    assert_eq!(app.inner.agents.announce_queue.lock().len(), 1);
    app.set_state(IDLE);
    app.pump_announcements();
    assert!(app.inner.agents.announce_queue.lock().is_empty());
    wait_event(app, |event| matches!(event, AppEvent::SpeakDone(1)));
    assert_eq!(*fixture.speaker.spoken.lock(), ["Aviso."]);
}

#[test]
fn announcement_without_reopen_does_not_start_a_call() {
    let fixture = Fixture::new();
    fixture.app.queue_announcement("Solo aviso.", false);
    assert!(!fixture.app.inner.session.in_call.load(Ordering::SeqCst));
    wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::SpeakDone(1))
    });
    assert_eq!(*fixture.speaker.spoken.lock(), ["Solo aviso."]);
}

#[test]
fn voice_text_merges_the_continued_phrase() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.show_voice_text("Hola");
    let first = app
        .inner
        .turn
        .last_voice_user_id
        .lock()
        .expect("caja de voz");
    *app.inner.turn.merge_user_id.lock() = Some(first);
    app.show_voice_text("Hola mundo");
    let users: Vec<String> = app
        .inner
        .chat
        .chat
        .snapshot()
        .1
        .into_iter()
        .filter(|message| message.role == ez_chat::store::Role::User)
        .map(|message| message.text)
        .collect();
    assert_eq!(users, ["Hola mundo"]);
    assert_eq!(*app.inner.turn.last_voice_user_id.lock(), Some(first));
}

#[test]
fn confirmation_hint_matches_the_python() {
    assert_eq!(confirmation_hint(false), " ¿Confirmas?");
    assert_eq!(
        confirmation_hint(true),
        " Di «confirmo» y el número para aceptar."
    );
}

#[test]
fn show_confirmation_without_a_pending_agent_does_nothing() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.show_confirmation("Aceptar.", true);
    assert!(app.inner.chat.chat.confirm().is_none());
    assert_eq!(app.state(), IDLE);
    assert!(fixture.speaker.spoken.lock().is_empty());
}

#[test]
fn latency_meta_carries_the_allowed_fields() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let timing = TurnTimings::new(0.0, 0.05);
    let meta = app.latency_meta(&timing, "cancelled");
    assert_eq!(meta["provider"], json!("openai"));
    assert_eq!(meta["model"], json!("gpt-prueba"));
    assert_eq!(meta["rounds"], json!(0));
    assert_eq!(meta["outcome"], json!("cancelled"));
    assert!(meta.get("first_ack_ms").is_none());
    timing.mark("first_ack", Some(0.15));
    let meta = app.latency_meta(&timing, "ok");
    assert_eq!(meta["first_ack_ms"], json!(100.0));
}

#[test]
fn round1_rounds_half_to_even_like_python() {
    assert_eq!(round1(2.25), 2.2);
    assert_eq!(round1(2.75), 2.8);
    assert_eq!(round1(12.34), 12.3);
}

#[test]
fn state_is_shared_by_every_clone() {
    let fixture = Fixture::new();
    let clone = fixture.app.clone();
    clone.set_state(THINKING);
    assert_eq!(VoiceApp::state(&fixture.app), THINKING);
}
