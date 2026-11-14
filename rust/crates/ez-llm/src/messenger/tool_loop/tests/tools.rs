//! Pruebas del bucle de herramientas del `Messenger` (F4-08).

use super::*;

use crate::messenger::RuntimeFailure;
use serde_json::{json, Value};

#[test]
fn a_complete_tool_cycle_uses_the_result_in_the_next_request() {
    let runtime = FakeRuntime::new();
    runtime.result(
        "get_datetime",
        Ok(json!({"ok": true, "data": {"hora": "14:05"}, "error": null})),
    );
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_round("get_datetime", "{\"tz\":\"UTC\"}", "call_1")),
            Script::Chunks(vec![text("Son las 14:05.")]),
        ],
        Some(runtime.clone()),
        &[],
    );
    let spoken = say(&messenger, "qué hora es");
    assert_eq!(
        runtime.state().executed,
        [("get_datetime".into(), "{\"tz\":\"UTC\"}".into())]
    );
    assert_eq!(spoken, "Son las 14:05.");
    let messages = messenger.messages();
    let assistant = &messages[messages.len() - 3];
    assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
    assert_eq!(
        assistant["tool_calls"][0]["function"],
        json!({"name": "get_datetime", "arguments": "{\"tz\":\"UTC\"}"})
    );
    let tool = &messages[messages.len() - 2];
    assert_eq!(tool["role"], "tool");
    assert_eq!(tool["tool_call_id"], "call_1");
    assert_eq!(
        serde_json::from_str::<Value>(tool["content"].as_str().expect("contenido")).expect("JSON")
            ["data"],
        json!({"hora": "14:05"})
    );
    let requests = requests(&client);
    let roles: Vec<&str> = requests[1]["messages"]
        .as_array()
        .expect("mensajes")
        .iter()
        .filter_map(|message| message["role"].as_str())
        .collect();
    assert!(roles.ends_with(&["assistant", "tool"]));
    assert_eq!(messenger.last_rounds().len(), 2);
    assert!(messenger.last_rounds()[0].contains("get_datetime"));
}

#[test]
fn slow_tools_announce_what_they_do() {
    for (name, arguments, ack) in [
        ("web_search", "{}", "Busco en la web."),
        (
            "delegate_to_agent",
            "{\"action\":\"send\",\"address\":\"t1\"}",
            "Le escribo al agente.",
        ),
        (
            "delegate_to_agent",
            "{\"action\":\"run\",\"agent\":\"claude\"}",
            "Se lo paso a Claude Code.",
        ),
    ] {
        let (messenger, _) = messenger_with(
            vec![
                Script::Chunks(tool_round(name, arguments, "c1")),
                Script::Chunks(vec![text("Hecho.")]),
            ],
            Some(FakeRuntime::new()),
            &[],
        );
        assert_eq!(say(&messenger, "hazlo"), format!("{ack} Hecho."));
    }
}

#[test]
fn fast_tools_do_not_announce() {
    for (name, arguments) in [
        ("voice_control", "{\"action\":\"mute\"}"),
        ("delegate_to_agent", "{\"action\":\"list\"}"),
        ("read_file", "{\"path\":\"a.txt\"}"),
    ] {
        let (messenger, _) = messenger_with(
            vec![
                Script::Chunks(tool_round(name, arguments, "c1")),
                Script::Chunks(vec![text("Listo.")]),
            ],
            Some(FakeRuntime::new()),
            &[],
        );
        assert_eq!(say(&messenger, "hazlo"), "Listo.");
    }
}

#[test]
fn no_ack_when_the_model_already_spoke() {
    let mut round = vec![text("Busco precios de GPU.")];
    round.extend(tool_round("web_search", "{}", "c1"));
    let (messenger, _) = messenger_with(
        vec![
            Script::Chunks(round),
            Script::Chunks(vec![text("Encontré tres.")]),
        ],
        Some(FakeRuntime::new()),
        &[],
    );
    assert_eq!(
        say(&messenger, "busca"),
        "Busco precios de GPU. Encontré tres."
    );
}

