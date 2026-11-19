//! Pruebas de `_tick`, `_handle_hotkey` y `_handle_event` (F6-03).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_core::metrics::{monotonic, TurnTimings};
use serde_json::json;

use super::support::Fixture;
use super::{in_call, turn_gen};
use crate::app::core::{AppEvent, MenuEvent, VoiceApp, LISTENING, SPEAKING, THINKING};
use crate::app::states::tests::support::{
    install_client, wait_event, Fixture as SpeakingFixture, ScriptedClient,
};
use crate::fast::ACK_REPLIES;

fn set_state(app: &VoiceApp, state: &str) {
    *app.inner.session.state.write() = state.to_string();
}

#[test]
fn tick_drains_hotkeys_events_and_menu_in_order() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.hotkey_events().put("toggle".to_string());
    app.events()
        .put(AppEvent::CloseRequest("hangup".to_string()));
    app.menu_events().put(MenuEvent::Hide);
    app.tick();
    assert!(in_call(app));
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.turn.close_after_answer.load(Ordering::SeqCst));
    assert_eq!(app.inner.turn.close_mode.read().as_str(), "hangup");
    assert!(app.hotkey_events().is_empty());
    assert!(app.events().is_empty());
    assert!(app.menu_events().is_empty());
}

#[test]
fn hotkey_toggle_starts_and_hangs_up() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.handle_hotkey("toggle");
    assert!(in_call(app));
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.turn.call_stop.lock().is_some());
    app.handle_hotkey("toggle");
    assert!(!in_call(app));
    assert_eq!(turn_gen(app), 2);
    assert!(app
        .inner
        .turn
        .call_stop
        .lock()
        .as_ref()
        .is_some_and(|stop| stop.is_set()));
}

#[test]
fn hotkey_cancel_is_safe_outside_a_call() {
    let fixture = Fixture::new();
    fixture.app.handle_hotkey("cancel");
    assert!(!in_call(&fixture.app));
    assert_eq!(turn_gen(&fixture.app), 0);
}

#[test]
fn hotkey_menu_shows_the_hidden_bar_and_saves_it() {
    let fixture = Fixture::new();
    assert!(fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    fixture.app.handle_hotkey("menu");
    assert!(!fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert!(fixture.saved_config().contains("hud_hidden"));
    assert_eq!(
        fixture
            .app
            .config()
            .get("hud_hidden")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[test]
fn the_chat_hotkey_shows_the_hidden_bar() {
    let fixture = Fixture::new();
    assert!(fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    fixture.app.handle_hotkey("chat");
    assert!(!fixture.app.inner.config.hud_hidden.load(Ordering::SeqCst));
    assert!(fixture.saved_config().contains("hud_hidden"));
}

#[test]
fn show_bar_if_hidden_is_a_noop_when_visible() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .config
        .hud_hidden
        .store(false, Ordering::SeqCst);
    fixture.app.show_bar_if_hidden();
    assert!(fixture.saved_config().is_empty());
}

#[test]
fn hotkey_capture_suspends_and_restores_the_hotkeys() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    assert_eq!(app.inner.ui.hotkeys.lock().len(), 8);
    app.handle_event(AppEvent::HotkeyCapture(true));
    assert!(app.inner.ui.hotkeys.lock().is_empty());
    app.handle_event(AppEvent::HotkeyCapture(false));
    assert_eq!(app.inner.ui.hotkeys.lock().len(), 8);
}

#[test]
fn transcribed_is_ignored_without_a_call_or_from_another_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.handle_event(AppEvent::Transcribed("hola".to_string(), 0));
    assert!(app.inner.turn.last_turn_audio.lock().is_none());
    app.start_call();
    assert_eq!(turn_gen(app), 1);
    app.handle_event(AppEvent::Transcribed("hola".to_string(), 0));
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.runtime.workers.lock().is_empty());
}

#[test]
fn transcribed_empty_only_returns_to_listening() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    app.handle_event(AppEvent::Transcribed("  \n ".to_string(), 1));
    assert_eq!(turn_gen(app), 1);
    assert!(app.inner.runtime.workers.lock().is_empty());
}

