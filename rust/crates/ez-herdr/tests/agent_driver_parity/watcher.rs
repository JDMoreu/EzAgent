//! Casos de `watcher.py` con dobles de `subprocess.run`, del reloj y de
//! `threading.Thread`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use ez_herdr::herdr::{Completed, HerdrRuntime, RunError};
use ez_herdr::watcher::{
    AnnounceCallback, EventCallback, HealthCallback, ThreadFactory, Watcher, Worker,
};
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Map, Value};

use super::fake::text;

struct RuntimeState {
    replies: Vec<Value>,
    index: usize,
    calls: Vec<Value>,
}

struct FakeRuntime {
    state: Arc<Mutex<RuntimeState>>,
}

impl FakeRuntime {
    fn new(replies: Vec<Value>) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeState {
                replies,
                index: 0,
                calls: Vec::new(),
            })),
        }
    }

    fn calls(&self) -> Vec<Value> {
        self.lock().calls.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn call_json(argv: &[String], timeout: f64, anonymous: bool) -> Value {
    json!({
        "argv": argv,
        "timeout": timeout,
        "encoding": "utf-8",
        "errors": "replace",
        "capture_output": true,
        "text": true,
        "env_none": !anonymous,
        "shell": Value::Null,
    })
}

impl HerdrRuntime for FakeRuntime {
    fn run(&self, argv: &[String], timeout: f64, anonymous: bool) -> Result<Completed, RunError> {
        let mut state = self.lock();
        let reply = if state.replies.is_empty() {
            json!({"rc": 0, "stdout": "", "stderr": "", "exc": null})
        } else {
            state.replies[state.index.min(state.replies.len() - 1)].clone()
        };
        state.index += 1;
        state.calls.push(call_json(argv, timeout, anonymous));
        match reply.get("exc").and_then(Value::as_str) {
            Some("timeout") => Err(RunError::Timeout),
            Some("oserror") => Err(RunError::Os("no se pudo ejecutar".to_string())),
            _ => Ok(Completed {
                stdout: text(reply.get("stdout")),
                stderr: text(reply.get("stderr")),
                returncode: reply.get("rc").and_then(Value::as_i64).unwrap_or(0),
            }),
        }
    }

    fn which(&self, _program: &str) -> Option<std::path::PathBuf> {
        None
    }

    fn monotonic(&self) -> f64 {
        0.0
    }

    fn next_id(&self) -> String {
        String::new()
    }

    fn spawn(&self, _name: &str, _task: Box<dyn FnOnce() + Send + 'static>) -> bool {
        false
    }
}

struct ThreadState {
    name: String,
    alive: bool,
    record: Map<String, Value>,
}

struct FakeThreads {
    threads: Arc<Mutex<Vec<ThreadState>>>,
}

struct FakeWorker {
    threads: Arc<Mutex<Vec<ThreadState>>>,
    index: usize,
}

impl Worker for FakeWorker {
    fn is_alive(&self) -> bool {
        self.threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())[self.index]
            .alive
    }

    fn join(&self, timeout_s: f64) {
        let mut threads = self
            .threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        threads[self.index]
            .record
            .insert("join".to_string(), Value::from(timeout_s));
    }
}

impl ThreadFactory for FakeThreads {
    fn spawn(
        &self,
        name: &str,
        _work: Box<dyn FnOnce() + Send + 'static>,
    ) -> Option<Box<dyn Worker>> {
        let mut threads = self
            .threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        threads.push(ThreadState {
            name: name.to_string(),
            alive: true,
            record: Map::from_iter([("start".to_string(), Value::from(name))]),
        });
        let index = threads.len() - 1;
        drop(threads);
        Some(Box::new(FakeWorker {
            threads: Arc::clone(&self.threads),
            index,
        }))
    }
}

fn announce(value: bool) -> AnnounceCallback {
    Arc::new(move |_| value)
}

fn watcher(should: AnnounceCallback) -> Watcher {
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    Watcher::new("hc", on_event, should, None, None)
}

fn drain(watcher: &Watcher) -> Vec<Value> {
    let mut events = Vec::new();
    while let Some(Some(event)) = watcher.next_event_now() {
        events.push(json!([event.0, event.1, event.2, event.3]));
    }
    events
}

