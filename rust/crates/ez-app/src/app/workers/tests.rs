//! Pruebas de transcripción, respuesta hablada y cola de voz (F6-04).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_chat::store::Role;
use ez_core::metrics::TurnTimings;
use ez_llm::errors::LlmError;
use ez_llm::messenger::TurnError;
use serde_json::json;

use crate::app::states::tests::support::{
    chunk, drain_events, install_client, stt_server, wait_event, Fixture, Script, ScriptedClient,
};

use super::super::core::state::PendingStt;
use super::super::core::AppEvent;
use super::{final_outcome, is_tool_ack, llm_label, turn_error_name};

fn timing() -> Arc<TurnTimings> {
    Arc::new(TurnTimings::new(0.0, 0.0))
}

fn answer_of(app: &crate::app::core::VoiceApp) -> Option<(String, bool)> {
    app.inner
        .chat
        .chat
        .snapshot()
        .1
        .into_iter()
        .find(|message| message.role == Role::Assistant)
        .map(|message| (message.text, message.error))
}

#[test]
fn answer_worker_streams_text_and_speaks_its_sentences() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let generation = app.bump_turn();
    let marks = timing();
    *app.inner.turn.turn_timing.lock() = Some(Arc::clone(&marks));
    install_client(
        app,
        ScriptedClient::new(vec![Script::Chunks(vec![
            chunk("Perfecto, "),
            chunk("sigamos "),
            chunk("con el resto."),
        ])]),
    );
    app.answer_worker("hola", generation);
    assert_eq!(app.last_summary(), "Perfecto, sigamos con el resto.");
    assert_eq!(
        answer_of(app),
        Some(("Perfecto, sigamos con el resto.".to_string(), false))
    );
    assert_eq!(
        *fixture.speaker.synthesized.lock(),
        ["Perfecto,", "sigamos con el resto."]
    );
    assert!(fixture.speaker.spoken.lock().is_empty());
    assert_eq!(fixture.speaker.plays.load(Ordering::SeqCst), 2);
    let events = drain_events(app);
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::Speaking(gen) if *gen == generation)));
    assert!(events.iter().any(
        |event| matches!(event, AppEvent::AnswerDone(gen, outcome) if *gen == generation && outcome == "ok")
    ));
    let marked = marks.marks.lock().map(|marks| {
        ["first_token", "first_sentence", "first_sound"]
            .iter()
            .all(|name| marks.contains_key(*name))
    });
    assert!(matches!(marked, Ok(true)));
}

#[test]
fn answer_worker_without_voice_replies_keeps_text_only() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner.chat.chat.set_voice_replies(false);
    let generation = app.bump_turn();
    install_client(
        app,
        ScriptedClient::new(vec![Script::Chunks(vec![chunk("Hola.")])]),
    );
    app.answer_worker("hola", generation);
    assert_eq!(answer_of(app), Some(("Hola.".to_string(), false)));
    assert!(fixture.speaker.spoken.lock().is_empty());
    assert!(fixture.speaker.synthesized.lock().is_empty());
    let events = drain_events(app);
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(gen, outcome) if *gen == generation && outcome == "ok")));
}

#[test]
fn answer_worker_reports_llm_errors_with_the_friendly_text() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let generation = app.bump_turn();
    install_client(
        app,
        ScriptedClient::new(vec![Script::Fail(LlmError::http(401, "bad key"))]),
    );
    app.answer_worker("hola", generation);
    let expected =
        "La clave de OpenAI (GPT, Codex) no es válida o no tiene acceso. Revísala en ajustes.";
    assert_eq!(answer_of(app), Some((expected.to_string(), true)));
    let events = drain_events(app);
    assert!(events.iter().any(
        |event| matches!(event, AppEvent::Error(message, Some(gen)) if message == expected && *gen == generation)
    ));
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(gen, outcome) if *gen == generation && outcome == "error")));
}

#[test]
fn answer_worker_cancels_a_turn_that_is_no_longer_current() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let stale = app.bump_turn();
    app.bump_turn();
    install_client(
        app,
        ScriptedClient::new(vec![Script::Chunks(vec![chunk("Hola.")])]),
    );
    app.answer_worker("hola", stale);
    assert!(answer_of(app).is_none());
    let events = drain_events(app);
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(gen, outcome) if *gen == stale && outcome == "cancelled")));
}

