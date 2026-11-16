//! Presupuesto de herramientas, caché de lecturas y avisos de espera del
//! `Messenger` (TMAP-4: `test_voice_execution.py` y `test_audit_turn.py`).

use serde_json::{json, Value};

use super::*;

use crate::errors::LlmError;
use crate::messenger::Messenger;
use crate::openai::Chunk;

fn tool_messages(messenger: &Messenger) -> Vec<Value> {
    messenger
        .messages()
        .into_iter()
        .filter(|message| message["role"] == "tool")
        .collect()
}

/// Una vuelta con varias llamadas a herramienta (índices y argumentos).
fn tool_batch(calls: &[(&str, &str, &str)]) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = calls
        .iter()
        .enumerate()
        .map(|(index, (name, _args, id))| call(name, "", index, Some(id)))
        .collect();
    for (index, (_name, args, _id)) in calls.iter().enumerate() {
        chunks.push(call("", args, index, None));
    }
    chunks.push(done("tool_calls"));
    chunks
}

fn last_user(request: &Value) -> &str {
    request["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .and_then(|message| message["content"].as_str())
        .unwrap_or("")
}

#[test]
fn four_real_calls_get_a_final_answer_and_the_budget_note() {
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(4);
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_batch(&[
                ("web_search", "{\"query\":\"a\"}", "1"),
                ("fetch_page", "{\"url\":\"https://a\"}", "2"),
            ])),
            Script::Chunks(tool_batch(&[
                ("fetch_page", "{\"url\":\"https://b\"}", "3"),
                ("write_file", "{\"path\":\"table.md\"}", "4"),
            ])),
            Script::Chunks(vec![text("La tabla está creada en table.md.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    let spoken = say(&messenger, "Prepara una tabla.");
    assert_eq!(runtime.state().executed.len(), 4);
    let requests = requests(&client);
    assert_eq!(requests.len(), 3);
    assert!(last_user(&requests[0]).contains("Quedan 4 llamadas"));
    assert!(last_user(&requests[1]).contains("Quedan 2 llamadas"));
    assert!(requests[2].get("tools").is_none());
    assert!(spoken.ends_with("table.md.") && !spoken.contains("dando vueltas"));
    assert_eq!(
        messenger
            .messages()
            .last()
            .map(|message| message["content"].clone()),
        Some(json!("La tabla está creada en table.md."))
    );
    assert_eq!(
        runtime.state().recorded.last().map(String::as_str),
        Some("La tabla está creada en table.md.")
    );
}

#[test]
fn an_oversized_batch_stops_at_the_budget() {
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(1);
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_batch(&[
                ("write_file", "{}", "1"),
                ("write_file", "{}", "2"),
            ])),
            Script::Chunks(vec![text("Solo se completó la primera acción.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    say(&messenger, "");
    assert_eq!(runtime.state().executed.len(), 1);
    let tools = tool_messages(&messenger);
    let rejected: Value = serde_json::from_str(
        tools.last().expect("resultado de la herramienta")["content"]
            .as_str()
            .expect("texto"),
    )
    .expect("JSON del rechazo");
    assert_eq!(rejected["ok"], json!(false));
    assert_eq!(requests(&client).len(), 2);
    assert!(requests(&client)[1].get("tools").is_none());
}

#[test]
fn a_cut_tool_call_is_not_executed_and_the_history_stays_valid() {
    let runtime = FakeRuntime::new();
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![
                call(
                    "delegate_to_agent",
                    "{\"action\": \"run\", \"prompt\": \"revisa el repo y",
                    0,
                    Some("c1"),
                ),
                done("length"),
            ]),
            Script::Chunks(vec![text("Lo intento de nuevo, más corto.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    let spoken = say(&messenger, "lanza un agente");
    assert!(runtime.state().executed.is_empty());
    assert!(spoken.contains("más corto"));
    let assistant = messenger
        .messages()
        .into_iter()
        .find(|message| message.get("tool_calls").is_some())
        .expect("llamada guardada");
    let arguments = assistant["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .expect("argumentos");
    assert_eq!(
        serde_json::from_str::<Value>(arguments).expect("JSON reparado")["action"],
        json!("run")
    );
    let tools = tool_messages(&messenger);
    let content = tools.last().expect("error de la herramienta")["content"]
        .as_str()
        .expect("texto");
    assert!(content.contains("incompletos"));
    assert!(content.contains("\"ok\": false"));
    for message in requests(&client)[1]["messages"]
        .as_array()
        .expect("mensajes")
    {
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let raw = call["function"]["arguments"].as_str().unwrap_or("{}");
            serde_json::from_str::<Value>(raw).expect("la petición no lleva JSON roto");
        }
    }
}

#[test]
fn the_read_cache_is_canonical_and_lasts_only_one_turn() {
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(10);
    let (messenger, _client) = messenger_with(
        vec![
            Script::Chunks(tool_batch(&[(
                "web_search",
                "{\"query\":\"a\",\"max_results\":5}",
                "1",
            )])),
            Script::Chunks(tool_batch(&[(
                "web_search",
                "{\"max_results\":5, \"query\":\"a\"}",
                "2",
            )])),
            Script::Chunks(vec![text("Respuesta.")]),
            Script::Chunks(tool_batch(&[(
                "web_search",
                "{\"query\":\"a\",\"max_results\":5}",
                "3",
            )])),
            Script::Chunks(vec![text("Actualizado.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    say(&messenger, "");
    assert_eq!(runtime.state().executed.len(), 1);
    say(&messenger, "");
    assert_eq!(runtime.state().executed.len(), 2);
}

#[test]
fn mutating_tools_are_never_cached_and_the_round_limit_reserves_a_final_response() {
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(10);
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_round("write_file", "{}", "1")),
            Script::Chunks(tool_round("write_file", "{}", "2")),
            Script::Chunks(vec![text("Terminado.")]),
        ],
        Some(runtime.clone()),
        &[("llm_max_tool_rounds", json!(2))],
    );
    say(&messenger, "");
    assert_eq!(runtime.state().executed.len(), 2);
    assert_eq!(requests(&client).len(), 3);
    assert!(requests(&client)[2].get("tools").is_none());
}

#[test]
fn a_provider_ignoring_the_finalization_cannot_run_more_tools() {
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(1);
    let (messenger, _client) = messenger_with(
        vec![
            Script::Chunks(tool_round("write_file", "{}", "1")),
            Script::Chunks(tool_round("write_file", "{}", "2")),
        ],
        Some(runtime.clone()),
        &[],
    );
    let spoken = say(&messenger, "");
    assert_eq!(runtime.state().executed.len(), 1);
    assert!(spoken.contains("entrega final confirmada"));
    assert_eq!(
        messenger
            .messages()
            .last()
            .map(|message| message["role"].clone()),
        Some(json!("assistant"))
    );
}

#[test]
fn json_recovery_keeps_the_budget_note_and_drops_the_poisoned_history() {
    let error = || {
        Script::Fail(LlmError::http(
            500,
            "Failed to parse tool call arguments as JSON",
        ))
    };
    let runtime = FakeRuntime::new();
    runtime.state().budget = Some(4);
    let (messenger, client) = messenger_with(
        vec![error(), error(), Script::Chunks(vec![text("Recuperado.")])],
        Some(runtime),
        &[],
    );
    messenger.set_messages(vec![
        json!({"role": "system", "content": "S"}),
        json!({"role": "user", "content": "antes"}),
        json!({"role": "assistant", "content": "", "tool_calls": [
            {"id": "old", "type": "function", "function": {"name": "read_file", "arguments": "{}"}}
        ]}),
        json!({"role": "tool", "tool_call_id": "old", "content": "datos"}),
    ]);
    assert!(say(&messenger, "sigamos").ends_with("Recuperado."));
    let final_request = requests(&client).last().cloned().expect("petición final");
    let messages = final_request["messages"].as_array().expect("mensajes");
    assert!(!messages
        .iter()
        .any(|message| { message.get("tool_calls").is_some() || message["role"] == "tool" }));
    assert!(last_user(&final_request).contains("Quedan 4 llamadas"));
}
