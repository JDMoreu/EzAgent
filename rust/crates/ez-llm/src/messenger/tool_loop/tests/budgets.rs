//! Presupuesto de herramientas, caché de lecturas y avisos de espera del
//! `Messenger` (TMAP-4: `test_voice_execution.py` y `test_audit_turn.py`).

use std::sync::Arc;

use serde_json::{json, Value};

use super::*;

use crate::errors::LlmError;
use crate::messenger::{AskMode, Messenger};
use crate::openai::Chunk;

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

fn tool_messages(messenger: &Messenger) -> Vec<Value> {
    messenger
        .messages()
        .into_iter()
        .filter(|message| message["role"] == "tool")
        .collect()
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

#[test]
fn volatile_data_only_travels_in_the_request() {
    let runtime = FakeRuntime::new();
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![text("a")]),
            Script::Chunks(vec![text("b")]),
            Script::Chunks(vec![text("c")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    for (index, data) in ["MEM-A", "MEM-B", "MEM-C"].iter().enumerate() {
        runtime.state().parts = Some(("SISTEMA".to_string(), (*data).to_string()));
        assert_eq!(
            say(&messenger, &format!("turno {index}")),
            ["a", "b", "c"][index]
        );
    }
    assert!(messenger.messages().iter().all(|message| {
        !message["content"]
            .as_str()
            .unwrap_or("")
            .contains("Datos del entorno")
    }));
    for (index, data) in ["MEM-A", "MEM-B", "MEM-C"].iter().enumerate() {
        let request = &requests(&client)[index];
        let containing: Vec<&Value> = request["messages"]
            .as_array()
            .expect("mensajes")
            .iter()
            .filter(|message| {
                message["content"]
                    .as_str()
                    .unwrap_or("")
                    .contains("Datos del entorno")
            })
            .collect();
        assert_eq!(containing.len(), 1, "turno {index}");
        assert!(containing[0]["content"]
            .as_str()
            .unwrap_or("")
            .contains(data));
        let users: Vec<&Value> = request["messages"]
            .as_array()
            .expect("mensajes")
            .iter()
            .filter(|message| message["role"] == "user")
            .collect();
        assert_eq!(Some(containing[0]), users.last().copied());
    }
}

fn big_history(messenger: &Messenger, turns: usize, size: usize) {
    let mut messages = vec![json!({"role": "system", "content": "S"})];
    for index in 0..turns {
        messages.push(json!({
            "role": "user",
            "content": format!("pregunta {index} {}", "x".repeat(size)),
        }));
        messages.push(json!({
            "role": "assistant",
            "content": format!("respuesta {index} {}", "y".repeat(size)),
        }));
    }
    messenger.set_messages(messages);
}

fn notes_sink(messenger: &Messenger) -> Arc<std::sync::Mutex<Vec<String>>> {
    let notes: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let collected = Arc::clone(&notes);
    messenger.set_on_context_event(Arc::new(move |text: &str| {
        collected
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(text.to_string());
    }));
    notes
}

#[test]
fn a_full_conversation_is_summarised_instead_of_forgotten() {
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![text("- Hablamos de precios y de un agente lanzado.")]),
            Script::Chunks(vec![text("Seguimos.")]),
        ],
        None,
        &[("llm_context_tokens", json!(8000))],
    );
    let notes = notes_sink(&messenger);
    big_history(&messenger, 10, 2500);
    assert_eq!(say(&messenger, "¿en qué íbamos?"), "Seguimos.");
    let requests = requests(&client);
    assert_eq!(requests.len(), 2);
    let summary = &requests[0]["messages"];
    assert!(summary[0]["content"]
        .as_str()
        .unwrap_or("")
        .contains("archivista"));
    assert!(summary[1]["content"]
        .as_str()
        .unwrap_or("")
        .contains("pregunta 0"));
    assert!(messenger.summary().starts_with("- Hablamos de precios"));
    let system = requests[1]["messages"][0]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(system.contains("Resumen de la conversación hasta ahora"));
    assert!(system.contains("Hablamos de precios"));
    let users: Vec<String> = requests[1]["messages"]
        .as_array()
        .expect("mensajes")
        .iter()
        .filter(|message| message["role"] == "user")
        .map(|message| message["content"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(users.iter().any(|text| text.starts_with("pregunta 9")));
    assert!(!users.iter().any(|text| text.starts_with("pregunta 0")));
    assert_eq!(users.last().map(String::as_str), Some("¿en qué íbamos?"));
    let notes = notes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(notes.len(), 2);
    assert!(notes[0].contains("Compactando"));
    assert!(notes[1].contains("compactada"));
}

#[test]
fn the_summary_survives_the_next_turns_and_reset_clears_it() {
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(vec![text("RESUMEN-UNO")]),
            Script::Chunks(vec![text("a")]),
            Script::Chunks(vec![text("b")]),
        ],
        None,
        &[("llm_context_tokens", json!(8000))],
    );
    big_history(&messenger, 10, 2500);
    say(&messenger, "uno");
    assert_eq!(say(&messenger, "dos"), "b");
    let third = requests(&client)[2]["messages"][0]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(third.contains("RESUMEN-UNO"));
    messenger.reset_history();
    assert_eq!(messenger.summary(), "");
    assert_eq!(messenger.messages().len(), 1);
}

