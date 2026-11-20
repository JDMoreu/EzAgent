//! Dobles del tablero del enjambre para los fallos de retención (DX-4b).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Map};

use super::super::{HeldSwarm, SwarmHold};
use super::{fixture_with_hc, wait_until, FakeHc};
use crate::app::states::tests::support::Fixture;

/// Tablero del enjambre con guion: `live` o `pause` pueden fallar y cuenta lo
/// que se le pide (arrancar, reanudar, detener), para demostrar el fail-closed.
struct FakeHold {
    live_result: Mutex<Result<Vec<HeldSwarm>, String>>,
    pause_result: Mutex<Result<(), String>>,
    paused: Mutex<Vec<String>>,
    resumed: Mutex<Vec<String>>,
    stopped: Mutex<Vec<String>>,
    started: AtomicUsize,
    server_started: AtomicUsize,
    pokes: AtomicUsize,
}

impl FakeHold {
    fn new(live: Result<Vec<HeldSwarm>, String>) -> Arc<Self> {
        Arc::new(Self {
            live_result: Mutex::new(live),
            pause_result: Mutex::new(Ok(())),
            paused: Mutex::new(Vec::new()),
            resumed: Mutex::new(Vec::new()),
            stopped: Mutex::new(Vec::new()),
            started: AtomicUsize::new(0),
            server_started: AtomicUsize::new(0),
            pokes: AtomicUsize::new(0),
        })
    }

    fn with_pause_error(live: Result<Vec<HeldSwarm>, String>, error: &str) -> Arc<Self> {
        let hold = Self::new(live);
        *hold.pause_result.lock() = Err(error.to_string());
        hold
    }

    fn paused(&self) -> Vec<String> {
        self.paused.lock().clone()
    }

    fn resumed(&self) -> Vec<String> {
        self.resumed.lock().clone()
    }

    fn stopped(&self) -> Vec<String> {
        self.stopped.lock().clone()
    }

    fn started(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }

    fn server_started(&self) -> usize {
        self.server_started.load(Ordering::SeqCst)
    }

    fn pokes(&self) -> usize {
        self.pokes.load(Ordering::SeqCst)
    }
}

impl SwarmHold for FakeHold {
    fn live(&self) -> Result<Vec<HeldSwarm>, String> {
        self.live_result.lock().clone()
    }

    fn pause(&self, id: &str) -> Result<(), String> {
        let result = self.pause_result.lock().clone();
        match result {
            Ok(()) => {
                self.paused.lock().push(id.to_string());
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn resume(&self, id: &str) -> Result<(), String> {
        self.resumed.lock().push(id.to_string());
        Ok(())
    }

    fn stop(&self, id: &str) -> Result<(), String> {
        self.stopped.lock().push(id.to_string());
        Ok(())
    }

    fn poke(&self) {
        self.pokes.fetch_add(1, Ordering::SeqCst);
    }

    fn start(&self) -> Result<(), String> {
        self.started.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn start_server(&self) -> Result<(), String> {
        self.server_started.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn fixture_with_hold(hold: Arc<FakeHold>) -> (Fixture, Arc<FakeHc>) {
    let (fixture, fake) = fixture_with_hc();
    *fixture.app.inner.agents.swarm_hold.lock() = Some(hold as Arc<dyn SwarmHold>);
    (fixture, fake)
}

/// Tarea con una cadena: si algo reanudara, `FakeHc` vería `hc run`/`hc send`.
fn task_with_chain(tasks: &ez_herdr::tasks::TaskRegistry) {
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t1"));
    spec.insert("agent".to_string(), json!("codex"));
    spec.insert("task".to_string(), json!("seguir con las pruebas"));
    spec.insert("background".to_string(), json!(true));
    tasks.add_chain(&spec).expect("cadena");
}

#[test]
fn a_broken_board_blocks_the_start_and_keeps_everything() {
    let hold = FakeHold::new(Err("No pude leer los enjambres guardados".to_string()));
    let (fixture, fake) = fixture_with_hold(Arc::clone(&hold));
    let tasks = &fixture.app.inner.agents.tasks;
    task_with_chain(tasks);
    fixture.app.start_pending_resume_with(true);
    assert_eq!(hold.started(), 0, "el supervisor no debe arrancar");
    assert_eq!(hold.server_started(), 1, "solo el servidor local");
    let pending = fixture
        .app
        .inner
        .agents
        .pending_resume
        .lock()
        .clone()
        .expect("retenido");
    assert!(pending.blocked.is_some());
    assert_eq!(pending.tasks.len(), 1);
    assert_eq!(pending.chains.len(), 1);
    assert!(fixture
        .app
        .inner
        .chat
        .chat
        .snapshot()
        .1
        .iter()
        .any(|message| message.text.contains("No reanudo nada")));
    assert!(fixture.app.answer_pending_resume("dale"));
    assert_eq!(hold.resumed().len(), 0);
    assert_eq!(fake.launches(), 0);
    assert_eq!(fake.sends(), 0);
    assert!(tasks.get("t1").is_some(), "la tarea no se pierde");
    assert_eq!(tasks.chains().len(), 1, "la cadena no se pierde");
    fixture.app.drain_workers(3.0);
}

#[test]
fn a_swarm_that_cannot_be_paused_blocks_the_start() {
    let active = HeldSwarm {
        id: "s1".to_string(),
        goal: "ordenar el repo".to_string(),
        was_paused: false,
    };
    let hold =
        FakeHold::with_pause_error(Ok(vec![active]), "No pude dejar el enjambre s1 en pausa");
    let (fixture, fake) = fixture_with_hold(Arc::clone(&hold));
    task_with_chain(&fixture.app.inner.agents.tasks);
    fixture.app.start_pending_resume_with(true);
    assert_eq!(hold.started(), 0, "el supervisor no debe arrancar");
    assert_eq!(hold.server_started(), 1, "solo el servidor local");
    assert!(hold.paused().is_empty());
    let pending = fixture
        .app
        .inner
        .agents
        .pending_resume
        .lock()
        .clone()
        .expect("retenido");
    assert!(pending.blocked.is_some());
    assert_eq!(pending.swarms.len(), 1);
    assert!(fixture.app.answer_pending_resume("sí"));
    assert_eq!(hold.resumed().len(), 0);
    assert_eq!(fake.launches(), 0);
    assert_eq!(fake.sends(), 0);
    assert!(fixture.app.answer_pending_resume("descártalo"));
    assert_eq!(hold.stopped(), vec!["s1".to_string()]);
    assert!(fixture.app.inner.agents.tasks.get("t1").is_none());
    fixture.app.drain_workers(3.0);
}

#[test]
fn the_yes_only_resumes_the_swarms_that_were_active() {
    let active = HeldSwarm {
        id: "a".to_string(),
        goal: "activo".to_string(),
        was_paused: false,
    };
    let paused = HeldSwarm {
        id: "p".to_string(),
        goal: "pausado".to_string(),
        was_paused: true,
    };
    let hold = FakeHold::new(Ok(vec![active, paused]));
    let (fixture, _fake) = fixture_with_hold(Arc::clone(&hold));
    fixture.app.start_pending_resume_with(true);
    assert_eq!(hold.started(), 1);
    assert_eq!(hold.server_started(), 0);
    assert_eq!(hold.paused(), vec!["a".to_string()]);
    assert!(fixture.app.answer_pending_resume("dale"));
    assert!(wait_until(|| hold.resumed() == vec!["a".to_string()]));
    assert_eq!(hold.pokes(), 1);
    fixture.app.drain_workers(3.0);
}
