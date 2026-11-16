//! Dobles del dominio `tools_herdr`: adaptador de Herdr y callbacks.

use std::sync::{Arc, Mutex};

use ez_agent::registry::ToolInfo;
use ez_agent::tools_herdr::args::{set_last_address, shared_state, HerdrCallbacks};
use ez_agent::tools_herdr::port::{HerdrPort, PortError};
use ez_herdr::herdr::{HerdrError, RunAgent};
use serde_json::{json, Map, Value};

use super::KNOWN;

type Calls = Arc<Mutex<Vec<Value>>>;

pub(super) fn record(calls: &Calls, value: Value) {
    calls.lock().expect("llamadas").push(value);
}

pub(super) fn raised(spec: &Value, key: &str) -> Option<PortError> {
    let value = spec.get(key)?;
    value.get("raise")?;
    let message = value["message"].as_str().unwrap_or("").to_string();
    Some(match value["raise"].as_str().unwrap_or("ValueError") {
        "HerdrError" => PortError::Herdr(HerdrError::Failed(message)),
        "HerdrNotFound" => PortError::Herdr(HerdrError::NotFound(message)),
        other => PortError::Unexpected(format!("{other}: {message}")),
    })
}

pub(super) struct FakePort {
    spec: Value,
    calls: Calls,
}

impl FakePort {
    pub(super) fn new(spec: &Value, calls: &Calls) -> Self {
        Self {
            spec: spec.clone(),
            calls: Arc::clone(calls),
        }
    }

    fn result(&self, key: &str, default: Value) -> Result<Value, PortError> {
        if let Some(error) = raised(&self.spec, key) {
            return Err(error);
        }
        Ok(self.spec.get(key).cloned().unwrap_or(default))
    }
}

pub(super) fn map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

