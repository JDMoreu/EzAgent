//! Paridad de `llm_tools.py` contra `parity/fixtures/llm_tools.jsonl`.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use ez_core::llm_providers::ModelCapabilities;
use ez_llm::messenger::{
    max_tokens_with, notice, remaining_tools, seal_tool_calls, tool_ack, user_content,
    with_volatile, AgentRuntimeApi, LlmSettings, TurnToken,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

#[path = "support/llm_tools_client.rs"]
mod client;
#[path = "support/llm_tools_turn.rs"]
mod turn;

use client::{
    default_messenger, field, messenger_with, strings_of, text_of, FakeRuntime, ScriptedClient,
    ScriptedStream,
};
use turn::{turn_error_text, turn_output};

const FIXTURES: &str = "../../../parity/fixtures/llm_tools.jsonl";

fn produced(case: &FixtureCase) -> Result<Value, String> {
    match case.function.as_str() {
        "turn" => turn_output(case),
        "ask" => ask_case(case),
        "tool_ack" | "trim_history" | "seal_tool_calls" => basic_case(case),
        "close_interrupted" | "note_done" => history_case(case),
        "max_tokens" | "request_timeout" | "turn_timeout" | "tool_round_limit" => limit_case(case),
        "user_content" | "notice" | "with_volatile" => message_case(case),
        "tools_for" | "base_tools" | "remaining_tools" | "prompt_parts" => tools_case(case),
        "wants_notify" | "wants_close" => wants_case(case),
        other => state_case(case, other),
    }
}

fn ask_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let runtime = match input.get("runtime") {
        Some(Value::Null) => None,
        _ => Some(Arc::new(FakeRuntime::with_default_prompt(&json!({})))),
    };
    let client = Arc::new(ScriptedClient::new(
        field(input, "scripts")
            .as_array()
            .cloned()
            .unwrap_or_default(),
        None,
    ));
    let messenger = messenger_with(runtime, Some(client), &[]);
    let answer = messenger.ask("x").map_err(turn_error_text)?;
    Ok(json!({"answer": answer, "history": messenger.messages()}))
}

fn basic_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "tool_ack" => {
            let raw = match field(input, "raw") {
                Value::String(text) => text.clone(),
                _ => String::new(),
            };
            Ok(json!({"ack": tool_ack(&text_of(input, "name"), &raw)}))
        }
        "trim_history" => {
            let messenger = default_messenger();
            messenger.set_messages(
                field(input, "messages")
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            messenger.trim_history(field(input, "keep").as_u64().unwrap_or(12) as usize);
            Ok(json!({"messages": messenger.messages()}))
        }
        _ => {
            let mut ordered = field(input, "ordered")
                .as_array()
                .cloned()
                .unwrap_or_default();
            let finish = input.get("finish_reason").and_then(Value::as_str);
            let complete = seal_tool_calls(&mut ordered, finish);
            Ok(json!({"ordered": ordered, "complete": complete}))
        }
    }
}

fn history_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let runtime = Arc::new(FakeRuntime::with_default_prompt(&json!({})));
    let messenger = messenger_with(Some(runtime), None, &[]);
    if case.function == "close_interrupted" {
        if let Some(messages) = input.get("messages").and_then(Value::as_array) {
            messenger.set_messages(messages.clone());
        }
        let partial = strings_of(input, "partial");
        let open_calls = field(input, "open_calls")
            .as_array()
            .cloned()
            .unwrap_or_default();
        let appended = field(input, "assistant_appended")
            .as_bool()
            .unwrap_or(false);
        messenger.close_interrupted(&partial, &open_calls, appended);
        return Ok(json!({"messages": messenger.messages()}));
    }
    messenger.set_turn_active(field(input, "turn_active").as_bool().unwrap_or(false));
    let args = field(input, "args");
    messenger.note_done(
        &text_of(args, "address"),
        &text_of(args, "agent"),
        &text_of(args, "reason"),
        &text_of(args, "output"),
        &text_of(args, "task"),
    );
    Ok(json!({"messages": messenger.messages(), "pending": messenger.pending_notes()}))
}