#[test]
fn a_short_conversation_is_never_compacted_and_a_bigger_window_postpones_it() {
    let (short, short_client) = messenger_with(
        vec![
            Script::Chunks(vec![text("uno")]),
            Script::Chunks(vec![text("dos")]),
        ],
        None,
        &[],
    );
    let notes = notes_sink(&short);
    say(&short, "a");
    say(&short, "b");
    assert_eq!(requests(&short_client).len(), 2);
    assert!(notes.lock().expect("notas").is_empty());
    assert_eq!(short.summary(), "");

    let (large, large_client) = messenger_with(
        vec![Script::Chunks(vec![text("ok")])],
        None,
        &[("llm_context_tokens", json!(200_000))],
    );
    big_history(&large, 10, 2500);
    say(&large, "x");
    assert_eq!(requests(&large_client).len(), 1);
    assert!(large.messages().len() > 20);
}

#[test]
fn a_single_enormous_turn_is_clipped_not_summarised() {
    let (messenger, client) = messenger_with(
        vec![Script::Chunks(vec![text("ok")])],
        None,
        &[("llm_context_tokens", json!(8000))],
    );
    messenger.set_messages(vec![
        json!({"role": "system", "content": "S"}),
        json!({"role": "user", "content": "lee el archivo"}),
        json!({"role": "assistant", "content": "", "tool_calls": [
            {"id": "c", "type": "function", "function": {"name": "r", "arguments": "{}"}}
        ]}),
        json!({"role": "tool", "tool_call_id": "c", "content": "Q".repeat(60_000)}),
        json!({"role": "assistant", "content": "es largo"}),
    ]);
    say(&messenger, "¿y ahora?");
    assert_eq!(requests(&client).len(), 1);
    let sent = requests(&client);
    let tool = sent[0]["messages"]
        .as_array()
        .expect("mensajes")
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("resultado");
    let content = tool["content"].as_str().unwrap_or("");
    assert!(content.chars().count() < 2000 && content.contains("recortado"));
}

