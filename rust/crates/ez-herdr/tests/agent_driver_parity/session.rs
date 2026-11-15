//! Casos de `agent_session.py` con el doble de sesión y un reloj que avanza.

use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::AgentLaunch;
use ez_herdr::session::{
    command_line, fill, is_shell, session_steps, valid_keys_value, AgentSession, FillValues,
};
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Map, Value};

use super::fake::{argv, number_or_zero, session_output, text, FakeHost, TestClock};

/// `_config()` del generador: valores por defecto + perfiles del caso.
pub(super) fn config(input: &Value) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hc_path", json!("hc"));
    cfg.set("herdr_app_path", json!("herdr-app"));
    if let Some(profiles) = input.get("profiles") {
        cfg.set("agent_profiles", profiles.clone());
    }
    cfg
}

pub(super) fn values(input: Option<&Value>) -> FillValues {
    let map = input.and_then(Value::as_object);
    FillValues::new(
        &map.map(|map| text(map.get("model"))).unwrap_or_default(),
        &map.map(|map| text(map.get("effort"))).unwrap_or_default(),
    )
}

fn step_map(input: &Value) -> Map<String, Value> {
    input
        .get("step")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn pair(value: (String, String)) -> Value {
    json!([value.0, value.1])
}

fn session(input: &Value) -> (AgentSession<FakeHost>, FakeHost, TestClock) {
    // DECISION: los casos de una sola pantalla traen `screen` (no `screens`);
    // el generador construye el doble con una lista de un elemento.
    let seed = match input.get("screen") {
        Some(screen) if input.get("screens").is_none() => json!({"screens": [screen]}),
        _ => input.clone(),
    };
    let host = FakeHost::new(&seed);
    let clock = TestClock::new();
    let clock_trait: Arc<dyn ez_herdr::session::SessionClock> = Arc::new(clock.clone());
    let cfg = config(input);
    (
        AgentSession::with_clock(host.clone(), cfg, clock_trait),
        host,
        clock,
    )
}

fn launch_from(input: &Value) -> AgentLaunch {
    let launch = input.get("launch").cloned().unwrap_or(Value::Null);
    let map = launch.as_object();
    AgentLaunch {
        argv: map
            .and_then(|map| map.get("argv"))
            .map(|value| argv(Some(value))),
        warnings: map
            .and_then(|map| map.get("warnings"))
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|item| text(Some(item))).collect())
            .unwrap_or_default(),
        pending: map
            .and_then(|map| map.get("pending"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        env: Map::new(),
        error: String::new(),
    }
}

fn triple(value: Value) -> (Vec<String>, Vec<String>, String) {
    let items = value.as_array().cloned().unwrap_or_default();
    let strings = |index: usize| {
        items
            .get(index)
            .and_then(Value::as_array)
            .map(|list| list.iter().map(|item| text(Some(item))).collect())
            .unwrap_or_default()
    };
    (
        strings(0),
        strings(1),
        items
            .get(2)
            .map(|item| text(Some(item)))
            .unwrap_or_default(),
    )
}

pub(super) fn run(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let result = match case.function.as_str() {
        "valid_keys" => {
            json!({"keys": valid_keys_value(input.get("keys").unwrap_or(&Value::Null))})
        }
        "fill" => json!({
            "text": fill(
                &text(input.get("text")),
                &values(input.get("values")),
                input.get("escape").and_then(Value::as_bool).unwrap_or(false),
            ),
        }),
        "command_line" => {
            let env = input
                .get("env")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let path = input
                .get("path_prepend")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| text(Some(item)))
                        .collect::<Vec<_>>()
                });
            json!({"line": command_line(&argv(input.get("argv")), &env, path.as_deref())})
        }
        "_is_shell" => json!({"is_shell": is_shell(&text(input.get("name")))}),
        "AgentSession.wait_shell_prompt" => {
            let (s, host, clock) = session(input);
            session_output(Value::Bool(s.wait_shell_prompt("t1")), &host, &clock)
        }
        "AgentSession.wait_ready" => {
            let (s, host, clock) = session(input);
            let timeout = number_or_zero(input.get("timeout"));
            session_output(Value::Bool(s.wait_ready("t1", timeout)), &host, &clock)
        }
        "AgentSession._screen_shows_program" => {
            let (s, host, clock) = session(input);
            session_output(Value::Bool(s.screen_shows_program("t1")), &host, &clock)
        }
        "AgentSession._try_screen" => {
            let (s, host, clock) = session(input);
            let value = json!({"screen": s.try_screen("t1"), "safe": s.screen("t1")});
            session_output(value, &host, &clock)
        }
        "AgentSession._session_steps" => {
            let cfg = config(input);
            let steps = session_steps(&cfg, &text(input.get("profile")), &text(input.get("kind")));
            json!({"steps": Value::Array(steps.into_iter().map(Value::Object).collect())})
        }
        "AgentSession._check_screen" => {
            let (s, host, clock) = session(input);
            let value = s.check_screen("t1", &step_map(input), &values(input.get("values")));
            session_output(Value::String(value), &host, &clock)
        }
        "AgentSession._cycle" => {
            let (s, host, clock) = session(input);
            let value = s.cycle("t1", &step_map(input), &values(input.get("values")));
            session_output(pair(value), &host, &clock)
        }
        "AgentSession._run_step" => {
            let (s, host, clock) = session(input);
            let value = s.run_step("t1", &step_map(input), &values(input.get("values")));
            session_output(pair(value), &host, &clock)
        }
        "AgentSession._apply_in_session" => {
            let (s, host, clock) = session(input);
            let (applied, warnings, fatal) = s.apply_in_session(
                "t1",
                &text(input.get("profile")),
                &values(input.get("values")),
            );
            session_output(json!([applied, warnings, fatal]), &host, &clock)
        }
        "AgentSession.blocking_dialog" => {
            let (s, host, clock) = session(input);
            session_output(Value::Bool(s.blocking_dialog("t1")), &host, &clock)
        }
        "AgentSession._unsent_in_input" => {
            let task = text(input.get("task"));
            let value =
                AgentSession::<FakeHost>::unsent_in_input(&text(input.get("screen")), &task);
            json!({"unsent": value})
        }
        "AgentSession.confirm_submitted" => {
            let (s, host, clock) = session(input);
            let value = s.confirm_submitted("t1", "haz algo importante");
            session_output(Value::Bool(value), &host, &clock)
        }
        "AgentSession.send_task" => {
            let send = input.get("send").cloned().unwrap_or(json!({"ok": true}));
            let host = FakeHost::new(&json!({"sends": [send]}));
            let clock = TestClock::new();
            let clock_trait: Arc<dyn ez_herdr::session::SessionClock> = Arc::new(clock.clone());
            let s = AgentSession::with_clock(host.clone(), config(input), clock_trait);
            let confirmed = input
                .get("confirmed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let value = s.send_task_with("t1", "tarea", confirmed);
            session_output(Value::Object(value), &host, &clock)
        }
        "AgentSession.finish_launch" => finish_launch(input),
        "AgentSession.configure_agent" => configure_agent(input),
        other => return Err(format!("función sin portar: {other}")),
    };
    Ok(result)
}