fn limit_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "max_tokens" => {
            let caps = input.get("caps").cloned().unwrap_or(Value::Null);
            Ok(json!({"max_tokens": max_tokens_with(
                field(input, "llm_max_tokens").as_i64().unwrap_or(0),
                field(input, "chat").as_bool().unwrap_or(false),
                field(input, "has_tools").as_bool().unwrap_or(false),
                field(&caps, "reasoning").as_bool().unwrap_or(false),
                field(&caps, "max_output").as_u64().unwrap_or(32_000) as u32,
            )}))
        }
        "tool_round_limit" => {
            let mut cfg = VoiceConfig::new();
            cfg.set(
                "llm_max_tool_rounds",
                field(input, "llm_max_tool_rounds").clone(),
            );
            Ok(json!({"limit": LlmSettings::from_config(&cfg).tool_round_limit()}))
        }
        _ => {
            let key = if case.function == "request_timeout" {
                "llm_timeout_secs"
            } else {
                "llm_turn_timeout_secs"
            };
            let mut cfg = VoiceConfig::new();
            cfg.set(key, field(input, key).clone());
            let settings = LlmSettings::from_config(&cfg);
            let timeout = if case.function == "request_timeout" {
                settings.request_timeout()
            } else {
                settings.turn_timeout()
            };
            Ok(json!({"timeout": timeout}))
        }
    }
}

fn message_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "user_content" => Ok(json!({"content": user_content(
            &text_of(input, "text"),
            &text_of(input, "volatile"),
        )})),
        "notice" => Ok(json!({"notice": notice(&text_of(input, "text"))})),
        _ => {
            let messages = field(input, "messages")
                .as_array()
                .cloned()
                .unwrap_or_default();
            let volatile = text_of(input, "volatile");
            let mut output = json!({"messages": with_volatile(&messages, &volatile)});
            if case.case.ends_with("no_muta") {
                output["original"] = json!(messages);
            }
            Ok(output)
        }
    }
}

fn tools_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    if case.function == "tools_for" {
        let variant = text_of(input, "variant");
        let runtime = match variant.as_str() {
            "runtime_none" => None,
            "tools_for_falla" => Some(Arc::new(FakeRuntime::with_default_prompt(
                &json!({"tools_for_fails": true}),
            ))),
            _ => Some(Arc::new(FakeRuntime::with_default_prompt(&json!({})))),
        };
        let caps = ModelCapabilities {
            tools: variant != "caps_sin_herramientas",
            temperature: true,
            max_output: 32_000,
            reasoning: false,
        };
        let messenger = default_messenger();
        let runtime = runtime.map(|runtime| runtime as Arc<dyn AgentRuntimeApi>);
        return Ok(json!({"tools": messenger.tools_for(runtime.as_ref(), "hola", caps)}));
    }
    if case.function == "base_tools" {
        let runtime = Arc::new(FakeRuntime::with_default_prompt(&json!({
            "registry_fails": field(input, "registry_fails").as_bool().unwrap_or(false),
        })));
        let messenger = default_messenger();
        let runtime = runtime as Arc<dyn AgentRuntimeApi>;
        return Ok(json!({"tools": messenger.base_tools(&runtime)}));
    }
    if case.function == "remaining_tools" {
        let spec = match text_of(input, "variant").as_str() {
            "ninguno" => return Ok(json!({"remaining": remaining_tools(None)})),
            "cero" => json!({"remaining": 0}),
            "tres" => json!({"remaining": 3}),
            "negativo" => json!({"remaining": -2}),
            _ => json!({}),
        };
        let runtime = Arc::new(FakeRuntime::with_default_prompt(&spec));
        let runtime = runtime as Arc<dyn AgentRuntimeApi>;
        return Ok(json!({"remaining": remaining_tools(Some(&runtime))}));
    }
    let spec = match text_of(input, "variant").as_str() {
        "completo" => json!({"parts": true, "prompt": "P", "volatile": "V"}),
        "vacio" => json!({"parts": true, "prompt": "P", "volatile": ""}),
        "parts_falla" => json!({"parts_fails": true}),
        _ => json!({}),
    };
    let runtime = Arc::new(FakeRuntime::with_default_prompt(&spec));
    let messenger = default_messenger();
    let runtime = runtime as Arc<dyn AgentRuntimeApi>;
    let (stable, volatile) = messenger.prompt_parts(&runtime);
    Ok(json!({"parts": [stable, volatile]}))
}

fn wants_case(case: &FixtureCase) -> Result<Value, String> {
    let messenger = default_messenger();
    messenger.import_context(Vec::new(), "", &text_of(&case.input, "text"));
    let result = if case.function == "wants_notify" {
        messenger.wants_notify()
    } else {
        messenger.wants_close()
    };
    Ok(json!({"result": result}))
}

