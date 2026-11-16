//! Pruebas de texto, petición y modo chat del `Messenger` (F4-07).

use std::sync::atomic::Ordering;

use super::*;

use crate::messenger::{AskMode, RuntimeFailure, CHAT_MODE_PROMPT, EMPTY_ANSWER, TOOL_MAX_TOKENS};
use crate::tool_markup::TOOL_MARKUP_NOTICE;
use serde_json::json;

fn tool_ack_of(name: &str, arguments: &str) -> Option<String> {
    super::super::tool_ack(name, arguments)
}

#[test]
fn streams_text_and_keeps_history() {
    let runtime = FakeRuntime::new();
    let (messenger, _) = messenger_with(
        vec![Script::Chunks(vec![
            text("Hola, "),
            empty(),
            text("¿qué tal?"),
        ])],
        Some(runtime.clone()),
        &[],
    );
    let pieces: Vec<String> = messenger
        .ask_stream("hola", AskMode::Voice)
        .collect::<Result<_, _>>()
        .expect("turno completo");
    assert_eq!(pieces, ["Hola, ", "¿qué tal?"]);
    let messages = messenger.messages();
    assert_eq!(messages[messages.len() - 1]["content"], "Hola, ¿qué tal?");
    assert_eq!(messages[messages.len() - 2]["content"], "hola");
    assert_eq!(
        messages[0],
        json!({"role": "system", "content": "PROMPT-SISTEMA"})
    );
    let state = runtime.state();
    assert_eq!(state.turns, ["hola"]);
    assert_eq!(state.recorded, ["Hola, ¿qué tal?"]);
    assert_eq!(messenger.last_rounds().len(), 1);
    assert!(messenger.last_rounds()[0].starts_with("llamada 1:"));
}

#[test]
fn ask_returns_clean_text_or_the_backup_message() {
    let (messenger, _) = messenger_with(vec![Script::Chunks(vec![text("  Hola  ")])], None, &[]);
    assert_eq!(messenger.ask("x").expect("respuesta"), "Hola");
    let (empty_messenger, _) = messenger_with(Vec::new(), None, &[]);
    assert_eq!(
        empty_messenger.ask("x").expect("respaldo"),
        "No he podido completar la consulta."
    );
}

#[test]
fn request_carries_model_temperature_and_tools() {
    let runtime = FakeRuntime::new();
    let (messenger, client) =
        messenger_with(vec![Script::Chunks(vec![text("ok")])], Some(runtime), &[]);
    say(&messenger, "hola");
    let requests = requests(&client);
    let request = &requests[0];
    assert_eq!(request["model"], "gpt-x");
    assert_eq!(request["stream"], true);
    assert_eq!(request["max_tokens"], TOOL_MAX_TOKENS);
    assert_eq!(request["temperature"], 0.3);
    assert_eq!(request["tools"][0]["function"]["name"], "get_datetime");
    assert_eq!(request["tool_choice"], "auto");
    assert_eq!(request["messages"][0]["content"], "PROMPT-SISTEMA");
}

#[test]
fn without_tools_there_is_no_tool_choice() {
    let runtime = FakeRuntime::new();
    runtime.state().tools.clear();
    let (messenger, client) =
        messenger_with(vec![Script::Chunks(vec![text("ok")])], Some(runtime), &[]);
    say(&messenger, "hola");
    let requests = requests(&client);
    assert!(requests[0].get("tools").is_none());
    assert!(requests[0].get("tool_choice").is_none());
    assert_eq!(requests[0]["max_tokens"], 150);
}

#[test]
fn without_runtime_there_are_no_tools_and_the_prompt_says_so() {
    let (messenger, client) = messenger_with(
        vec![Script::Chunks(vec![text("No puedo hacer eso ahora.")])],
        None,
        &[],
    );
    assert!(messenger.messages()[0]["content"]
        .as_str()
        .is_some_and(|text| text.contains("núcleo agéntico no está disponible")));
    say(&messenger, "borra todo");
    assert!(requests(&client)[0].get("tools").is_none());
}

#[test]
fn a_tool_without_runtime_is_not_executed() {
    let (messenger, _) = messenger_with(
        vec![
            Script::Chunks(tool_round("delete_path", "{\"path\": \"C:/\"}", "c1")),
            Script::Chunks(vec![text("No puedo.")]),
        ],
        None,
        &[],
    );
    let spoken = say(&messenger, "borra");
    assert!(spoken.contains("No pude ejecutar la herramienta"));
}