#[test]
fn transcribed_starts_the_answer_worker_for_the_current_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    // «hola» lo atiende el atajo local de F6-06a; aquí se pide una frase normal.
    install_client(app, ScriptedClient::new(Vec::new()));
    app.start_call();
    app.handle_event(AppEvent::Transcribed(
        "  revisa el informe  ".to_string(),
        1,
    ));
    assert_eq!(turn_gen(app), 1);
    assert_eq!(app.inner.runtime.workers.lock().len(), 1);
}

#[test]
fn answer_done_clears_the_turn_and_resets_capture() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    set_state(app, SPEAKING);
    *app.inner.speech.speaking_kind.write() = Some("answer".to_string());
    *app.inner.turn.turn_timing.lock() = Some(Arc::new(TurnTimings::new(0.0, 1.0)));
    app.inner.audio.reset_capture.clear();
    let before = monotonic();
    app.handle_event(AppEvent::AnswerDone(generation, "ok".to_string()));
    assert!(app.inner.audio.reset_capture.is_set());
    assert!(!app.inner.session.ignore_utterance.load(Ordering::SeqCst));
    assert!(!app.inner.turn.post_tts_ignore.load(Ordering::SeqCst));
    assert!(*app.inner.turn.post_tts_until.lock() >= before + 0.25);
    assert!(app.inner.turn.turn_timing.lock().is_none());
    assert!(app.inner.speech.speaking_kind.read().is_none());
}

#[test]
fn answer_done_from_another_turn_changes_nothing() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    app.inner.audio.reset_capture.clear();
    app.handle_event(AppEvent::AnswerDone(99, "ok".to_string()));
    assert!(!app.inner.audio.reset_capture.is_set());
}

#[test]
fn answer_done_honours_close_after_answer() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    app.inner
        .turn
        .close_after_answer
        .store(true, Ordering::SeqCst);
    *app.inner.turn.close_mode.write() = "pause".to_string();
    app.handle_event(AppEvent::AnswerDone(generation, "ok".to_string()));
    assert!(!app.inner.turn.close_after_answer.load(Ordering::SeqCst));
}

#[test]
fn speak_done_clears_the_echo_guards() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    *app.inner.speech.speaking_kind.write() = Some("confirm".to_string());
    app.inner.audio.reset_capture.clear();
    app.handle_event(AppEvent::SpeakDone(generation));
    assert!(app.inner.audio.reset_capture.is_set());
    assert!(app.inner.speech.speaking_kind.read().is_none());
    assert!(*app.inner.turn.post_tts_until.lock() > 0.0);
}

#[test]
fn speak_done_from_another_turn_is_ignored() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    app.inner.audio.reset_capture.clear();
    app.handle_event(AppEvent::SpeakDone(turn_gen(app) + 1));
    assert!(!app.inner.audio.reset_capture.is_set());
}

#[test]
fn busy_done_follows_the_barge_in_flag() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    // Fuera de LISTENING es cuando el aviso terminado reinicia la captura.
    set_state(app, THINKING);
    let generation = turn_gen(app);
    app.inner.speech.busy_speaking.set();
    app.inner.audio.reset_capture.clear();
    *app.inner.turn.post_tts_until.lock() = 0.0;
    let before = monotonic();
    app.handle_event(AppEvent::BusyDone(generation));
    assert!(!app.inner.speech.busy_speaking.is_set());
    assert!(app.inner.audio.reset_capture.is_set());
    assert!(*app.inner.turn.post_tts_until.lock() >= before + 0.2);
    // Si el aviso ya estaba cortado, no se resetea la captura.
    app.inner.speech.busy_speaking.set();
    app.inner.speech.busy_barged.store(true, Ordering::SeqCst);
    app.inner.audio.reset_capture.clear();
    app.handle_event(AppEvent::BusyDone(generation));
    assert!(!app.inner.speech.busy_barged.load(Ordering::SeqCst));
    assert!(!app.inner.audio.reset_capture.is_set());
    // Un turno viejo no toca nada.
    app.inner.speech.busy_speaking.set();
    app.handle_event(AppEvent::BusyDone(generation + 1));
    assert!(!app.inner.speech.busy_speaking.is_set());
    assert!(!app.inner.audio.reset_capture.is_set());
}