impl HerdrPort for FakePort {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        record(&self.calls, json!({"kind": "resolve_agent", "name": name}));
        match self.spec.get("resolve").and_then(Value::as_object) {
            Some(mapping) => mapping
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_string),
            None => KNOWN.contains(&name).then(|| name.to_string()),
        }
    }

    fn label_for(&self, address: &str) -> String {
        record(
            &self.calls,
            json!({"kind": "label_for", "address": address}),
        );
        if self
            .spec
            .get("label_raise")
            .is_some_and(|value| value.is_object())
        {
            panic!("nombre roto");
        }
        if self.spec.get("label_raises") == Some(&json!(true)) {
            panic!("nombre roto");
        }
        self.spec["labels"]
            .get(address)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        record(&self.calls, json!({"kind": "list_agents"}));
        let value = self.result("list", json!([]))?;
        Ok(value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(map)
            .collect())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "get_agent", "address": address}),
        );
        Ok(map(self.result(
            "get",
            json!({"agent": "codex", "state": "done"}),
        )?))
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, PortError> {
        record(
            &self.calls,
            json!({"kind": "read_agent", "address": address, "lines": lines, "limit": 6000}),
        );
        if self.spec.get("read_error") == Some(&json!(true)) {
            return Err(PortError::Unexpected("OSError: terminal ilegible".into()));
        }
        Ok(self.spec["read"].as_str().unwrap_or("").to_string())
    }

    fn read_full(&self, address: &str) -> Result<String, PortError> {
        record(
            &self.calls,
            json!({"kind": "read_full", "address": address, "lines": 1500}),
        );
        if self.spec.get("read_full_error") == Some(&json!(true)) {
            return Err(PortError::Unexpected("OSError: terminal ilegible".into()));
        }
        Ok(self.spec["read_full"]
            .as_str()
            .or_else(|| self.spec["read"].as_str())
            .unwrap_or("")
            .to_string())
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "send_agent", "address": address, "text": text, "confirmed": confirmed}),
        );
        Ok(map(self.result(
            "send",
            json!({"ok": true, "address": address, "message": "Mensaje enviado."}),
        )?))
    }

    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        let has_on_ready = params.on_ready.is_some();
        let mut kwargs = json!({
            "cwd": params.cwd,
            "name": params.name,
            "confirmed": params.confirmed,
            "background": params.background,
            "gui": params.gui,
            "space": params.space,
        });
        if let Some(model) = params.model {
            kwargs["model"] = json!(model);
        }
        if let Some(effort) = params.effort {
            kwargs["effort"] = json!(effort);
        }
        record(
            &self.calls,
            json!({"kind": "run_agent", "agent": params.agent, "task": params.task,
                "kwargs": kwargs, "has_on_ready": has_on_ready}),
        );
        if self.spec.get("call_on_ready") == Some(&json!(true)) {
            if let Some(callback) = params.on_ready {
                callback(map(self.spec.get("on_ready").cloned().unwrap_or(json!({}))));
            }
        }
        Ok(map(self.result(
            "run",
            json!({"ok": true, "address": "t1", "terminal": "t1"}),
        )?))
    }

    fn open_terminal(
        &self,
        name: &str,
        cwd: Option<&str>,
        shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "open_terminal", "name": name, "cwd": cwd, "shell": shell,
                "space": space}),
        );
        Ok(map(self.result(
            "open_terminal",
            json!({"ok": true, "terminal": "t1", "address": "t1"}),
        )?))
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        record(&self.calls, json!({"kind": "agents_catalog"}));
        self.spec["catalog"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(map)
            .collect()
    }

    fn configure_agent(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Map<String, Value> {
        record(
            &self.calls,
            json!({"kind": "configure_agent", "address": address, "model": model,
                "effort": effort}),
        );
        map(self.spec.get("configure").cloned().unwrap_or(json!(
            {"ok": true, "address": address, "agent": "codex", "applied": []}
        )))
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "keys_agent", "address": address, "keys": keys}),
        );
        Ok(map(self.result(
            "keys",
            json!({"ok": true, "address": address, "keys": keys}),
        )?))
    }

    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "focus_agent", "address": address}),
        );
        Ok(map(self.result(
            "focus",
            json!({"ok": true, "address": address, "focused": true}),
        )?))
    }

    fn kill_agent(&self, address: &str, confirmed: bool) -> Result<Map<String, Value>, PortError> {
        record(
            &self.calls,
            json!({"kind": "kill_agent", "address": address, "confirmed": confirmed}),
        );
        Ok(map(self.result(
            "kill",
            json!({"ok": true, "address": address, "message": "Terminal terminado."}),
        )?))
    }

    fn save_context(&self, text: &str) -> Option<String> {
        record(&self.calls, json!({"kind": "save_context", "text": text}));
        self.spec
            .get("save_context")
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

pub(super) fn panic_if(spec: &Value, key: &str) {
    if spec.get(key).is_some_and(Value::is_object) {
        panic!("{}", spec[key]["message"].as_str().unwrap_or("x"));
    }
}

pub(super) fn callbacks(spec: &Value, calls: &Calls) -> HerdrCallbacks {
    let spec_watch = spec.clone();
    let calls_watch = Arc::clone(calls);
    let spec_close = spec.clone();
    let calls_close = Arc::clone(calls);
    let spec_mic = spec.clone();
    let calls_mic = Arc::clone(calls);
    let calls_summary = Arc::clone(calls);
    let spec_mute = spec.clone();
    let calls_mute = Arc::clone(calls);
    let spec_user = spec.clone();
    let calls_user = Arc::clone(calls);
    let calls_remember = Arc::clone(calls);
    let calls_enqueue = Arc::clone(calls);
    let calls_deliver = Arc::clone(calls);
    let calls_prewarm = Arc::clone(calls);
    let calls_failed = Arc::clone(calls);
    let summary = spec["summary"].as_str().unwrap_or("").to_string();
    HerdrCallbacks {
        watch: Arc::new(move |address| {
            record(&calls_watch, json!({"kind": "watch", "address": address}));
            panic_if(&spec_watch, "watch_raise");
        }),
        request_close: Arc::new(move || {
            record(&calls_close, json!({"kind": "request_close"}));
            panic_if(&spec_close, "close_raise");
        }),
        mic_mute: Arc::new(move |muted| {
            record(&calls_mic, json!({"kind": "mic_mute", "muted": muted}));
            panic_if(&spec_mic, "mic_raise");
        }),
        last_summary: Arc::new(move || {
            record(&calls_summary, json!({"kind": "last_summary"}));
            summary.clone()
        }),
        mute: Arc::new(move |minutes| {
            record(&calls_mute, json!({"kind": "mute", "minutes": minutes}));
            panic_if(&spec_mute, "mute_raise");
        }),
        get_last_user_text: Arc::new(move || {
            record(&calls_user, json!({"kind": "get_last_user_text"}));
            if spec_user
                .get("user_text_raise")
                .is_some_and(Value::is_object)
            {
                panic!("x");
            }
            match spec_user.get("user_text") {
                None => String::new(),
                Some(Value::Null) => "none".to_string(),
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
            }
        }),
        remember_task: Arc::new(move |address, agent, task| {
            record(
                &calls_remember,
                json!({"kind": "remember_task", "address": address,
                "agent": agent, "task": task}),
            );
        }),
        enqueue_chain: Arc::new(move |spec| {
            record(
                &calls_enqueue,
                json!({"kind": "enqueue_chain", "spec": spec}),
            );
        }),
        deliver_answer: Arc::new(move |address| {
            record(
                &calls_deliver,
                json!({"kind": "deliver_answer", "address": address}),
            );
        }),
        prewarm_chain: Arc::new(move |spec| {
            record(
                &calls_prewarm,
                json!({"kind": "prewarm_chain", "spec": spec}),
            );
        }),
        async_launch: spec["async_launch"] == json!(true),
        launch_failed: Arc::new(move |address, agent, error| {
            record(
                &calls_failed,
                json!({"kind": "launch_failed", "address": address,
                "agent": agent, "error": error}),
            );
        }),
        // Los fixtures son del Python: no hay nombres dudosos que preguntar.
        confirm_name: None,
    }
}

pub(super) fn resolver(input: &Value) -> Box<dyn Fn(&str) -> String> {
    let mapping = input.get("label_map").cloned().unwrap_or(Value::Null);
    match input["labels"].as_str().unwrap_or("") {
        "label" => {
            Box::new(move |address: &str| mapping[address].as_str().unwrap_or("").to_string())
        }
        "raise" => Box::new(|_| panic!("x")),
        _ => Box::new(|_| String::new()),
    }
}

pub(super) fn info_json(info: &ToolInfo) -> Value {
    json!({"level": info.level, "bulk": info.bulk, "always_confirm": info.always_confirm,
        "summary": info.summary})
}

pub(super) fn state(input: &Value) -> ez_agent::tools_herdr::args::SharedState {
    let state = shared_state();
    if let Some(address) = input.get("last_address").and_then(Value::as_str) {
        set_last_address(&state, Some(address.to_string()));
    }
    state
}