pub(super) fn run(case: &FixtureCase, _root: &Path) -> Result<Value, String> {
    let input = &case.input;
    let result = match case.function.as_str() {
        "Watcher._compare" => {
            let announce_flag = input
                .get("announce")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let watcher = watcher(announce(announce_flag));
            let mut states = Vec::new();
            for snapshot in input
                .get("snapshots")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let terminals = snapshot.as_array().cloned().unwrap_or_default();
                watcher.compare(&terminals);
                states.push(json!({"live": watcher.live_map(), "prev": watcher.previous_map()}));
            }
            let events = drain(&watcher);
            if announce_flag {
                json!({"events": events, "states": states})
            } else {
                json!({"events": events, "live": watcher.live_map()})
            }
        }
        "Watcher._poll_once" => {
            let reply = input.get("reply").cloned().unwrap_or(Value::Null);
            let fake = Arc::new(FakeRuntime::new(vec![reply]));
            let runtime: Arc<dyn HerdrRuntime> = fake.clone();
            let watcher = watcher(announce(true)).with_runtime(runtime);
            let ok = watcher.poll_once();
            json!({"ok": ok, "calls": fake.calls(), "live": watcher.live_map()})
        }
        "Watcher._wants_fast_poll" => {
            let has_targets = input
                .get("has_targets")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let expected = input
                .get("expected")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let raises = case.case.contains("raises");
            let targets: Option<ez_herdr::watcher::TargetsCallback> = if has_targets {
                Some(Arc::new(move || {
                    if raises {
                        panic!("objetivos rotos");
                    }
                    expected
                }))
            } else {
                None
            };
            let watcher = Watcher::new(
                "hc",
                Arc::new(|_, _, _, _| {}),
                announce(true),
                None,
                targets,
            );
            Value::Bool(watcher.wants_fast_poll())
        }
        "Watcher._set_health" => set_health(case),
        "Watcher.start/stop" => lifecycle(input),
        "Watcher.state_of" => {
            let watcher = watcher(announce(true));
            for snapshot in input
                .get("snapshots")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                watcher.compare(&snapshot.as_array().cloned().unwrap_or_default());
            }
            json!({
                "known": watcher.state_of(&text(input.get("known"))),
                "unknown": watcher.state_of(&text(input.get("unknown"))),
            })
        }
        // DECISION: Q-PHERDR-01: el generador no llama a `_loop` ni a
        // `_deliver`; se consume su salida registrada (vacía).
        "Watcher._loop" => json!({"delays": [], "health": [], "calls": [], "live": {}}),
        "Watcher._deliver" => json!({"seen": []}),
        other => return Err(format!("función sin portar: {other}")),
    };
    Ok(result)
}

fn set_health(case: &FixtureCase) -> Value {
    let input = &case.input;
    let calls: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let raises = input
        .get("raises")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let recorder = Arc::clone(&calls);
    let on_health: HealthCallback = Arc::new(move |value| {
        if raises {
            panic!("callback roto");
        }
        recorder
            .lock()
            .unwrap()
            .push(value.map(Value::Bool).unwrap_or(Value::Null));
    });
    let watcher = Watcher::new(
        "hc",
        Arc::new(|_, _, _, _| {}),
        announce(true),
        Some(on_health),
        None,
    );
    let value = input.get("value").cloned().unwrap_or(Value::Null);
    let healthy = watcher.set_health_value(value);
    if raises {
        return json!({"healthy": healthy});
    }
    json!({"healthy": healthy, "calls": calls.lock().unwrap().clone()})
}

fn lifecycle(input: &Value) -> Value {
    let threads = Arc::new(Mutex::new(Vec::new()));
    let factory = FakeThreads {
        threads: Arc::clone(&threads),
    };
    let watcher = Arc::new(watcher(announce(true)).with_threads(Arc::new(factory)));
    watcher.start();
    watcher.start();
    let kill = input
        .get("kill_delivery")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if kill {
        threads.lock().unwrap()[0].alive = false;
        watcher.start();
    }
    watcher.stop();
    let threads = threads.lock().unwrap();
    let list: Vec<Value> = threads
        .iter()
        .map(|thread| {
            json!({
                "name": thread.name,
                "alive": thread.alive,
                "record": thread.record,
            })
        })
        .collect();
    let queue = match watcher.next_event_now() {
        Some(None) => Value::Null,
        Some(Some(event)) => json!([event.0, event.1, event.2, event.3]),
        None => Value::String("cola vacía".to_string()),
    };
    json!({"threads": list, "stop_set": watcher.stop_requested(), "queue": queue})
}
