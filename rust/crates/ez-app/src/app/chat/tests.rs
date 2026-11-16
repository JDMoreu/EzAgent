//! Pruebas del chat escrito (F6-05).

use ez_chat::store::Source;
use ez_llm::errors::LlmError;
use serde_json::json;

use crate::app::core::{AppEvent, IDLE, SPEAKING, THINKING};
use crate::app::states::tests::support::{
    chunk, drain_events, install_client, wait_event, Fixture, Script, ScriptedClient,
};

fn scripted(fixture: &Fixture, scripts: Vec<Script>) {
    install_client(&fixture.app, ScriptedClient::new(scripts));
}

fn answer_done(events: &[AppEvent], expected: &str) -> bool {
    events
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(_, outcome) if outcome == expected))
}

#[test]
fn chat_send_streams_the_answer_into_the_chat() {
    let fixture = Fixture::new();
    scripted(
        &fixture,
        vec![Script::Chunks(vec![chunk("Listo."), chunk(" Ahora.")])],
    );
    drain_events(&fixture.app);
    fixture.app.chat_send("  hazlo  ");
    assert_eq!(fixture.app.state(), THINKING);
    assert_eq!(fixture.app.inner.chat.chat.snapshot().1[0].text, "hazlo");
    let events = wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::AnswerDone(..))
    });
    assert!(answer_done(&events, "ok"));
    assert_eq!(
        fixture.app.inner.chat.chat.last_assistant_text(),
        "Listo. Ahora."
    );
    assert_eq!(fixture.app.last_summary(), "Listo. Ahora.");
}

#[test]
fn chat_send_ignores_blank_text() {
    let fixture = Fixture::new();
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_send(" \t ");
    assert_eq!(crate::app::events::turn_gen(&fixture.app), generation);
    assert!(fixture.app.inner.chat.chat.snapshot().1.is_empty());
}

/// Confirmación pendiente que apunta lo que el usuario escribe.
struct PendingGate {
    submitted: parking_lot::Mutex<Vec<String>>,
}

impl ez_agent::runtime::ConfirmationGate for PendingGate {
    fn ask(&self, _summary: &str, _strong: bool, _require_number: Option<i64>) -> bool {
        true
    }

    fn pending(&self) -> bool {
        true
    }

    fn pending_token(&self) -> Option<String> {
        None
    }

    fn arm(&self, _token: Option<&str>) {}

    fn submit_text(&self, text: &str, _token: Option<&str>) -> Option<bool> {
        self.submitted.lock().push(text.to_string());
        Some(true)
    }

    fn submit_click(&self, _yes: bool) {}

    fn cancel(&self, _reason: &str) {}
}