#[test]
fn busy_notice_adds_a_chat_note_for_the_current_turn() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    let before = app.inner.chat.chat.snapshot().1.len();
    app.handle_event(AppEvent::BusyNotice(generation, "answer".to_string()));
    let messages = app.inner.chat.chat.snapshot().1;
    assert_eq!(messages.len(), before + 1);
    assert_eq!(
        messages.last().map(|m| m.text.as_str()),
        Some("Termino lo anterior y sigo.")
    );
    assert!(!app.inner.speech.busy_speaking.is_set());
    app.handle_event(AppEvent::BusyNotice(generation + 1, "answer".to_string()));
    assert_eq!(app.inner.chat.chat.snapshot().1.len(), before + 1);

    // Con voz en el chat, además se dice (y se marca como aviso en curso).
    app.inner.chat.chat.set_voice_replies(true);
    app.handle_event(AppEvent::BusyNotice(generation, "chat".to_string()));
    assert_eq!(app.inner.chat.chat.snapshot().1.len(), before + 2);
    assert!(app.inner.speech.busy_speaking.is_set());
}

#[test]
fn confirm_ask_stores_the_token_and_confirm_done_clears_the_card() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.handle_event(AppEvent::ConfirmAsk(
        "¿Sigo?".to_string(),
        true,
        Some("tok-1".to_string()),
    ));
    assert_eq!(app.inner.turn.confirm_token.read().as_str(), "tok-1");
    app.inner.chat.chat.ask_confirm("¿Sigo?", true);
    app.handle_event(AppEvent::ConfirmDone);
    assert!(app.inner.chat.chat.confirm().is_none());
    app.handle_event(AppEvent::ConfirmAsk("¿Sigo?".to_string(), false, None));
    assert!(app.inner.turn.confirm_token.read().is_empty());
}

#[test]
fn confirm_click_without_an_agent_only_stops_the_tts() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    let stop = Arc::new(crate::app::core::state::Flag::new());
    *app.inner.speech.tts_stop.lock() = Some(Arc::clone(&stop));
    app.handle_event(AppEvent::ConfirmClick(false, Some("tok".to_string())));
    assert!(stop.is_set());
}

#[test]
fn errors_ignore_stale_turns_and_hang_up_on_microphone_failures() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    let generation = turn_gen(app);
    app.handle_event(AppEvent::Error("x".to_string(), Some(0)));
    assert!(in_call(app));
    app.handle_event(AppEvent::Error(
        "Kokoro: roto".to_string(),
        Some(generation),
    ));
    assert!(in_call(app));
    app.handle_event(AppEvent::Error(
        "No puedo usar el micrófono: sin dispositivo".to_string(),
        None,
    ));
    assert!(!in_call(app));
}

