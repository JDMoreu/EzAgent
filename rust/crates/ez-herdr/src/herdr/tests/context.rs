//! Contexto de Herdr para el prompt: acotado, refrescado fuera del hilo y sin
//! lanzar procesos duplicados (TMAP-3).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{Map, Value};

use super::super::runtime::{HerdrRuntime, SpawnTask};
use super::super::*;

#[derive(Default)]
struct Control {
    agents: Option<Vec<Map<String, Value>>>,
    fail: bool,
    calls: AtomicUsize,
    spawned: AtomicUsize,
    now: AtomicU64,
    tasks: Mutex<Vec<SpawnTask>>,
}

impl Control {
    fn with_agents(count: usize) -> Self {
        let agents = (0..count)
            .map(|index| {
                let mut item = Map::new();
                item.insert("address".to_string(), Value::from(format!("a{index}")));
                item.insert("cwd".to_string(), Value::from("C:/project"));
                item.insert("agent".to_string(), Value::from("codex"));
                item.insert("state".to_string(), Value::from("working"));
                item.insert("label".to_string(), Value::from("x".repeat(1000)));
                item
            })
            .collect();
        Self {
            agents: Some(agents),
            ..Self::default()
        }
    }

    fn run_tasks(&self) {
        let tasks: Vec<SpawnTask> = std::mem::take(&mut *self.tasks.lock().expect("tareas"));
        for task in tasks {
            task();
        }
    }
}

struct FakeRuntime {
    control: Arc<Control>,
}

impl HerdrRuntime for FakeRuntime {
    fn run(
        &self,
        _argv: &[String],
        _timeout: f64,
        _anonymous: bool,
    ) -> Result<Completed, RunError> {
        self.control.calls.fetch_add(1, Ordering::SeqCst);
        Err(RunError::Os("sin herdr".to_string()))
    }

    fn which(&self, _program: &str) -> Option<PathBuf> {
        None
    }

    fn monotonic(&self) -> f64 {
        f64::from_bits(self.control.now.load(Ordering::SeqCst))
    }

    fn next_id(&self) -> String {
        "id".to_string()
    }

    fn spawn(&self, _name: &str, task: SpawnTask) -> bool {
        self.control.spawned.fetch_add(1, Ordering::SeqCst);
        self.control.tasks.lock().expect("tareas").push(task);
        true
    }

    fn agents(&self) -> Option<Vec<Map<String, Value>>> {
        if self.control.fail {
            return None;
        }
        self.control.agents.clone()
    }
}

fn herdr(control: &Arc<Control>) -> Arc<Herdr> {
    Arc::new(Herdr::with_runtime(
        VoiceConfig::new(),
        Arc::new(FakeRuntime {
            control: Arc::clone(control),
        }),
    ))
}

#[test]
fn context_is_bounded_and_the_first_snapshot_is_not_awaited() {
    let control = Arc::new(Control::with_agents(30));
    let herdr = herdr(&control);
    assert_eq!(herdr.context_text(), "");
    control.run_tasks();
    let context = herdr.context_text();
    assert!(context.contains("a0"), "{context}");
    assert!(context.contains("C:/project"), "{context}");
    assert!(context.contains("\"total\": 30"), "{context}");
    assert!(!context.contains("\"address\": \"a12\""), "{context}");
    assert!(ez_core::pyvalue::char_len(&context) < 5000);
    assert_eq!(context.matches("</untrusted_agent_windows>").count(), 1);
}

#[test]
fn an_inflight_refresh_does_not_block_or_duplicate_processes() {
    let control = Arc::new(Control::with_agents(1));
    let herdr = herdr(&control);
    assert_eq!(herdr.context_text(), "");
    for _ in 0..20 {
        assert_eq!(herdr.context_text(), "");
    }
    assert_eq!(control.spawned.load(Ordering::SeqCst), 1);
    assert!(herdr.context_pending());
    control.run_tasks();
    let context = herdr.context_text();
    assert!(context.contains("\"address\": \"a0\""), "{context}");
}

#[test]
fn a_failed_refresh_discards_the_context_and_backs_off() {
    let control = Arc::new(Control {
        agents: None,
        fail: true,
        ..Control::default()
    });
    let herdr = herdr(&control);
    assert_eq!(herdr.context_text(), "");
    control.run_tasks();
    assert_eq!(herdr.context_text(), "");
    assert_eq!(control.calls.load(Ordering::SeqCst), 1);
}
