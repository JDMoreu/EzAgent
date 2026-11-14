//! Pruebas de la llamada y del barge-in (F6-03).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_core::metrics::monotonic;
use serde_json::json;

use crate::app::core::state::Flag;
use crate::app::core::{AppEvent, IDLE, LISTENING, SPEAKING, THINKING};
use crate::app::events::support::Fixture;
use crate::app::events::{in_call, turn_gen};

fn set_state(app: &crate::app::core::VoiceApp, state: &str) {
    *app.inner.session.state.write() = state.to_string();
}

#[test]
fn start_call_sets_the_call_state_and_advances_the_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    assert!(in_call(app));
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.turn.call_stop.lock().is_some());
    assert!(!app.inner.session.user_speaking.load(Ordering::SeqCst));
    // En los dobles no se abre el micrófono ni se crea el hilo de captura.
    assert!(app.inner.turn.call_thread.lock().is_none());
    assert!(fixture.mic.lock().device_index.is_none());
    app.start_call();
    assert_eq!(turn_gen(app), 1);
}

#[test]
fn hang_up_stops_capture_and_advances_the_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    *app.inner.speech.speaking_kind.write() = Some("answer".to_string());
    app.hang_up();
    assert!(!in_call(app));
    assert_eq!(turn_gen(app), 2);
    assert!(app
        .inner
        .turn
        .call_stop
        .lock()
        .as_ref()
        .is_some_and(|stop| stop.is_set()));
    assert!(app.inner.speech.speaking_kind.read().is_none());
    assert!(!app.inner.session.user_speaking.load(Ordering::SeqCst));
    app.hang_up();
    assert_eq!(turn_gen(app), 2);
}

#[test]
fn hang_up_lets_a_running_announcement_finish() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    *app.inner.speech.speaking_kind.write() = Some("announce".to_string());
    app.hang_up();
    assert!(!in_call(app));
    assert_eq!(turn_gen(app), generation);
    assert_eq!(
        app.inner.speech.speaking_kind.read().as_deref(),
        Some("announce")
    );
}

#[test]
fn barge_in_requires_a_call_a_speaking_state_and_the_setting() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner.config.cfg.write().set("barge_in", json!(true));
    set_state(app, SPEAKING);
    app.on_barge_in(0.5);
    assert_eq!(turn_gen(app), 0);
    assert!(!app.inner.session.user_speaking.load(Ordering::SeqCst));
    app.start_call();
    app.inner.session.muted.store(true, Ordering::SeqCst);
    app.on_barge_in(0.5);
    assert_eq!(turn_gen(app), 1);
    app.inner.session.muted.store(false, Ordering::SeqCst);
    app.inner.config.cfg.write().set("barge_in", json!(false));
    app.on_barge_in(0.5);
    assert_eq!(turn_gen(app), 1);
}

#[test]
fn barge_in_cuts_tts_and_invalidates_the_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, SPEAKING);
    app.inner.config.cfg.write().set("barge_in", json!(true));
    app.inner
        .turn
        .close_after_answer
        .store(true, Ordering::SeqCst);
    *app.inner.turn.close_mode.write() = "hangup".to_string();
    app.on_barge_in(0.4);
    assert_eq!(turn_gen(app), 2);
    assert!(app.inner.session.user_speaking.load(Ordering::SeqCst));
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(!app.inner.turn.close_after_answer.load(Ordering::SeqCst));
    assert!(app.inner.turn.close_mode.read().is_empty());
}

#[test]
fn barge_in_during_the_busy_notice_keeps_the_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    set_state(app, THINKING);
    app.inner.config.cfg.write().set("barge_in", json!(true));
    app.inner.speech.busy_speaking.set();
    let busy_stop = Arc::new(Flag::new());
    *app.inner.speech.busy_stop.lock() = Some(Arc::clone(&busy_stop));
    app.on_barge_in(0.4);
    assert_eq!(turn_gen(app), generation);
    assert!(!app.inner.speech.busy_speaking.is_set());
    assert!(app.inner.speech.busy_barged.load(Ordering::SeqCst));
    assert!(busy_stop.is_set());
    assert!(app.inner.session.user_speaking.load(Ordering::SeqCst));
}

#[test]
fn speech_start_discards_the_tts_tail() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, LISTENING);
    *app.inner.turn.post_tts_until.lock() = monotonic() + 10.0;
    app.on_speech_start(0.3);
    assert!(app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(app.inner.turn.post_tts_ignore.load(Ordering::SeqCst));
}

#[test]
fn speech_start_while_thinking_merges_the_previous_phrase() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, THINKING);
    *app.inner.turn.post_tts_until.lock() = 0.0;
    *app.inner.turn.last_turn_audio.lock() = Some((vec![1, 2], 16_000, monotonic() - 1.0));
    *app.inner.turn.last_voice_user_id.lock() = Some(7);
    app.on_speech_start(0.3);
    assert_eq!(
        *app.inner.turn.resume_pcm.lock(),
        Some((vec![1, 2], 16_000))
    );
    assert_eq!(*app.inner.turn.merge_user_id.lock(), Some(7));
    assert_eq!(turn_gen(app), 2);
    assert!(app.inner.session.user_speaking.load(Ordering::SeqCst));
}