fn finish_launch(input: &Value) -> Value {
    let outcome = input.get("outcome").cloned().unwrap_or(json!({}));
    let launch = launch_from(input);
    let task = text(input.get("task"));
    let calls: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let apply_calls = Arc::clone(&calls);
    let send_calls = Arc::clone(&calls);
    let send_result = outcome.get("send").cloned().unwrap_or(json!({"ok": true}));
    let mut apply_result = triple(outcome.get("apply").cloned().unwrap_or(json!([[], [], ""])));
    // DECISION: el oráculo captura el `apply` después de que `finish_launch`
    // appende el aviso del envío a esa misma lista (Q-PDRIVER-03); se deshace
    // ese append para partir del valor que recibió el Python.
    let send_warning = send_effect(&send_result);
    if !send_warning.is_empty() && apply_result.1.last() == Some(&send_warning) {
        apply_result.1.pop();
    }
    let s = AgentSession::new(FakeHost::new(input), VoiceConfig::new());
    let value = s.finish_launch_with(
        "t1",
        "claude",
        &launch,
        &task,
        outcome
            .get("ready")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        outcome
            .get("blocking")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        move |pending: &Map<String, Value>| {
            apply_calls.lock().expect("llamadas").push(json!({
                "kind": "apply",
                "address": "t1",
                "key": "claude",
                "values": pending,
            }));
            apply_result.clone()
        },
        move |task: &str| {
            send_calls.lock().expect("llamadas").push(json!({
                "kind": "send_task",
                "address": "t1",
                "task": task,
            }));
            send_result.as_object().cloned().unwrap_or_default()
        },
    );
    json!({"value": value, "calls": calls.lock().expect("llamadas").clone(), "delays": []})
}

/// Aviso que `finish_launch` agrega a los warnings según el resultado del envío.
fn send_effect(sent: &Value) -> String {
    use ez_core::pyvalue::{python_str, truthy};
    if !sent.get("ok").is_some_and(truthy) {
        return format!(
            "No pude enviar la tarea: {}",
            python_str(sent.get("error").unwrap_or(&Value::Null))
        );
    }
    match sent.get("warning").filter(|value| truthy(value)) {
        Some(warning) => python_str(warning),
        None => String::new(),
    }
}

fn configure_agent(input: &Value) -> Value {
    let agent = input.get("agent").cloned().unwrap_or(json!({}));
    let host = FakeHost::new(&json!({"agents": [agent]}));
    let s = AgentSession::new(host.clone(), config(input));
    let wanted = input.get("values").cloned().unwrap_or(json!({}));
    let model = wanted.get("model").and_then(Value::as_str);
    let effort = wanted.get("effort").and_then(Value::as_str);
    let apply = triple(input.get("apply").cloned().unwrap_or(json!([[], [], ""])));
    let value = s.configure_agent_with("t1", model, effort, apply);
    json!({"value": value, "calls": host.calls(), "delays": []})
}
