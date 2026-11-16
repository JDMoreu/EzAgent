//! Pruebas de la fila del modo autónomo en la vista rápida (AUTO-UI, 10.19).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_agent::autonomous::{Mode, Summary, TaskState};
use ez_agent::runtime::{AgentRuntime, ConfirmationGate, RuntimeCallbacks};
use ez_core::config::VoiceConfig;
use parking_lot::Mutex;
use serde_json::{json, Value};

use super::super::wiring::{peek_row, AUTONOMOUS_KEY, STOP_QUESTION};
use super::Harness;
use crate::app::activity::activity_rows;
use crate::app::states::tests::support::Fixture;

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

fn summary(state: TaskState, pause_reason: Option<&str>) -> Summary {
    Summary {
        id: String::from("20261116-120000-abcd"),
        mode: Mode::Single,
        state,
        round: 3,
        max_rounds: 5,
        approvals: 2,
        clean_streak: 1,
        workdir: String::from(r"C:\proyecto"),
        accepted: false,
        pause_reason: pause_reason.map(str::to_string),
    }
}

#[test]
fn the_row_shows_the_round_the_state_and_the_reason() {
    for state in [
        TaskState::Preparing,
        TaskState::Running,
        TaskState::Verifying,
    ] {
        let current = summary(state, None);
        let (title, detail, active) = peek_row(&current).expect("la fila existe");
        assert_eq!(title, "Autónomo · ronda 3");
        assert_eq!(
            format!("{title} · {detail}"),
            current.describe(),
            "la fila dice el texto de `Summary::describe`"
        );
        assert_eq!(
            active,
            matches!(state, TaskState::Running | TaskState::Verifying)
        );
    }
    let paused = summary(TaskState::Paused, Some("el mismo problema vuelve"));
    let (title, detail, active) = peek_row(&paused).expect("la pausa también tiene fila");
    assert_eq!(title, "Autónomo · ronda 3");
    assert_eq!(detail, "en pausa: el mismo problema vuelve");
    assert!(!active, "en pausa no hay marca en la órbita");
    assert_eq!(format!("{title} · {detail}"), paused.describe());

    assert_eq!(peek_row(&summary(TaskState::Done, None)), None);
    assert_eq!(peek_row(&summary(TaskState::Stopped, None)), None);
}

struct TestGate {
    answer: bool,
    asked: Mutex<Vec<String>>,
}

impl ConfirmationGate for TestGate {
    fn ask(&self, summary: &str, _strong: bool, _require_number: Option<i64>) -> bool {
        self.asked.lock().push(summary.to_string());
        self.answer
    }

    fn pending(&self) -> bool {
        false
    }

    fn pending_token(&self) -> Option<String> {
        None
    }

    fn arm(&self, _token: Option<&str>) {}

    fn submit_text(&self, _text: &str, _token: Option<&str>) -> Option<bool> {
        None
    }

    fn submit_click(&self, _yes: bool) {}

    fn cancel(&self, _reason: &str) {}
}

/// Runtime de verdad para que la fila encuentre la `ConfirmationGate`; sus
/// carpetas viven en un directorio temporal.
fn runtime_with(gate: Arc<TestGate>) -> AgentRuntime {
    let directory = std::env::temp_dir().join(format!(
        "ezagent-auto-ui-{}-{}",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::SeqCst)
    ));
    let permissions = directory.join("permissions.toml");
    let runtime = AgentRuntime::new(
        VoiceConfig::new(),
        None,
        RuntimeCallbacks::default(),
        Some(&permissions),
    )
    .expect("runtime de prueba");
    runtime.set_confirm_gate(Some(gate as Arc<dyn ConfirmationGate>));
    runtime
}

fn fixture_with(harness: &Harness) -> Fixture {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    let fixture = Fixture::with_config(cfg);
    *fixture.app.inner.agents.autonomous.lock() = Some(Arc::clone(&harness.loop_));
    fixture
}

fn wait_asked(gate: &TestGate) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !gate.asked.lock().is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn the_row_asks_the_code_question_and_a_yes_stops_the_task() {
    let harness = Harness::new();
    harness.start();
    assert!(harness.wait_launch(1), "el ejecutor está en marcha");
    let fixture = fixture_with(&harness);
    let rows = activity_rows(&fixture.app);
    assert_eq!(rows[0].key, AUTONOMOUS_KEY, "{rows:?}");
    assert_eq!(rows[0].title, "Autónomo · ronda 1");
    assert!(rows[0].live, "la marca acompaña a la tarea en marcha");
    assert!(rows[0].marked(), "la órbita pinta la tarea activa");

    let gate = Arc::new(TestGate {
        answer: true,
        asked: Mutex::new(Vec::new()),
    });
    *fixture.app.inner.agents.agent.lock() = Some(runtime_with(Arc::clone(&gate)));
    fixture.app.open_activity(AUTONOMOUS_KEY);

    assert!(wait_asked(&gate), "la fila pidió confirmación");
    assert_eq!(gate.asked.lock().as_slice(), [STOP_QUESTION]);
    assert!(
        harness.wait_state(TaskState::Stopped),
        "el «sí» detuvo la tarea"
    );
    let rows = activity_rows(&fixture.app);
    assert!(
        rows.iter().all(|row| row.key != AUTONOMOUS_KEY),
        "la fila se fue al terminar: {rows:?}"
    );
}

#[test]
fn a_no_leaves_the_task_running() {
    let harness = Harness::new();
    harness.start();
    assert!(harness.wait_launch(1));
    let fixture = fixture_with(&harness);
    let gate = Arc::new(TestGate {
        answer: false,
        asked: Mutex::new(Vec::new()),
    });
    *fixture.app.inner.agents.agent.lock() = Some(runtime_with(Arc::clone(&gate)));
    fixture.app.open_activity(AUTONOMOUS_KEY);

    assert!(wait_asked(&gate), "la fila pidió confirmación");
    assert_eq!(gate.asked.lock().as_slice(), [STOP_QUESTION]);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        harness.loop_.status().map(|summary| summary.state),
        Some(TaskState::Running),
        "el «no» no detiene nada"
    );
}

#[test]
fn the_paused_row_keeps_the_reason_but_loses_the_orbit_mark() {
    let harness = Harness::new();
    harness.start();
    assert!(harness.wait_launch(1));
    harness.port.set("t1", "idle", "");
    for _ in 0..10 {
        harness.clock.advance(30.0);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        harness.wait_state(TaskState::Paused),
        "estado: {:?}",
        harness.loop_.status()
    );
    let fixture = fixture_with(&harness);
    let rows = activity_rows(&fixture.app);
    let row = rows
        .iter()
        .find(|row| row.key == AUTONOMOUS_KEY)
        .expect("la fila sigue en pausa");
    let reason = harness
        .loop_
        .status()
        .and_then(|summary| summary.pause_reason)
        .expect("motivo de la pausa");
    assert_eq!(row.title, "Autónomo · ronda 1");
    assert_eq!(row.agent, format!("en pausa: {reason}"));
    assert_eq!(row.state, ez_hud::paint::marks::AUTONOMOUS);
    assert!(!row.live, "en pausa no lleva marca en la órbita");
    assert!(!row.marked(), "la órbita no pinta una tarea en pausa");
}