#[test]
fn speech_start_while_thinking_ignores_old_audio() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, THINKING);
    *app.inner.turn.post_tts_until.lock() = 0.0;
    *app.inner.turn.last_turn_audio.lock() = Some((vec![1, 2], 16_000, monotonic() - 10.0));
    app.on_speech_start(0.3);
    assert!(app.inner.turn.resume_pcm.lock().is_none());
    assert_eq!(turn_gen(app), 2);
}

#[test]
fn speech_start_while_speaking_needs_the_detector_threshold() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, SPEAKING);
    {
        let mut cfg = app.inner.config.cfg.write();
        cfg.set("barge_in", json!(true));
        cfg.set("barge_in_rms", json!(0.008));
    }
    app.on_speech_start(0.4);
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    app.inner
        .session
        .ignore_utterance
        .store(false, Ordering::SeqCst);
    app.inner.config.cfg.write().set("barge_in_rms", json!(0.0));
    app.on_speech_start(0.4);
    assert_eq!(turn_gen(app), 2);
    assert!(app.inner.session.user_speaking.load(Ordering::SeqCst));
}

#[test]
fn utterance_outside_a_call_is_ignored() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.on_utterance(vec![1, 2], 16_000);
    assert!(app.inner.turn.last_turn_audio.lock().is_none());
    assert_eq!(turn_gen(app), 0);
}

#[test]
fn ignored_utterance_is_dropped_unless_it_continued_past_the_window() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    app.inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    app.on_utterance(vec![1, 2], 16_000);
    assert!(app.inner.turn.last_turn_audio.lock().is_none());
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert_eq!(turn_gen(app), 1);
    app.inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    app.inner.turn.post_tts_ignore.store(true, Ordering::SeqCst);
    *app.inner.turn.post_tts_until.lock() = monotonic() - 1.0;
    app.on_utterance(vec![1, 2], 16_000);
    assert!(app.inner.turn.last_turn_audio.lock().is_some());
    assert_eq!(turn_gen(app), 2);
}

#[test]
fn empty_and_muted_utterances_release_the_speaking_state() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    app.inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    app.inner.turn.post_tts_ignore.store(true, Ordering::SeqCst);
    app.on_utterance(Vec::new(), 16_000);
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(!app.inner.turn.post_tts_ignore.load(Ordering::SeqCst));
    app.inner
        .session
        .ignore_utterance
        .store(true, Ordering::SeqCst);
    app.inner.session.muted.store(true, Ordering::SeqCst);
    app.on_utterance(vec![1, 2], 16_000);
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(app.inner.turn.last_turn_audio.lock().is_none());
    assert_eq!(turn_gen(app), 1);
}

#[test]
fn utterance_merges_the_resumed_phrase_and_records_the_timing() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    fixture.mic.lock().last_speech_end = Some(12.5);
    *app.inner.turn.resume_pcm.lock() = Some((vec![9], 16_000));
    app.on_utterance(vec![1, 2], 16_000);
    let last = app
        .inner
        .turn
        .last_turn_audio
        .lock()
        .clone()
        .expect("audio del turno");
    assert_eq!(last.0, vec![9, 1, 2]);
    assert_eq!(last.1, 16_000);
    assert!(app.inner.turn.resume_pcm.lock().is_none());
    let timing = app
        .inner
        .turn
        .turn_timing
        .lock()
        .clone()
        .expect("marcas del turno");
    assert!((timing.speech_end - 12.5).abs() < 1e-9);
    assert_eq!(turn_gen(app), 2);
}

#[test]
fn utterance_does_not_merge_audio_from_another_rate() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    *app.inner.turn.resume_pcm.lock() = Some((vec![9], 8_000));
    app.on_utterance(vec![1], 16_000);
    let last = app
        .inner
        .turn
        .last_turn_audio
        .lock()
        .clone()
        .expect("audio del turno");
    assert_eq!(last.0, vec![1]);
}

#[test]
fn is_speaking_covers_tts_and_the_busy_notice() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    assert!(!app.is_speaking());
    set_state(app, SPEAKING);
    assert!(app.is_speaking());
    set_state(app, IDLE);
    app.inner.speech.busy_speaking.set();
    assert!(app.is_speaking());
}

#[test]
fn mic_level_is_safe_with_and_without_a_call() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.on_mic_level(0.3);
    app.start_call();
    set_state(app, LISTENING);
    app.inner.session.muted.store(true, Ordering::SeqCst);
    app.on_mic_level(0.9);
    assert!(in_call(app));
}

#[test]
fn call_worker_reports_a_missing_microphone() {
    let fixture = Fixture::new();
    fixture.app.call_worker(Arc::new(Flag::new()));
    let events: Vec<AppEvent> = std::iter::from_fn(|| fixture.app.events().get_nowait()).collect();
    assert_eq!(
        events,
        vec![AppEvent::Error(
            "No puedo usar el micrófono: No hay ningún micrófono disponible.".to_string(),
            None,
        )]
    );
}
