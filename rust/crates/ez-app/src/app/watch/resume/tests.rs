//! Pruebas de la pregunta de arranque y de lo que queda retenido (DX-4).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::{Completed, Herdr, HerdrRuntime, RunError};
use ez_swarm::service::MemberSpec;
use ez_swarm::stack::SwarmStack;
use ez_swarm::supervisor::{LaunchRequest, SwarmHost};
use parking_lot::Mutex;
use serde_json::{json, Map, Value};

use super::*;
use crate::app::core::AppEvent;
use crate::app::events::turn_gen;
use crate::app::states::tests::support::Fixture;

fn config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    cfg
}

fn wait_until(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    check()
}

/// `hc` falso: `t1` ya terminó, `t2` es el destino de una cadena.
struct FakeHc {
    calls: Mutex<Vec<Vec<String>>>,
}

impl FakeHc {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
        })
    }

    fn launches(&self) -> usize {
        self.calls
            .lock()
            .iter()
            .filter(|argv| matches!(argv.get(1).map(String::as_str), Some("run") | Some("do")))
            .count()
    }

    fn sends(&self) -> usize {
        self.calls
            .lock()
            .iter()
            .filter(|argv| argv.get(1).map(String::as_str) == Some("send"))
            .count()
    }
}

impl HerdrRuntime for FakeHc {
    fn run(&self, argv: &[String], _timeout: f64, _anonymous: bool) -> Result<Completed, RunError> {
        self.calls.lock().push(argv.to_vec());
        let stdout = match argv.get(1).map(String::as_str) {
            Some("get") => json!({
                "status": {"state": "done"},
                "agent": "claude",
                "name": "claude1"
            }),
            Some("ls") => json!({"terminals": []}),
            Some("read") => json!("listo"),
            Some("do") => json!({"terminal": "t2", "tab": "pestaña 1", "space": "ws"}),
            Some("run") => json!({"address": "t2", "name": "codex1", "state": "live"}),
            _ => json!({"ok": true}),
        };
        Ok(Completed {
            stdout: stdout.to_string(),
            stderr: String::new(),
            returncode: 0,
        })
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        Some(PathBuf::from(program))
    }

    fn monotonic(&self) -> f64 {
        1000.0
    }

    fn next_id(&self) -> String {
        "abcd1234".to_string()
    }

    fn spawn(&self, _name: &str, _task: Box<dyn FnOnce() + Send + 'static>) -> bool {
        true
    }
}

fn fixture_with_hc() -> (Fixture, Arc<FakeHc>) {
    let fake = FakeHc::new();
    let cfg = config();
    let runtime: Arc<dyn HerdrRuntime> = fake.clone();
    let herdr = Arc::new(Herdr::with_runtime(cfg.clone(), runtime));
    let fixture = Fixture::with_setup(cfg, move |parts| {
        parts.herdr = Some(herdr);
    });
    (fixture, fake)
}

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