fn state_case(case: &FixtureCase, function: &str) -> Result<Value, String> {
    match function {
        "export_context" | "import_context" | "reset_history" => state_history(case, function),
        "reload_llm" => reload_case(case),
        "set_confirm_actions" | "close" | "pending_notify_address" | "cancel_turn" => {
            lifecycle_case(case, function)
        }
        "apply_deferred_history" | "take_pending_notes" | "check_cancel" => {
            deferred_case(case, function)
        }
        other => panic!("función desconocida: {other}"),
    }
}

fn state_history(case: &FixtureCase, function: &str) -> Result<Value, String> {
    let input = &case.input;
    let messenger = default_messenger();
    let messages = field(input, "messages")
        .as_array()
        .cloned()
        .unwrap_or_default();
    if function == "export_context" {
        let body = messages
            .into_iter()
            .filter(|m| m.get("role").and_then(Value::as_str) != Some("system"))
            .collect();
        messenger.import_context(
            body,
            &text_of(input, "summary"),
            &text_of(input, "last_user"),
        );
        let (messages, summary, last_user) = messenger.export_context();
        return Ok(json!({"messages": messages, "summary": summary, "last_user": last_user}));
    }
    if function == "import_context" {
        let active = field(input, "turn_active").as_bool().unwrap_or(false);
        messenger.set_turn_active(active);
        messenger.import_context(
            messages,
            &text_of(input, "summary"),
            &text_of(input, "last_user"),
        );
        if !active {
            return Ok(json!({
                "messages": messenger.messages(),
                "summary": messenger.summary(),
                "last_user": messenger.export_context().2,
            }));
        }
        let (messages, summary, last_user) = messenger.deferred_history().unwrap_or_default();
        return Ok(
            json!({"messages": messenger.messages(), "deferred": [messages, summary, last_user]}),
        );
    }
    messenger.set_messages(messages);
    let active = field(input, "turn_active").as_bool().unwrap_or(false);
    messenger.set_turn_active(active);
    messenger.set_pending_notes(vec![json!({"role": "user", "content": "nota"})]);
    messenger.reset_history();
    if !active {
        return Ok(json!({
            "messages": messenger.messages(),
            "summary": messenger.summary(),
            "last_user": messenger.export_context().2,
        }));
    }
    let (messages, summary, last_user) = messenger.deferred_history().unwrap_or_default();
    Ok(json!({
        "messages": messenger.messages(),
        "deferred": [messages, summary, last_user],
        "pending": messenger.pending_notes(),
    }))
}