#[test]
#[ignore = "TMAP: diferencia con Python, ver informe"]
fn a_failing_notification_callback_does_not_break_the_turn() {
    let (messenger, _client) = messenger_with(
        vec![
            Script::Chunks(vec![text("RES")]),
            Script::Chunks(vec![text("ok")]),
        ],
        None,
        &[("llm_context_tokens", json!(8000))],
    );
    messenger.set_on_context_event(Arc::new(|_text: &str| {
        panic!("la ventana se cerró");
    }));
    big_history(&messenger, 10, 2500);
    assert_eq!(say(&messenger, "x"), "ok");
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "la herramienta no llegó a ejecutarse"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn blocked_runtime() -> (Arc<FakeRuntime>, Arc<Gate>) {
    let runtime = FakeRuntime::new();
    let gate = Gate::new();
    runtime.state().budget = Some(4);
    runtime.state().execute_gate = Some(Arc::clone(&gate));
    (runtime, gate)
}

fn busy_messenger() -> (Messenger, Arc<FakeRuntime>, Arc<Gate>) {
    let (runtime, gate) = blocked_runtime();
    let (messenger, _client) = messenger_with(
        vec![
            Script::Chunks(tool_round("slow", "{}", "c1")),
            Script::Chunks(vec![text("segundo")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    (messenger, runtime, gate)
}

fn consume(messenger: Messenger, text: &'static str) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut out = String::new();
        for chunk in messenger.ask_stream(text, AskMode::Voice) {
            match chunk {
                Ok(text) => out.push_str(&text),
                Err(error) if error.is_cancelled() => return String::new(),
                Err(error) => return format!("ERROR:{error}"),
            }
        }
        out
    })
}

#[test]
fn the_waiting_turn_warns_once_after_the_busy_wait() {
    let (messenger, runtime, gate) = busy_messenger();
    let busy: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&busy);
    messenger.set_on_busy(Arc::new(move |mode: &str| {
        recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(mode.to_string());
    }));
    let first = consume(messenger.clone(), "uno");
    wait_until(|| !runtime.state().executed.is_empty());
    let started = std::time::Instant::now();
    let second = consume(messenger.clone(), "dos");
    wait_until(|| !busy.lock().expect("avisos").is_empty());
    assert!(started.elapsed().as_secs_f64() >= 1.4);
    assert_eq!(busy.lock().expect("avisos").as_slice(), ["voice"]);
    gate.release();
    first.join().expect("primer turno");
    assert_eq!(second.join().expect("segundo turno"), "segundo");
}

#[test]
fn no_warning_when_the_previous_turn_releases_early() {
    let (messenger, runtime, gate) = busy_messenger();
    let busy: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&busy);
    messenger.set_on_busy(Arc::new(move |mode: &str| {
        recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(mode.to_string());
    }));
    let first = consume(messenger.clone(), "uno");
    wait_until(|| !runtime.state().executed.is_empty());
    let second = consume(messenger.clone(), "dos");
    // Deja que el segundo turno pida el lock (y cancele el primero) antes de soltar.
    std::thread::sleep(std::time::Duration::from_millis(100));
    gate.release();
    let first_out = first.join().expect("primer turno");
    let second_out = second.join().expect("segundo turno");
    assert_eq!(second_out, "segundo", "primero: {first_out:?}");
    assert!(busy.lock().expect("avisos").is_empty());
}

#[test]
fn cancelling_the_wait_says_nothing() {
    let (messenger, runtime, gate) = busy_messenger();
    let busy: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&busy);
    messenger.set_on_busy(Arc::new(move |mode: &str| {
        recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(mode.to_string());
    }));
    let first = consume(messenger.clone(), "uno");
    wait_until(|| !runtime.state().executed.is_empty());
    let second = consume(messenger.clone(), "dos");
    std::thread::sleep(std::time::Duration::from_millis(300));
    messenger.cancel_turn();
    gate.release();
    first.join().expect("primer turno");
    assert_eq!(second.join().expect("segundo turno"), "");
    assert!(busy.lock().expect("avisos").is_empty());
}

#[test]
fn cancelling_during_a_tool_leaves_the_history_valid_and_no_extra_request() {
    let (runtime, gate) = blocked_runtime();
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_round("slow", "{}", "c1")),
            Script::Chunks(vec![text("Después.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    let worker = consume(messenger.clone(), "hazlo");
    wait_until(|| !runtime.state().executed.is_empty());
    messenger.cancel_turn();
    gate.release();
    let _ = worker.join().expect("turno cancelado");
    assert_eq!(requests(&client).len(), 1);
    assert!(tool_messages(&messenger)
        .iter()
        .any(|message| message["tool_call_id"] == "c1"));
    assert_eq!(
        messenger
            .messages()
            .last()
            .map(|message| message["role"].clone()),
        Some(json!("tool"))
    );

    assert_eq!(say(&messenger, "sigue"), "Después.");
    let sent = requests(&client)
        .last()
        .cloned()
        .expect("historial enviado");
    let messages = sent["messages"].as_array().expect("mensajes");
    let open_ids: Vec<&str> = messages
        .iter()
        .filter(|message| message["role"] == "assistant")
        .flat_map(|message| {
            message
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|call| call["id"].as_str())
        .collect();
    let closed_ids: Vec<&str> = messages
        .iter()
        .filter(|message| message["role"] == "tool")
        .filter_map(|message| message["tool_call_id"].as_str())
        .collect();
    assert!(open_ids.iter().all(|id| closed_ids.contains(id)));
}