#[test]
fn several_tools_run_in_index_order() {
    let round = vec![
        call("beta", "", 1, Some("b")),
        call("", "{}", 1, None),
        call("alfa", "", 0, Some("a")),
        call("", "{\"x\":1}", 0, None),
    ];
    let runtime = FakeRuntime::new();
    let (messenger, _) = messenger_with(
        vec![Script::Chunks(round), Script::Chunks(vec![text("ok")])],
        Some(runtime.clone()),
        &[],
    );
    say(&messenger, "haz dos cosas");
    let executed: Vec<String> = runtime
        .state()
        .executed
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    assert_eq!(executed, ["alfa", "beta"]);
    let ids: Vec<Value> = messenger
        .messages()
        .iter()
        .filter(|message| message["role"] == "tool")
        .map(|message| message["tool_call_id"].clone())
        .collect();
    assert_eq!(ids, [json!("a"), json!("b")]);
}

#[test]
fn a_runtime_failure_is_spoken_and_passed_to_the_model() {
    let runtime = FakeRuntime::new();
    runtime.result(
        "read_file",
        Err(RuntimeFailure::new("RuntimeError", "disco roto")),
    );
    let mut round = vec![text("Lo leo.")];
    round.extend(tool_round("read_file", "{}", "c1"));
    let (messenger, _) = messenger_with(
        vec![
            Script::Chunks(round),
            Script::Chunks(vec![text("No pude leerlo.")]),
        ],
        Some(runtime),
        &[],
    );
    let spoken = say(&messenger, "lee");
    assert!(spoken.contains("No pude ejecutar la herramienta"));
    assert!(spoken.ends_with("No pude leerlo."));
    let tool = messenger
        .messages()
        .into_iter()
        .find(|message| message["role"] == "tool")
        .expect("mensaje de herramienta");
    let payload: Value =
        serde_json::from_str(tool["content"].as_str().expect("contenido")).expect("JSON");
    assert_eq!(payload["ok"], false);
    assert!(payload["error"]
        .as_str()
        .is_some_and(|text| text.contains("RuntimeError") && text.contains("disco roto")));
}

#[test]
fn repeated_web_queries_finish_with_results_instead_of_asking_to_rephrase() {
    let (messenger, client) = messenger_with(
        vec![
            Script::Chunks(tool_round("web_search", "{}", "c1")),
            Script::Chunks(tool_round("web_search", "{}", "c2")),
            Script::Chunks(vec![text(
                "Encontré resultados, pero aún faltan fuentes por verificar.",
            )]),
        ],
        Some(FakeRuntime::new()),
        &[],
    );
    let spoken = say(&messenger, "busca sin parar");
    assert_eq!(requests(&client).len(), 3);
    assert_eq!(requests(&client)[2].get("tools"), None);
    assert!(spoken.ends_with("faltan fuentes por verificar."));
    assert_eq!(
        messenger
            .messages()
            .last()
            .map(|message| message["role"].clone()),
        Some(json!("assistant"))
    );
}

#[test]
fn a_tool_selection_failure_falls_back_to_base_tools() {
    let runtime = FakeRuntime::new();
    runtime.state().tools_error = Some(RuntimeFailure::new(
        "TypeError",
        "expected string or bytes-like object",
    ));
    runtime.state().base_tools = vec![
        json!({"type": "function", "function": {"name": "delegate_to_agent"}}),
        json!({"type": "function", "function": {"name": "web_search"}}),
    ];
    let (messenger, client) = messenger_with(
        vec![Script::Chunks(vec![text("Hecho.")])],
        Some(runtime),
        &[],
    );
    assert_eq!(say(&messenger, "hola"), "Hecho.");
    let requests = requests(&client);
    let names: Vec<&str> = requests[0]["tools"]
        .as_array()
        .expect("herramientas")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert!(names.contains(&"delegate_to_agent"));
}