#[test]
fn test_done_restores_barge_in() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.inner.config.cfg.write().set("barge_in", json!(false));
    app.handle_event(AppEvent::TestDone(false, 0.5));
    assert_eq!(
        app.config()
            .get("barge_in")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    app.handle_event(AppEvent::TestDone(true, 0.0));
    assert_eq!(
        app.config()
            .get("barge_in")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[test]
fn the_chat_microphone_starts_listening_and_then_toggles_mute() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    // Sin llamada y sin silencio, el botón del chat sólo pide reanudar la
    // escucha (AppHooks::start_call del arnés no tiene efecto visible).
    app.handle_event(AppEvent::ChatMute(false));
    assert!(!in_call(app));
    assert!(!app.inner.session.muted.load(Ordering::SeqCst));
    // Dentro de la llamada silencia y desilencia el mismo micro de la barra.
    app.start_call();
    assert!(in_call(app));
    app.handle_event(AppEvent::ChatMute(true));
    assert!(app.inner.session.muted.load(Ordering::SeqCst));
    app.handle_event(AppEvent::ChatMute(false));
    assert!(!app.inner.session.muted.load(Ordering::SeqCst));
}

#[test]
fn chat_send_and_other_seams_are_safe() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.handle_event(AppEvent::ChatSend("hola".to_string()));
    app.handle_event(AppEvent::Say("hola".to_string()));
    app.handle_event(AppEvent::ChatNew);
    app.handle_event(AppEvent::ChatSwitch(1));
    app.handle_event(AppEvent::ChatDelete(1));
    app.handle_event(AppEvent::ChatStop);
    app.handle_event(AppEvent::ChatMute(false));
    app.handle_event(AppEvent::HudAction("mute".to_string()));
    app.handle_event(AppEvent::TrayAction("open".to_string()));
    app.handle_event(AppEvent::MenuAction("panel".to_string()));
    app.handle_event(AppEvent::ActivityOpen("a1".to_string()));
    app.handle_event(AppEvent::ActivityDismiss("a1".to_string()));
    app.handle_event(AppEvent::AgentWarn("aviso".to_string()));
    app.handle_event(AppEvent::LlmModelsReady(
        "opencode".to_string(),
        vec!["modelo".to_string()],
    ));
    app.handle_event(AppEvent::Wake);
    app.handle_event(AppEvent::Hide);
    app.handle_event(AppEvent::Raw(vec![json!("hud_error"), json!("roto")]));
    app.handle_event(AppEvent::Raw(vec![json!("desconocido")]));
    // «say» y «agent_warn» hablan de verdad (antes eran costuras vacías).
    assert_eq!(app.state(), SPEAKING);
}

#[test]
fn barge_in_event_routes_to_the_call_handler() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    app.start_call();
    set_state(app, SPEAKING);
    app.handle_event(AppEvent::AudioBargeIn(0.4));
    assert_eq!(turn_gen(app), 2);
    assert!(app.inner.session.user_speaking.load(Ordering::SeqCst));
}

#[test]
fn guarded_swallows_a_panicking_handler() {
    super::guarded(|| panic!("evento roto"));
    // Llegar aquí ya prueba que el pánico no cruzó el bucle.
    assert!(std::panic::catch_unwind(|| 1 + 1).is_ok());
}

#[test]
fn speech_start_event_is_ignored_without_a_call() {
    let fixture = Fixture::new();
    let app = &fixture.app;
    set_state(app, LISTENING);
    app.handle_event(AppEvent::AudioSpeechStart(0.2));
    assert!(!app.inner.session.user_speaking.load(Ordering::SeqCst));
}

/// Líneas `ack_fast` ya registradas en el registro temporal de las pruebas.
fn ack_fast_records() -> usize {
    let text = std::fs::read_to_string(ez_core::dirs::latency_path()).unwrap_or_default();
    text.lines()
        .filter(|line| line.contains("ack_fast"))
        .count()
}