fn task_row(address: &str, agent: &str, task: &str) -> Map<String, Value> {
    json!({"address": address, "agent": agent, "task": task})
        .as_object()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn without_pending_things_nothing_is_asked() {
    let fixture = Fixture::with_config(config());
    fixture.app.begin_pending_resume();
    assert!(fixture.app.inner.agents.pending_resume.lock().is_none());
    assert!(fixture.app.inner.chat.chat.snapshot().1.is_empty());
    assert!(fixture.speaker.spoken.lock().is_empty());
}

#[test]
fn a_pending_task_is_asked_once_by_voice_and_chat() {
    let fixture = Fixture::with_config(config());
    fixture
        .app
        .inner
        .agents
        .tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    fixture.app.begin_pending_resume();
    let pending = fixture
        .app
        .inner
        .agents
        .pending_resume
        .lock()
        .clone()
        .expect("pregunta");
    assert_eq!(
        pending.text,
        "Quedó pendiente «arreglar la API» (Claude Code). ¿Quieres que lo reanude?"
    );
    let messages = fixture.app.inner.chat.chat.snapshot().1;
    assert_eq!(
        messages.last().map(|message| message.text.as_str()),
        Some(pending.text.as_str())
    );
    assert!(wait_until(|| {
        fixture
            .speaker
            .spoken
            .lock()
            .iter()
            .any(|line| line == &pending.text)
    }));
    // Nada se vigila ni se lanza hasta el «sí».
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
    let before = fixture.app.inner.chat.chat.snapshot().1.len();
    fixture.app.begin_pending_resume();
    assert_eq!(fixture.app.inner.chat.chat.snapshot().1.len(), before);
    fixture.app.drain_workers(2.0);
}

#[test]
fn no_keeps_the_task_and_does_not_ask_again() {
    let fixture = Fixture::with_config(config());
    let tasks = &fixture.app.inner.agents.tasks;
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    fixture.app.begin_pending_resume();
    fixture.app.drain_workers(2.0);
    let generation = turn_gen(&fixture.app);
    fixture
        .app
        .handle_event(AppEvent::Transcribed("no".to_string(), generation));
    assert!(fixture
        .app
        .inner
        .agents
        .pending_resume
        .lock()
        .as_ref()
        .is_some_and(|pending| pending.declined));
    assert!(tasks.get("t1").is_some());
    assert!(fixture.app.inner.runtime.workers.lock().is_empty());
    let notes = fixture.app.inner.chat.chat.snapshot().1.len();
    fixture.app.begin_pending_resume();
    assert_eq!(fixture.app.inner.chat.chat.snapshot().1.len(), notes);
}

#[test]
fn a_chain_only_launches_after_the_yes() {
    let (fixture, fake) = fixture_with_hc();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t1"));
    spec.insert("agent".to_string(), json!("codex"));
    spec.insert("task".to_string(), json!("seguir con las pruebas"));
    spec.insert("background".to_string(), json!(true));
    tasks.add_chain(&spec).expect("cadena");
    fixture.app.begin_pending_resume();
    fixture.app.drain_workers(2.0);
    assert_eq!(fake.launches(), 0);
    assert!(!fixture.app.inner.agents.watched.lock().contains("t1"));
    assert!(fixture.app.answer_pending_resume("dale, reanuda"));
    assert!(wait_until(|| fake.launches() > 0));
    fixture.app.drain_workers(3.0);
}

#[test]
fn a_declined_task_only_returns_with_the_resume_command() {
    let (fixture, fake) = fixture_with_hc();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    fixture.app.begin_pending_resume();
    fixture.app.drain_workers(2.0);
    assert!(fixture.app.answer_pending_resume("déjalo"));
    assert!(!fixture.app.answer_pending_resume("sí"));
    assert_eq!(fake.launches(), 0);
    assert!(fixture.app.answer_pending_resume("reanuda lo pendiente"));
    assert!(wait_until(|| tasks.get("t1").is_none()));
    fixture.app.drain_workers(3.0);
}

#[test]
fn discard_deletes_the_task_and_its_chain() {
    let fixture = Fixture::with_config(config());
    let tasks = &fixture.app.inner.agents.tasks;
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    let mut spec = Map::new();
    spec.insert("source".to_string(), json!("t1"));
    spec.insert("agent".to_string(), json!("codex"));
    tasks.add_chain(&spec).expect("cadena");
    fixture.app.begin_pending_resume();
    assert!(fixture.app.answer_pending_resume("descártalo"));
    assert!(tasks.get("t1").is_none());
    assert!(tasks.chains().is_empty());
    assert!(fixture.app.inner.agents.pending_resume.lock().is_none());
}

#[test]
fn the_chat_also_answers_the_startup_question() {
    let (fixture, fake) = fixture_with_hc();
    let tasks = &fixture.app.inner.agents.tasks;
    tasks
        .add("t1", "claude", "arreglar la API", None)
        .expect("tarea");
    fixture.app.begin_pending_resume();
    fixture.app.drain_workers(2.0);
    fixture.app.chat_send("dale");
    assert!(wait_until(|| tasks.get("t1").is_none()));
    assert_eq!(fake.launches(), 0);
    let messages = fixture.app.inner.chat.chat.snapshot().1;
    assert_eq!(
        messages.last().map(|message| message.text.as_str()),
        Some("dale")
    );
    fixture.app.drain_workers(3.0);
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

#[test]
fn the_question_names_everything_once() {
    let task = task_row("t1", "opencode", "arreglar la API");
    let chain = task_row("t1", "codex", "seguir");
    let swarm = HeldSwarm {
        id: "s1".to_string(),
        goal: "ordenar el repo".to_string(),
        was_paused: false,
    };
    assert_eq!(
        resume_question(std::slice::from_ref(&task), &[], &[]),
        "Quedó pendiente «arreglar la API» (OpenCode). ¿Quieres que lo reanude?"
    );
    assert_eq!(
        resume_question(&[task], &[chain], &[swarm]),
        "Quedaron pendientes «arreglar la API» (OpenCode), una cadena hacia Codex \
         y el enjambre «ordenar el repo». ¿Quieres que los reanude?"
    );
}

/// `SwarmHost` sin terminales: el enjambre no lanza nada en la prueba.
struct FakeSwarmHost;

impl SwarmHost for FakeSwarmHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(Vec::new())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, String> {
        Err(format!("No tengo la terminal {address}."))
    }

    fn read_agent(&self, _address: &str, _lines: i64) -> Result<String, String> {
        Ok(String::new())
    }

    fn send_task(&self, _address: &str, _line: &str) -> Result<Map<String, Value>, String> {
        Err("sin terminales".to_string())
    }

    fn run_agent(
        &self,
        _agent: &str,
        _task: &str,
        _request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        Err("la prueba no lanza agentes".to_string())
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        Some(name.to_string())
    }

    fn swarm_args(&self, _agent: &str) -> Vec<String> {
        Vec::new()
    }

    fn save_context(&self, _text: &str) -> Option<String> {
        None
    }

    fn default_cwd(&self) -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn agent_names(&self) -> Vec<String> {
        Vec::new()
    }
}

struct TempFolder(PathBuf);

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_saved_swarm_waits_paused_for_the_yes() {
    let base = std::env::temp_dir().join(format!(
        "ezagent-dx4-swarm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    let folder = TempFolder(base.clone());
    let slot: Arc<Mutex<Option<Arc<SwarmStack>>>> = Arc::new(Mutex::new(None));
    let slot_for = Arc::clone(&slot);
    let fixture = Fixture::with_setup(config(), move |parts| {
        parts.build_swarm = true;
        parts.swarm_factory = Some(Arc::new(move |_args: crate::app::core::SwarmArgs| {
            let stack = SwarmStack::new(
                &base,
                Arc::new(FakeSwarmHost),
                Arc::new(|_: &str| {}),
                None,
                "ezs",
            )
            .ok()?;
            let stack = Arc::new(stack);
            *slot_for.lock() = Some(Arc::clone(&stack));
            Some(stack)
        }));
    });
    let stack = slot.lock().clone().expect("enjambre montado");
    let members = [MemberSpec::new("dev1", "developer", "opencode")];
    let (id, _) = stack
        .service
        .start(
            "ordenar el repo",
            &members,
            &std::env::temp_dir().to_string_lossy(),
            None,
        )
        .expect("enjambre guardado");
    assert_eq!(
        stack
            .store()
            .get(&id)
            .expect("tablero")
            .map(|swarm| swarm.status),
        Some("active".to_string())
    );
    fixture.app.begin_pending_resume();
    assert_eq!(
        stack
            .store()
            .get(&id)
            .expect("tablero")
            .map(|swarm| swarm.status),
        Some("paused".to_string())
    );
    assert!(!stack.base_dir.join("endpoint.json").exists());
    assert!(fixture.app.answer_pending_resume("dale"));
    assert!(wait_until(|| {
        stack
            .store()
            .get(&id)
            .ok()
            .flatten()
            .map(|swarm| swarm.status)
            == Some("active".to_string())
    }));
    fixture.app.drain_workers(3.0);
    drop(stack);
    drop(fixture);
    drop(folder);
}