#[test]
fn stt_worker_publishes_the_transcription_and_releases_the_slot() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let (url, server) = stt_server("hola mundo");
    app.inner.config.cfg.write().set("whisper_url", json!(url));
    let generation = app.bump_turn();
    let marks = timing();
    app.inner.turn.stt.lock().busy = true;
    app.stt_worker(vec![0u8; 640], 16_000, generation, Arc::clone(&marks));
    let _ = server.join();
    let events = drain_events(app);
    assert!(events.iter().any(
        |event| matches!(event, AppEvent::Transcribed(text, gen) if text == "hola mundo" && *gen == generation)
    ));
    assert!(!app.inner.turn.stt.lock().busy);
    assert!(marks
        .marks
        .lock()
        .map(|marks| marks.contains_key("stt_ready"))
        .unwrap_or(false));
}

#[test]
fn stt_worker_discards_whisper_hallucinations() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let (url, server) = stt_server("Gracias por ver el video");
    app.inner.config.cfg.write().set("whisper_url", json!(url));
    let generation = app.bump_turn();
    app.inner.turn.stt.lock().busy = true;
    app.stt_worker(vec![0u8; 640], 16_000, generation, timing());
    let _ = server.join();
    let events = drain_events(app);
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::Transcribed(text, _) if text.is_empty())));
}

#[test]
fn stt_worker_reports_a_missing_whisper_server() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner
        .config
        .cfg
        .write()
        .set("whisper_url", json!("http://127.0.0.1:1/inference"));
    let generation = app.bump_turn();
    let marks = timing();
    app.inner.turn.stt.lock().busy = true;
    app.stt_worker(vec![0u8; 640], 16_000, generation, Arc::clone(&marks));
    let events = drain_events(app);
    assert!(events.iter().any(|event| matches!(
        event,
        AppEvent::Error(message, Some(gen))
            if message.starts_with("No puedo conectar con whisper-server.") && *gen == generation
    )));
    assert!(!app.inner.turn.stt.lock().busy);
    assert!(!marks
        .marks
        .lock()
        .map(|marks| marks.contains_key("stt_ready"))
        .unwrap_or(false));
}

#[test]
fn start_stt_keeps_only_the_latest_pending() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner.turn.stt.lock().busy = true;
    app.start_stt(vec![1], 16_000, 1, timing());
    app.start_stt(vec![2], 16_000, 1, timing());
    let pending = app
        .inner
        .turn
        .stt
        .lock()
        .pending
        .as_ref()
        .map(|pending| pending.pcm.clone());
    assert_eq!(pending, Some(vec![2]));
}

#[test]
fn finish_stt_drops_a_stale_pending() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.bump_turn();
    {
        let mut stt = app.inner.turn.stt.lock();
        stt.busy = true;
        stt.pending = Some(PendingStt {
            pcm: vec![1],
            rate: 16_000,
            generation: 0,
            timing: timing(),
        });
    }
    app.finish_stt();
    assert!(!app.inner.turn.stt.lock().busy);
    assert!(app.inner.turn.stt.lock().pending.is_none());
    assert!(wait_event(app, |event| matches!(event, AppEvent::Transcribed(_, _))).is_empty());
}

#[test]
fn an_empty_ok_answer_becomes_empty() {
    assert_eq!(final_outcome("ok", ""), "empty");
    assert_eq!(final_outcome("ok", "algo"), "ok");
    assert_eq!(final_outcome("cancelled", ""), "cancelled");
}

#[test]
fn llm_label_uses_the_provider_spec() {
    let fixture = Fixture::new();
    let mut cfg = fixture.app.config();
    assert_eq!(llm_label(&cfg), "OpenAI (GPT, Codex)");
    cfg.set("llm_provider", json!("proveedor-raro"));
    assert_eq!(llm_label(&cfg), "proveedor-raro");
    cfg.set("llm_provider", json!(""));
    assert_eq!(llm_label(&cfg), "OpenCode Go");
}

#[test]
fn turn_error_names_match_the_python_exceptions() {
    assert_eq!(
        turn_error_name(&TurnError::Cancelled("x".into())),
        "TurnCancelled"
    );
    assert_eq!(
        turn_error_name(&TurnError::Timeout("x".into())),
        "LLMTurnTimeout"
    );
    assert_eq!(
        turn_error_name(&TurnError::Llm(LlmError::other("x"))),
        "LlmError"
    );
}

#[test]
fn tool_ack_phrases_are_recognised_without_real_answers() {
    for text in [
        "Busco en la web. ",
        "Abro la página.",
        "Ejecuto el script.",
        "Le escribo al agente.",
        "Se lo paso a un agente.",
        "Se lo paso a OpenCode. ",
    ] {
        assert!(is_tool_ack(text), "{text}");
    }
    assert!(!is_tool_ack("Abro la página y te cuento."));
    assert!(!is_tool_ack("Se lo paso a OpenCode y sigo."));
}