/// Tamaño y fecha de la ruta real, sin la variable `EZAGENT_LOGS_DIR`.
fn real_latency_stamp() -> Option<(u64, Option<std::time::SystemTime>)> {
    let real = ez_core::dirs::app_data_dir()
        .join("logs")
        .join("latency.jsonl");
    std::fs::metadata(&real)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

/// Prepara una salida hablada reciente para probar el camino rápido del acuse.
fn arm_outbound(fixture: &SpeakingFixture, kind: &str, question: bool) {
    let speech = &fixture.app.inner.speech;
    *speech.last_outbound_kind.write() = Some(kind.to_string());
    *speech.last_outbound_done_at.write() = monotonic();
    *speech.last_outbound_question.write() = question;
}

#[test]
fn ack_after_a_statement_speaks_one_short_phrase_without_the_llm() {
    let fixture = SpeakingFixture::new();
    let app = &fixture.app;
    install_client(app, ScriptedClient::new(Vec::new()));
    app.start_call();
    let generation = turn_gen(app);
    let before = app.inner.chat.chat.snapshot().1.len();
    let ack_records = ack_fast_records();
    arm_outbound(&fixture, "announce", false);

    app.handle_event(AppEvent::Transcribed("ok".to_string(), generation));

    assert!(
        ack_fast_records() > ack_records,
        "el acuse debe registrarse como ack_fast"
    );
    // El camino rápido no abre un turno del LLM: no se añade mensaje de usuario.
    assert_eq!(app.inner.chat.chat.snapshot().1.len(), before);
    wait_event(app, |event| matches!(event, AppEvent::SpeakDone(_)));
    // El hilo de TTS solo encola el evento: se procesa aquí como en `_tick`.
    app.handle_event(AppEvent::SpeakDone(turn_gen(app)));
    let spoken = fixture.speaker.spoken.lock();
    assert_eq!(spoken.len(), 1, "{spoken:?}");
    assert!(ACK_REPLIES.contains(&spoken[0].as_str()), "{spoken:?}");
    drop(spoken);
    // La respuesta se dice con kind `ack`: no rearma el atajo.
    assert_eq!(
        app.inner.speech.last_outbound_kind.read().as_deref(),
        Some("ack")
    );
}

#[test]
fn the_ack_fast_path_writes_to_the_test_logs_not_the_real_one() {
    let before = real_latency_stamp();
    let fixture = SpeakingFixture::new();
    let app = &fixture.app;
    install_client(app, ScriptedClient::new(Vec::new()));
    app.start_call();
    let generation = turn_gen(app);
    let ack_records = ack_fast_records();
    arm_outbound(&fixture, "announce", false);

    app.handle_event(AppEvent::Transcribed("ok".to_string(), generation));

    // La ruta real no cambia: ni tamaño ni fecha.
    assert_eq!(before, real_latency_stamp());
    // El registro temporal sí recibe la línea del acuse.
    assert!(
        ack_fast_records() > ack_records,
        "el acuse debe registrarse en el registro temporal"
    );
}

#[test]
fn ack_after_a_question_goes_to_the_llm_as_today() {
    let fixture = SpeakingFixture::new();
    let app = &fixture.app;
    install_client(app, ScriptedClient::new(Vec::new()));
    app.start_call();
    let generation = turn_gen(app);
    arm_outbound(&fixture, "announce", true);

    app.handle_event(AppEvent::Transcribed("ok".to_string(), generation));

    // No hay respuesta rápida: se consultó el contexto y se consumió el tipo.
    assert!(fixture.speaker.spoken.lock().is_empty());
    assert!(app.inner.speech.last_outbound_kind.read().is_none());
}

#[test]
fn a_second_ack_without_a_new_outbound_goes_to_the_llm() {
    let fixture = SpeakingFixture::new();
    let app = &fixture.app;
    install_client(app, ScriptedClient::new(Vec::new()));
    app.start_call();
    arm_outbound(&fixture, "announce", false);
    app.handle_event(AppEvent::Transcribed("ok".to_string(), turn_gen(app)));
    wait_event(app, |event| matches!(event, AppEvent::SpeakDone(_)));
    app.handle_event(AppEvent::SpeakDone(turn_gen(app)));
    assert_eq!(fixture.speaker.spoken.lock().len(), 1);
    assert_eq!(
        app.inner.speech.last_outbound_kind.read().as_deref(),
        Some("ack")
    );

    // Sin nueva salida hablada, el segundo «ok» ya no activa el atajo.
    app.handle_event(AppEvent::Transcribed("ok".to_string(), turn_gen(app)));
    assert_eq!(fixture.speaker.spoken.lock().len(), 1);
}
