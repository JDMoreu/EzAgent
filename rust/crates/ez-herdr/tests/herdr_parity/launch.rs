//! Casos de `run_agent`: lanzamiento con el `AgentDriver` real y doble de sesión.

use std::path::Path;
use std::sync::{Arc, Mutex};

use ez_core::pyvalue::py_strip;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{Herdr, LaunchSupport, RunAgent};
use ez_herdr::session::SessionHost;
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Map, Value};

use super::harness::*;

#[derive(Clone)]
struct FakeHost;

impl SessionHost for FakeHost {
    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, ez_herdr::herdr::HerdrError> {
        Ok(Map::new())
    }

    fn send_agent(
        &self,
        _address: &str,
        _text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, ez_herdr::herdr::HerdrError> {
        Ok(Map::new())
    }

    fn keys_agent(
        &self,
        _address: &str,
        _keys: &[String],
    ) -> Result<Map<String, Value>, ez_herdr::herdr::HerdrError> {
        Ok(Map::new())
    }

    fn read_agent(
        &self,
        _address: &str,
        _lines: Option<i64>,
    ) -> Result<String, ez_herdr::herdr::HerdrError> {
        Ok(String::new())
    }
}

#[derive(Clone)]
struct Harness {
    driver: AgentDriver<FakeHost>,
    record: Arc<Mutex<Vec<Value>>>,
    outcome: Value,
}

impl LaunchSupport for Harness {
    fn agent_launch(
        &self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> ez_herdr::herdr::AgentLaunch {
        self.driver.agent_launch(agent, model, effort)
    }

    fn wait_shell_prompt(&self, address: &str) -> bool {
        self.record.lock().expect("registro").push(json!({
            "kind": "wait_shell_prompt",
            "address": address,
        }));
        true
    }

    fn finish_launch(
        &self,
        address: &str,
        key: &str,
        launch: &ez_herdr::herdr::AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        self.record.lock().expect("registro").push(json!({
            "kind": "finish_launch",
            "address": address,
            "key": key,
            "launch": {
                "argv": launch.argv,
                "warnings": launch.warnings,
                "pending": launch.pending,
                "env": launch.env,
            },
            "task": task,
        }));
        object(self.outcome.clone())
    }
}

pub(super) fn index_of(case: &str) -> String {
    case.rsplit('/')
        .next()
        .and_then(|name| name.split_once('-').map(|(index, _)| index))
        .unwrap_or("0")
        .to_string()
}

pub(super) fn send_calls(calls: &[Value]) -> Vec<Value> {
    calls
        .iter()
        .filter(|call| call["argv"][1] == "send")
        .map(|call| {
            json!({
                "kind": "send_agent",
                "address": call["argv"][2],
                "text": call["argv"][3],
                "confirmed": true,
            })
        })
        .collect()
}

pub(super) fn run_agent_case(
    world: &Arc<World>,
    case: &FixtureCase,
    temp: &str,
) -> Result<Value, String> {
    let input = &case.input;
    let root = Path::new(temp).join("run_agent").join(index_of(&case.case));
    std::fs::create_dir_all(&root).expect("carpeta del caso");
    std::env::set_var("LOCALAPPDATA", &root);
    let reply = input.get("reply").cloned().unwrap_or(json!({}));
    let mut replies =
        vec![json!({"rc": 0, "stdout": reply.to_string(), "stderr": "", "exc": null})];
    let send_fails = input
        .get("send")
        .and_then(|send| send.get("ok"))
        .and_then(Value::as_bool)
        == Some(false);
    if send_fails {
        replies.push(json!({
            "rc": 1,
            "stdout": "",
            "stderr": text(input.get("send").and_then(|send| send.get("error"))),
            "exc": null,
        }));
    }
    {
        let mut state = world.lock();
        state.replies = replies.iter().map(Reply::from).collect();
        state.index = 0;
    }
    let raw_task_text = text(input.get("task"));
    let task = py_strip(&raw_task_text);
    let kwargs = input.get("kwargs").cloned().unwrap_or(Value::Null);
    let gui = kwargs.get("gui").and_then(Value::as_bool).unwrap_or(false);
    let task_text = if task.is_empty() {
        String::new()
    } else if gui {
        format!("{}{task}", ez_herdr::herdr::GUI_INSTRUCTION)
    } else {
        task.to_string()
    };
    let outcome = input.get("outcome").cloned().unwrap_or(json!({
        "ok": true,
        "task_sent": !task_text.is_empty(),
        "warnings": [],
    }));
    let record = Arc::new(Mutex::new(Vec::new()));
    let ready = Arc::new(Mutex::new(Vec::new()));
    let voice_cfg = config(temp, input);
    let harness = Harness {
        driver: AgentDriver::new(FakeHost, voice_cfg),
        record: Arc::clone(&record),
        outcome,
    };
    let path_prepend = kwargs
        .get("path_prepend")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        });
    let extra_args = kwargs
        .get("extra_args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        });
    let env = kwargs.get("env").and_then(Value::as_object).cloned();
    let ready_sink = Arc::clone(&ready);
    let on_ready = kwargs.get("on_ready").is_some().then(|| {
        Box::new(move |value: Map<String, Value>| {
            ready_sink.lock().expect("ready").push(Value::Object(value));
        }) as ez_herdr::herdr::ReadyCallback
    });
    let agent = text(input.get("agent"));
    let raw_task = text(input.get("task"));
    let params = RunAgent {
        agent: &agent,
        task: &raw_task,
        cwd: None,
        name: kwargs.get("name").and_then(Value::as_str),
        confirmed: false,
        background: kwargs
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        gui,
        space: kwargs.get("space").and_then(Value::as_str),
        model: input.get("model").and_then(Value::as_str),
        effort: input.get("effort").and_then(Value::as_str),
        on_ready,
        env,
        path_prepend,
        extra_args,
        ..RunAgent::default()
    };
    let herdr = Arc::new(Herdr::with_runtime(config(temp, input), runtime_of(world)));
    let result = herdr
        .run_agent(harness, params)
        .map_err(|error| error.message().to_string())?;
    let mut agent_calls: Vec<Value> = Vec::new();
    let mut sends = send_calls(&world.calls());
    for entry in record.lock().expect("registro").iter() {
        if entry["kind"] == "finish_launch" {
            agent_calls.append(&mut sends);
        }
        agent_calls.push(entry.clone());
    }
    agent_calls.append(&mut sends);
    let calls: Vec<Value> = world
        .calls()
        .into_iter()
        .filter(|call| call["argv"][1] != "send")
        .collect();
    Ok(json!({
        "result": result,
        "calls": calls,
        "agent_calls": agent_calls,
        "ready": ready.lock().expect("ready").clone(),
        "threads": world.threads(),
        "files": files_under(&root),
    }))
}
