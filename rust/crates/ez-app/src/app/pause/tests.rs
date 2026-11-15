//! Pruebas de pausa, silencio del micrófono y reactivación (F6-07).

use std::sync::atomic::Ordering;

use ez_core::config::VoiceConfig;
use serde_json::{json, Value};

use crate::app::core::{AppEvent, LISTENING};
use crate::app::states::tests::support::{drain_events, stt_server, wait_event, Fixture};

fn setup() -> Fixture {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    Fixture::with_config(cfg)
}

#[test]
fn request_close_marks_the_turn() {
    let fixture = setup();
    assert!(!fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
    fixture.app.request_close();
    assert!(fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
}

#[test]
fn close_action_only_accepts_pause_and_hangup() {
    let fixture = setup();
    for (configured, expected) in [
        ("", "pause"),
        ("pause", "pause"),
        ("HANGUP", "hangup"),
        ("otra", "pause"),
    ] {
        fixture
            .app
            .inner
            .config
            .cfg
            .write()
            .set("close_action", json!(configured));
        assert_eq!(fixture.app.close_action(), expected, "{configured}");
    }
}

#[test]
fn pause_message_prefers_the_wake_phrase() {
    let fixture = setup();
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("wake_phrase", json!("hola EzAgent"));
    assert_eq!(
        fixture.app.pause_message(),
        "En pausa. Di «hola EzAgent» cuando me necesites."
    );
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("wake_phrase", json!(""));
    assert_eq!(
        fixture.app.pause_message(),
        "En pausa. Di «EzAgent, escúchame» cuando me necesites."
    );
}

#[test]
fn pause_message_without_wake_talks_about_the_microphone() {
    let fixture = setup();
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("voice_wake", json!(false));
    assert_eq!(
        fixture.app.pause_message(),
        "En pausa. Toca el micrófono para volver."
    );
}

#[test]
fn close_listening_in_pause_keeps_the_call_muted() {
    let fixture = setup();
    fixture
        .app
        .inner
        .session
        .in_call
        .store(true, Ordering::SeqCst);
    *fixture.app.inner.turn.close_mode.write() = "pause".to_string();
    assert!(fixture.app.close_listening());
    assert!(fixture.app.inner.session.in_call.load(Ordering::SeqCst));
    assert!(fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert_eq!(fixture.app.state(), LISTENING);
    assert!(fixture.app.inner.turn.close_mode.read().is_empty());
}

#[test]
fn close_listening_with_hangup_ends_the_call() {
    let fixture = setup();
    fixture
        .app
        .inner
        .session
        .in_call
        .store(true, Ordering::SeqCst);
    *fixture.app.inner.turn.close_mode.write() = "hangup".to_string();
    assert!(!fixture.app.close_listening());
    assert!(!fixture.app.inner.session.in_call.load(Ordering::SeqCst));
}

#[test]
fn close_listening_without_a_call_hangs_up() {
    let fixture = setup();
    *fixture.app.inner.turn.close_mode.write() = "pause".to_string();
    assert!(!fixture.app.close_listening());
    assert!(!fixture.app.inner.session.in_call.load(Ordering::SeqCst));
}

#[test]
fn llm_mic_mute_requests_pause_only_when_muting() {
    let fixture = setup();
    fixture.app.llm_mic_mute(false);
    assert!(drain_events(&fixture.app).is_empty());
    fixture.app.llm_mic_mute(true);
    assert_eq!(
        drain_events(&fixture.app),
        vec![AppEvent::CloseRequest("pause".to_string())]
    );
}

#[test]
fn set_mic_muted_updates_state_and_the_capture_reset() {
    let fixture = setup();
    fixture.app.set_mic_muted(true, "prueba");
    assert!(fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert!(fixture
        .app
        .inner
        .session
        .ignore_utterance
        .load(Ordering::SeqCst));
    assert!(fixture.app.inner.audio.reset_capture.is_set());
    fixture.app.set_mic_muted(false, "");
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));
    assert!(!fixture
        .app
        .inner
        .session
        .ignore_utterance
        .load(Ordering::SeqCst));
}

#[test]
fn on_wake_needs_a_muted_call() {
    let fixture = setup();
    fixture.app.on_wake();
    assert!(drain_events(&fixture.app).is_empty());
    fixture
        .app
        .inner
        .session
        .in_call
        .store(true, Ordering::SeqCst);
    fixture.app.on_wake();
    assert!(drain_events(&fixture.app).is_empty());
    fixture.app.set_mic_muted(true, "prueba");
    fixture.app.on_wake();
    assert!(!fixture.app.inner.session.muted.load(Ordering::SeqCst));
    let events = wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::SpeakDone(_))
    });
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::SpeakDone(_))));
    assert_eq!(*fixture.speaker.spoken.lock(), vec!["Dime.".to_string()]);
}

#[test]
fn maybe_wake_ignores_a_disabled_switch() {
    let fixture = setup();
    fixture
        .app
        .inner
        .session
        .in_call
        .store(true, Ordering::SeqCst);
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("voice_wake", json!(false));
    let before = fixture.app.inner.runtime.workers.lock().len();
    fixture.app.maybe_wake(&[0u8; 32_000], 16_000);
    assert_eq!(fixture.app.inner.runtime.workers.lock().len(), before);
}

#[test]
fn maybe_wake_ignores_a_call_that_is_not_paused() {
    let fixture = setup();
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("voice_wake", json!(true));
    let before = fixture.app.inner.runtime.workers.lock().len();
    fixture.app.maybe_wake(&[0u8; 32_000], 16_000);
    assert_eq!(fixture.app.inner.runtime.workers.lock().len(), before);
}

#[test]
fn wake_worker_emits_wake_for_the_phrase() {
    let fixture = setup();
    let (url, server) = stt_server("EzAgent, escúchame");
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("whisper_url", json!(url));
    fixture.app.wake_worker(&[0u8; 32_000], 16_000);
    let events = drain_events(&fixture.app);
    assert!(events.contains(&AppEvent::Wake));
    server.join().expect("servidor de whisper");
}

#[test]
fn wake_worker_stays_quiet_for_other_text() {
    let fixture = setup();
    let (url, server) = stt_server("una frase cualquiera");
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("whisper_url", json!(url));
    fixture.app.wake_worker(&[0u8; 32_000], 16_000);
    assert!(drain_events(&fixture.app).is_empty());
    server.join().expect("servidor de whisper");
}