#[test]
fn chat_send_answers_a_pending_confirmation_instead_of_starting_a_turn() {
    let fixture = Fixture::new();
    let gate = std::sync::Arc::new(PendingGate {
        submitted: parking_lot::Mutex::new(Vec::new()),
    });
    let permissions = std::env::temp_dir().join(format!(
        "ezagent-chat-confirm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    let runtime = ez_agent::runtime::AgentRuntime::new(
        ez_core::config::VoiceConfig::new(),
        None,
        ez_agent::runtime::RuntimeCallbacks::default(),
        Some(&permissions),
    )
    .expect("runtime de prueba");
    runtime.set_confirm_gate(Some(
        std::sync::Arc::clone(&gate) as std::sync::Arc<dyn ez_agent::runtime::ConfirmationGate>
    ));
    *fixture.app.inner.agents.agent.lock() = Some(runtime);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_send("confirmo 3");
    assert_eq!(gate.submitted.lock().as_slice(), ["confirmo 3".to_string()]);
    assert_eq!(crate::app::events::turn_gen(&fixture.app), generation);
    let messages = fixture.app.inner.chat.chat.snapshot().1;
    assert_eq!(
        messages.last().map(|message| message.text.as_str()),
        Some("confirmo 3")
    );
    let _ = std::fs::remove_file(&permissions);
}

#[test]
fn chat_worker_speaks_the_answer_when_chat_voice_is_on() {
    let fixture = Fixture::new();
    scripted(
        &fixture,
        vec![Script::Chunks(vec![chunk("Las ventas subieron un 12 %.")])],
    );
    drain_events(&fixture.app);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_worker("cómo van las ventas", generation);
    assert_eq!(
        fixture.speaker.synthesized.lock().as_slice(),
        ["Las ventas subieron un 12 %.".to_string()]
    );
}

#[test]
fn chat_worker_skips_tables_and_points_to_the_chat() {
    let fixture = Fixture::new();
    scripted(
        &fixture,
        vec![Script::Chunks(vec![chunk(
            "Te dejo la comparativa.\n\n| a | b |\n|---|---|\n| 1 | 2 |",
        )])],
    );
    drain_events(&fixture.app);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_worker("compara", generation);
    assert_eq!(
        fixture.speaker.synthesized.lock().as_slice(),
        ["Te dejo la comparativa. El detalle está en el chat.".to_string()]
    );
}

#[test]
fn chat_worker_shows_the_messenger_empty_answer() {
    let fixture = Fixture::new();
    scripted(&fixture, vec![Script::Chunks(Vec::new())]);
    drain_events(&fixture.app);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_worker("hola", generation);
    let message = fixture.app.inner.chat.chat.snapshot().1.remove(0);
    // El mensajero sustituye una salida vacía por su aviso (igual que Python).
    assert_eq!(
        message.text,
        "El modelo devolvió una respuesta vacía. Prueba otra vez o cambia de modelo en ajustes."
    );
    let events = drain_events(&fixture.app);
    assert!(answer_done(&events, "ok"));
}

#[test]
fn chat_worker_reports_a_friendly_model_error() {
    let fixture = Fixture::new();
    scripted(
        &fixture,
        vec![Script::Fail(LlmError::other("boom del proveedor"))],
    );
    drain_events(&fixture.app);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_worker("hola", generation);
    let message = fixture.app.inner.chat.chat.snapshot().1.remove(0);
    assert!(message.error);
    assert!(message.text.contains("boom del proveedor"));
    let events = drain_events(&fixture.app);
    assert!(answer_done(&events, "error"));
}

#[test]
fn chat_worker_drops_a_turn_from_an_older_generation() {
    let fixture = Fixture::new();
    scripted(&fixture, vec![Script::Chunks(vec![chunk("Hola.")])]);
    drain_events(&fixture.app);
    let generation = crate::app::events::turn_gen(&fixture.app) + 1;
    fixture.app.chat_worker("hola", generation);
    assert!(fixture.app.inner.chat.chat.snapshot().1.is_empty());
    let events = drain_events(&fixture.app);
    assert!(answer_done(&events, "cancelled"));
}

#[test]
fn chat_stop_cuts_the_voice_and_returns_to_idle() {
    let fixture = Fixture::new();
    let stop = fixture.app.new_tts_stop();
    *fixture.app.inner.speech.speaking_kind.write() = Some("answer".to_string());
    *fixture.app.inner.session.state.write() = SPEAKING.to_string();
    fixture.app.chat_stop();
    assert!(stop.is_set());
    assert!(fixture.app.inner.speech.speaking_kind.read().is_none());
    assert_eq!(fixture.app.state(), IDLE);
}

#[test]
fn chat_new_starts_an_empty_conversation() {
    let fixture = Fixture::new();
    fixture.app.inner.chat.chat.add_user("antes", Source::Chat);
    *fixture.app.inner.session.last_summary.write() = "resumen viejo".to_string();
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_new();
    assert!(fixture.app.inner.chat.chat.snapshot().1.is_empty());
    assert_eq!(fixture.app.last_summary(), "");
    assert_eq!(crate::app::events::turn_gen(&fixture.app), generation + 1);
}

#[test]
fn chat_switch_does_nothing_while_an_answer_is_running() {
    let fixture = Fixture::new();
    let message = fixture.app.inner.chat.chat.begin_assistant(Source::Chat);
    let generation = crate::app::events::turn_gen(&fixture.app);
    fixture.app.chat_switch(7);
    assert_eq!(crate::app::events::turn_gen(&fixture.app), generation);
    fixture.app.inner.chat.chat.cancel(message.id);
}

#[test]
fn set_chat_voice_applies_and_persists_the_choice() {
    let fixture = Fixture::new();
    fixture.app.set_chat_voice(false);
    assert!(!fixture.app.inner.chat.chat.voice_replies());
    assert!(fixture.saved_config().contains("\"chat_voice\": false"));
    fixture.app.set_chat_voice(true);
    assert!(fixture.app.inner.chat.chat.voice_replies());
    assert!(fixture.saved_config().contains("\"chat_voice\": true"));
}

#[test]
fn chat_media_roots_fall_back_to_home_without_an_agent() {
    let fixture = Fixture::new();
    let (roots, bases) = fixture.app.chat_media_roots();
    assert_eq!(roots, vec![ez_core::dirs::home()]);
    assert!(bases.is_empty());
}

#[test]
fn read_roots_expand_only_the_home_prefix() {
    let home = ez_core::dirs::home();
    assert_eq!(super::expand_user("~"), home);
    assert_eq!(
        super::expand_user("~/notas"),
        std::path::PathBuf::from(format!("{}/notas", home.display()))
    );
    assert_eq!(
        super::expand_user("$HOME/notas"),
        std::path::PathBuf::from("$HOME/notas")
    );
}

#[test]
fn speak_busy_does_not_repeat_while_the_notice_sounds() {
    let fixture = Fixture::new();
    fixture.app.inner.speech.busy_speaking.set();
    fixture.app.speak_busy("Un momento.", 1);
    assert!(fixture.speaker.spoken.lock().is_empty());
}

#[test]
fn speak_busy_speaks_and_reports_its_generation() {
    let fixture = Fixture::new();
    fixture.app.speak_busy("Un momento.", 9);
    assert!(fixture.app.inner.speech.busy_speaking.is_set());
    let events = wait_event(&fixture.app, |event| matches!(event, AppEvent::BusyDone(9)));
    assert!(events
        .iter()
        .any(|event| matches!(event, AppEvent::BusyDone(9))));
    assert_eq!(
        fixture.speaker.spoken.lock().as_slice(),
        ["Un momento.".to_string()]
    );
}

#[test]
fn chat_config_values_are_read_from_voice_config() {
    let fixture = Fixture::new();
    fixture
        .app
        .inner
        .config
        .cfg
        .write()
        .set("chat_voice", json!(true));
    assert!(fixture.app.inner.chat.chat.voice_replies());
    assert_eq!(
        fixture
            .app
            .config()
            .get("chat_voice")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(!fixture.app.inner.chat.chat.busy());
}