#[test]
fn without_a_runtime_the_chat_prompt_does_not_stick_to_the_next_turn() {
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![text("Hola.")]),
            Script::Chunks(vec![text("Adiós.")]),
        ],
        None,
        &[],
    );
    let _ = messenger
        .ask_stream("hola", AskMode::Chat)
        .collect::<Result<String, _>>()
        .expect("chat");
    say(&messenger, "adiós");
    let requests = requests(&client);
    let first = requests[0]["messages"][0]["content"]
        .as_str()
        .expect("prompt");
    assert!(first.contains(CHAT_MODE_PROMPT));
    let second = requests[1]["messages"][0]["content"]
        .as_str()
        .expect("prompt");
    assert!(!second.contains(CHAT_MODE_PROMPT));
    assert!(second.contains("núcleo agéntico no está disponible"));
}

#[test]
fn a_runtime_that_fails_to_begin_degrades_to_conversation() {
    let runtime = FakeRuntime::new();
    runtime.state().begin_error = Some(RuntimeFailure::new("RuntimeError", "memoria corrupta"));
    let (messenger, client) = messenger_with(
        vec![Script::Chunks(vec![text("Hola igual.")])],
        Some(runtime),
        &[],
    );
    assert_eq!(messenger.ask("hola").expect("respuesta"), "Hola igual.");
    assert!(requests(&client)[0].get("tools").is_none());
}

#[test]
fn chat_mode_allows_markdown_and_long_answers() {
    let (messenger, client) = messenger_with(
        vec![Script::Chunks(vec![text("## Resumen\n\n- uno")])],
        Some(FakeRuntime::new()),
        &[],
    );
    let answer: String = messenger
        .ask_stream("resume", AskMode::Chat)
        .collect::<Result<_, _>>()
        .expect("turno completo");
    assert_eq!(answer, "## Resumen\n\n- uno");
    let requests = requests(&client);
    assert_eq!(requests[0]["max_tokens"], 2400);
    let system = requests[0]["messages"][0]["content"]
        .as_str()
        .expect("prompt");
    assert!(system.starts_with("PROMPT-SISTEMA") && system.contains(CHAT_MODE_PROMPT));
}

#[test]
fn the_chat_mode_block_does_not_leak_into_the_next_voice_turn() {
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![text("Uno.")]),
            Script::Chunks(vec![text("Dos.")]),
        ],
        Some(FakeRuntime::new()),
        &[],
    );
    let _ = messenger
        .ask_stream("a", AskMode::Chat)
        .collect::<Result<String, _>>()
        .expect("chat");
    say(&messenger, "b");
    let requests = requests(&client);
    let system = requests[1]["messages"][0]["content"]
        .as_str()
        .expect("prompt");
    assert!(!system.contains(CHAT_MODE_PROMPT));
    assert_eq!(requests[1]["max_tokens"], TOOL_MAX_TOKENS);
}

#[test]
fn chat_does_not_add_filler_acks() {
    let (messenger, _) = messenger_with(
        vec![
            Script::Chunks(tool_round("web_search", "{}", "c1")),
            Script::Chunks(vec![text("Encontré tres resultados.")]),
        ],
        Some(FakeRuntime::new()),
        &[],
    );
    let out: String = messenger
        .ask_stream("busca", AskMode::Chat)
        .collect::<Result<_, _>>()
        .expect("turno completo");
    assert_eq!(out, "Encontré tres resultados.");
}

#[test]
fn tool_markup_written_as_text_is_neither_spoken_nor_saved() {
    let dsml = "<｜DSML｜ calls> <｜DSML｜ invoke name=\"delegate_to_agent\">";
    let (messenger, _) = messenger_with(
        vec![Script::Chunks(vec![
            text("Se lo paso. "),
            text(&dsml[..5]),
            text(&format!("{}{}", &dsml[5..], "basura")),
        ])],
        Some(FakeRuntime::new()),
        &[],
    );
    let spoken = say(&messenger, "dile a claude");
    assert!(!spoken.contains("DSML") && !spoken.contains("invoke"));
    assert_eq!(spoken, format!("Se lo paso. {TOOL_MARKUP_NOTICE}"));
    assert!(messenger
        .messages()
        .iter()
        .all(|message| !message["content"].as_str().unwrap_or("").contains("DSML")));
}

#[test]
fn an_empty_answer_is_announced_and_recorded() {
    let (messenger, _) = messenger_with(vec![Script::Chunks(Vec::new())], None, &[]);
    assert_eq!(say(&messenger, "x"), EMPTY_ANSWER);
    assert_eq!(
        messenger.messages().last().expect("mensaje")["content"],
        EMPTY_ANSWER
    );
}

#[test]
fn a_length_finish_reason_appends_the_notice() {
    let (messenger, _) = messenger_with(
        vec![Script::Chunks(vec![text("Corte."), done("length")])],
        None,
        &[],
    );
    assert_eq!(
        say(&messenger, "x"),
        "Corte. La respuesta se cortó por el límite de tokens; pide algo más corto."
    );
}