fn reload_case(case: &FixtureCase) -> Result<Value, String> {
    let runtime = Arc::new(FakeRuntime::with_default_prompt(&json!({})));
    let client = Arc::new(ScriptedClient::new(Vec::new(), None));
    let messenger = messenger_with(Some(runtime.clone()), Some(client.clone()), &[]);
    messenger.set_messages(
        field(&case.input, "messages")
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    *runtime.prompt.lock().expect("prompt") = text_of(&case.input, "prompt");
    messenger.reload_llm();
    Ok(json!({
        "messages": messenger.messages(),
        "refreshed": runtime.refreshed.load(Ordering::SeqCst),
        "old_closed": client.closed.load(Ordering::SeqCst),
    }))
}

fn lifecycle_case(case: &FixtureCase, function: &str) -> Result<Value, String> {
    let input = &case.input;
    let runtime = Arc::new(FakeRuntime::with_default_prompt(&json!({})));
    let client = Arc::new(ScriptedClient::new(Vec::new(), None));
    let messenger = messenger_with(Some(runtime.clone()), Some(client.clone()), &[]);
    match function {
        "set_confirm_actions" => {
            let active = field(input, "turn_active").as_bool().unwrap_or(false);
            messenger.set_turn_active(active);
            messenger.set_confirm_actions(field(input, "enabled").as_bool().unwrap_or(false));
            let mut output = json!({
                "cfg": messenger.config().get("confirm_actions").cloned().unwrap_or(Value::Null),
                "system": messenger.messages()[0].clone(),
            });
            if !active {
                output["runtime"] = json!(*runtime.confirm.lock().expect("confirmación"));
            }
            Ok(output)
        }
        "close" => {
            messenger.close();
            Ok(json!({"client_closed": client.closed.load(Ordering::SeqCst)}))
        }
        "pending_notify_address" => {
            let variant = text_of(input, "variant");
            let runtime = match variant.as_str() {
                "runtime_direccion" => Some(Arc::new(FakeRuntime::with_default_prompt(
                    &json!({"pending": "t1:9"}),
                ))),
                "runtime_falla" => Some(Arc::new(FakeRuntime::with_default_prompt(
                    &json!({"pending": "raise"}),
                ))),
                _ => None,
            };
            let messenger = messenger_with(runtime, None, &[]);
            messenger.import_context(Vec::new(), "", &text_of(input, "text"));
            messenger.set_last_agent_address(field(input, "address").as_str());
            Ok(json!({"address": messenger.pending_notify_address()}))
        }
        _ => cancel_case(input, &messenger, &runtime),
    }
}

fn cancel_case(
    input: &Value,
    messenger: &ez_llm::messenger::Messenger,
    runtime: &Arc<FakeRuntime>,
) -> Result<Value, String> {
    let before = messenger.turn_generation();
    if field(input, "token").as_bool() != Some(true) {
        messenger.cancel_turn();
        return Ok(json!({"generation_delta": messenger.turn_generation() - before}));
    }
    let token = Arc::new(TurnToken::new());
    let closed = Arc::new(Mutex::new(false));
    let stream = ScriptedStream {
        chunks: VecDeque::new(),
        boom: None,
        closed: closed.clone(),
    };
    messenger.install_cancel_target(token.clone(), Box::new(stream));
    messenger.cancel_turn();
    Ok(json!({
        "cancelled": token.is_cancelled(),
        "generation_delta": messenger.turn_generation() - before,
        "stream_closed": *closed.lock().expect("cierre"),
        "runtime_cancelled": runtime.cancelled.load(Ordering::SeqCst),
    }))
}

fn deferred_case(case: &FixtureCase, function: &str) -> Result<Value, String> {
    let messenger = default_messenger();
    match function {
        "take_pending_notes" => {
            messenger.set_pending_notes(vec![
                json!({"role": "user", "content": "uno"}),
                json!({"role": "user", "content": "dos"}),
            ]);
            let taken = messenger.take_pending_notes();
            Ok(json!({"taken": taken, "remaining": messenger.pending_notes()}))
        }
        "check_cancel" => {
            let token = Arc::new(TurnToken::new());
            if field(&case.input, "cancelled").as_bool() == Some(true) {
                token.cancel();
                return messenger
                    .check_cancel(&token)
                    .map_err(|error| format!("TurnCancelled: {error}"))
                    .map(|_| Value::Null);
            }
            messenger.check_cancel(&token).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        _ => apply_deferred(case),
    }
}

fn apply_deferred(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let messenger = default_messenger();
    let messages = field(input, "messages")
        .as_array()
        .cloned()
        .unwrap_or_default();
    messenger.set_messages(messages);
    if field(input, "summary").as_str() != Some("resumen") {
        messenger.apply_deferred_history();
        return Ok(json!({
            "messages": messenger.messages(),
            "deferred": messenger.deferred_history().map(|(m, s, u)| json!([m, s, u])),
        }));
    }
    messenger.set_turn_active(true);
    messenger.import_context(
        vec![json!({"role": "user", "content": "nuevo"})],
        "resumen",
        "nuevo usuario",
    );
    messenger.set_turn_active(false);
    messenger.apply_deferred_history();
    Ok(json!({
        "messages": messenger.messages(),
        "summary": messenger.summary(),
        "last_user": messenger.export_context().2,
        "pending": messenger.pending_notes(),
        "deferred": messenger.deferred_history().map(|(m, s, u)| json!([m, s, u])),
    }))
}

#[test]
fn every_llm_tools_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de llm_tools legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        match (produced(case), &case.error) {
            (Ok(value), None) => assert_eq!(value, case.output, "caso {}", case.case),
            (Err(error), Some(expected)) => assert_eq!(&error, expected, "caso {}", case.case),
            (Ok(value), Some(expected)) => panic!(
                "caso {}: Python falla con {expected:?} y Rust devolvió {value}",
                case.case
            ),
            (Err(error), None) => panic!("caso {}: fallo inesperado: {error}", case.case),
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 219, "muy pocos casos: {cases_run}");
}
