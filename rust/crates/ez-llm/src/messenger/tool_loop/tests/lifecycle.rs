//! Pruebas de cancelación y errores del `Messenger` (F4-07).

use super::*;

use crate::errors::LlmError;
use crate::messenger::{AskMode, TurnError};

#[test]
fn the_provider_error_is_propagated() {
    let (messenger, _) = messenger_with(
        vec![Script::Fail(LlmError::http(429, "too many"))],
        None,
        &[],
    );
    let error = messenger
        .ask_stream("x", AskMode::Voice)
        .collect::<Result<String, _>>()
        .expect_err("el proveedor falla");
    assert!(matches!(&error, TurnError::Llm(llm) if llm.status == Some(429)));
}

#[test]
fn cancelling_mid_stream_ends_the_turn_and_repairs_the_history() {
    let gate = Gate::new();
    let (messenger, _) = messenger_with(
        vec![Script::Blocked {
            first: text("Hola"),
            rest: vec![text(" mundo")],
            gate: gate.clone(),
        }],
        None,
        &[],
    );
    let mut stream = messenger.ask_stream("x", AskMode::Voice);
    assert_eq!(
        stream.next().expect("fragmento").expect("texto"),
        "Hola".to_string()
    );
    messenger.cancel_turn();
    gate.release();
    let remaining: Vec<Result<String, TurnError>> = stream.by_ref().collect();
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0]
        .as_ref()
        .expect_err("turno cancelado")
        .is_cancelled());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let messages = messenger.messages();
        if messages
            .last()
            .is_some_and(|message| message["content"] == "Hola [interrumpido]")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "el historial no se reparó"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