#[test]
fn exhausted_rounds_end_with_the_fallback() {
    let (messenger, _) = messenger_with(
        vec![
            Script::Chunks(tool_round("web_search", "{}", "c1")),
            Script::Chunks(Vec::new()),
        ],
        Some(FakeRuntime::new()),
        &[("llm_max_tool_rounds", json!(1))],
    );
    let spoken = say(&messenger, "busca");
    assert_eq!(
        spoken,
        "Busco en la web. Conservé los resultados obtenidos, pero no pude completar la tarea. \
Todavía no hay una entrega final confirmada."
    );
}

#[test]
fn tool_ack_matches_the_python_phrases() {
    assert_eq!(
        tool_ack_of("web_search", "{}").as_deref(),
        Some("Busco en la web.")
    );
    assert_eq!(
        tool_ack_of("fetch_page", "{}").as_deref(),
        Some("Abro la página.")
    );
    assert_eq!(
        tool_ack_of("run_skill_script", "{}").as_deref(),
        Some("Ejecuto el script.")
    );
    assert_eq!(
        tool_ack_of(
            "delegate_to_agent",
            "{\"action\":\"run\",\"agent\":\"opencode\"}"
        )
        .as_deref(),
        Some("Se lo paso a OpenCode.")
    );
    assert_eq!(
        tool_ack_of(
            "delegate_to_agent",
            "{\"action\":\"run\",\"agent\":\"otro\"}"
        )
        .as_deref(),
        Some("Se lo paso a otro.")
    );
    assert_eq!(
        tool_ack_of("delegate_to_agent", "{\"action\":\"run\"}").as_deref(),
        Some("Se lo paso a un agente.")
    );
    assert_eq!(
        tool_ack_of("delegate_to_agent", "{\"action\":\"send\"}").as_deref(),
        Some("Le escribo al agente.")
    );
    assert_eq!(tool_ack_of("read_file", "{}"), None);
    assert_eq!(
        tool_ack_of("web_search", "no-json").as_deref(),
        Some("Busco en la web.")
    );
}

#[test]
fn the_stream_is_closed_when_the_turn_ends() {
    let (messenger, client) = messenger_with(vec![Script::Chunks(vec![text("ok")])], None, &[]);
    assert!(messenger.ask("x").is_ok());
    assert!(client.closed.load(Ordering::SeqCst) > 0);
}

#[test]
fn reload_llm_regenerates_the_prompt_and_refreshes_the_runtime() {
    let runtime = FakeRuntime::new();
    let (messenger, _) = messenger_with(
        vec![Script::Chunks(vec![text("a")])],
        Some(runtime.clone()),
        &[],
    );
    say(&messenger, "hola");
    runtime.state().prompt = "PROMPT-NUEVO".into();
    messenger.reload_llm();
    assert_eq!(
        messenger.messages(),
        [json!({"role": "system", "content": "PROMPT-NUEVO"})]
    );
    assert_eq!(runtime.state().refreshed, 1);
}

#[test]
fn set_confirm_actions_updates_config_runtime_and_prompt() {
    let runtime = FakeRuntime::new();
    let (messenger, _) = messenger_with(Vec::new(), Some(runtime.clone()), &[]);
    messenger.set_confirm_actions(true);
    assert_eq!(
        messenger.config().get("confirm_actions"),
        Some(&json!(true))
    );
    assert_eq!(runtime.state().confirm, Some(true));
    assert_eq!(messenger.messages()[0]["content"], json!("PROMPT-SISTEMA"));
}

#[test]
fn pending_notify_address_delegates_to_the_runtime() {
    let runtime = FakeRuntime::new();
    runtime.state().notify = Some("t1:9".into());
    let (messenger, _) = messenger_with(Vec::new(), Some(runtime), &[]);
    assert_eq!(messenger.pending_notify_address().as_deref(), Some("t1:9"));
}

#[test]
fn reset_history_while_a_turn_is_active_is_deferred() {
    let (messenger, _) = messenger_with(Vec::new(), None, &[]);
    messenger.history().turn_active = true;
    messenger.reset_history();
    assert_eq!(messenger.messages().len(), 1);
    messenger.history().turn_active = false;
    messenger.apply_deferred_history();
    assert_eq!(messenger.messages().len(), 1);
    assert_eq!(messenger.messages()[0]["role"], "system");
}

#[test]
fn a_normal_conversation_keeps_its_early_turns() {
    // F18: el tope ya es por tokens; un número fijo olvidaba vueltas sin resumirlas.
    let scripts: Vec<Script> = (0..12)
        .map(|index| Script::Chunks(vec![text(&format!("r{index}"))]))
        .collect();
    let (messenger, _) = messenger_with(scripts, None, &[]);
    for index in 0..12 {
        say(&messenger, &format!("pregunta {index}"));
    }
    let messages = messenger.messages();
    assert_eq!(messages.len(), 25);
    assert!(messages
        .iter()
        .any(|message| message["content"] == "pregunta 0"));
    assert_eq!(
        messages.last().map(|message| message["content"].clone()),
        Some(json!("r11"))
    );
}
