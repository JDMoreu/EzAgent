//! Pruebas del bucle del modo autónomo (AUTO-A3) con un `HerdrPort` falso y
//! un directorio temporal: nada de Herdr real, ni ventanas, ni esperas largas.

mod fake;
mod flow;
mod safety;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_agent::audit::AuditLog;
use ez_agent::autonomous::artifacts::{Encargo, TaskDir};
use ez_agent::autonomous::{AutonomousState, Criterion, Limits, Mode, StartSpec, TaskState};
use serde_json::{json, Value};

use crate::app::core::{AppEvent, Queue};

use super::{AutonomousLoop, TestClock};

pub(crate) use fake::FakePort;

const CLAUDE_BASH: &str = include_str!("../../../../ez-herdr/tests/permission/claude_bash.txt");
const CODEX_EXEC: &str = include_str!("../../../../ez-herdr/tests/permission/codex_exec.txt");
const CODEX_EDIT: &str = include_str!("../../../../ez-herdr/tests/permission/codex_edit.txt");

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

/// App en miniatura: bucle, puerto, reloj y carpetas temporales.
pub(crate) struct Harness {
    pub port: Arc<FakePort>,
    pub clock: Arc<TestClock>,
    pub loop_: Arc<AutonomousLoop>,
    pub events: Arc<Queue<AppEvent>>,
    pub root: PathBuf,
    pub workdir: PathBuf,
    pub audit: PathBuf,
}

impl Harness {
    pub(crate) fn new() -> Self {
        let root = temp_path("auto-root");
        let workdir = temp_path("auto-work");
        std::fs::create_dir_all(&root).expect("carpeta raíz");
        std::fs::create_dir_all(&workdir).expect("carpeta de trabajo");
        let port = FakePort::new();
        let clock = TestClock::new();
        let events = Queue::<AppEvent>::new(None);
        let audit = root.join("auditoria.jsonl");
        let loop_ = AutonomousLoop::new(
            Arc::clone(&port) as Arc<dyn ez_agent::tools_herdr::port::HerdrPort>,
            Arc::clone(&events),
            Arc::clone(&clock) as Arc<dyn super::Clock>,
            root.clone(),
        );
        loop_.set_audit(Arc::new(AuditLog::new(&audit)));
        Self {
            port,
            clock,
            loop_,
            events,
            root,
            workdir,
            audit,
        }
    }

    pub(crate) fn spec(&self) -> StartSpec {
        StartSpec {
            task: "arreglar la API".to_string(),
            done_criteria: vec![Criterion {
                text: "compila".to_string(),
                check: "cargo build".to_string(),
            }],
            agent: "claude".to_string(),
            space: "proyecto".to_string(),
            workdir: self.workdir.display().to_string(),
            terminal: None,
            verifier_agent: "claude".to_string(),
            max_rounds: 5,
            max_minutes: 120,
            max_approvals: 50,
        }
    }

    pub(crate) fn start(&self) -> String {
        self.loop_.start_task(&self.spec()).expect("arranca")
    }

    pub(crate) fn wait(&self, mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    pub(crate) fn wait_state(&self, wanted: TaskState) -> bool {
        self.wait(|| {
            self.loop_
                .status()
                .is_some_and(|summary| summary.state == wanted)
        })
    }

    pub(crate) fn wait_launch(&self, count: usize) -> bool {
        self.wait(|| self.port.launches().len() >= count)
    }

    pub(crate) fn wait_keys(&self, count: usize) -> bool {
        self.wait(|| self.port.keys_sent().len() >= count)
    }

    pub(crate) fn paused_reason(&self) -> Option<String> {
        self.loop_
            .status()
            .filter(|summary| summary.state == TaskState::Paused)
            .and_then(|summary| summary.pause_reason)
    }

    pub(crate) fn task_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    pub(crate) fn write_fin(&self, id: &str, round: i64) {
        let path = self
            .task_dir(id)
            .join("compartido")
            .join(format!("fin-{round}.md"));
        std::fs::write(path, "Terminé: compila.\n").expect("fin");
    }

    pub(crate) fn write_verdict(&self, id: &str, k: i64, verdict: Value) {
        let path = self
            .task_dir(id)
            .join(format!("verificacion-{k}"))
            .join("veredicto.json");
        std::fs::write(path, verdict.to_string()).expect("veredicto");
    }

    pub(crate) fn says(&self) -> Vec<String> {
        let mut texts = Vec::new();
        while let Some(event) = self.events.get_nowait() {
            if let AppEvent::Say(text) = event {
                texts.push(text);
            }
        }
        texts
    }

    pub(crate) fn audit_text(&self) -> String {
        std::fs::read_to_string(&self.audit).unwrap_or_default()
    }

    pub(crate) fn bitacora(&self, id: &str) -> String {
        std::fs::read_to_string(self.task_dir(id).join("privado").join("bitacora.jsonl"))
            .unwrap_or_default()
    }

    /// Crea una tarea guardada como si la app se hubiera cerrado a medias.
    pub(crate) fn seed_running(&self, id: &str) -> PathBuf {
        let now = 1_700_000_000.0;
        let state = AutonomousState::new(
            id,
            Mode::Single,
            &self.workdir.display().to_string(),
            now,
            Limits {
                max_rounds: 5,
                max_minutes: 120,
                max_approvals: 200,
            },
        );
        let dir = TaskDir::create(
            &self.root,
            &Encargo {
                task: "la tarea del reinicio".to_string(),
                space: "proyecto".to_string(),
                workdir: self.workdir.display().to_string(),
            },
            &[Criterion {
                text: "compila".to_string(),
                check: "cargo build".to_string(),
            }],
            &state,
        )
        .expect("artefactos");
        let mut saved = state;
        saved.state = TaskState::Running;
        let _ = saved.executor_started("t1", "claude");
        dir.write_state(&saved).expect("estado");
        dir.path().to_path_buf()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.loop_.shutdown();
    }
}

/// `AAAAMMDD-HHMMSS-<4 hex>` con la hora fijada (para el reinicio).
pub(crate) fn fixed_id() -> String {
    format!(
        "20261116-12{:02}{:02}-{:04x}",
        UNIQUE.fetch_add(1, Ordering::SeqCst) % 60,
        UNIQUE.load(Ordering::SeqCst) % 60,
        UNIQUE.load(Ordering::SeqCst) & 0xffff
    )
}

pub(crate) fn clean_verdict(token: &str, evidence: &str) -> Value {
    json!({
        "token": token,
        "cumple": true,
        "criterios": [{"n": 1, "cumple": true, "evidencia": evidence}],
        "hallazgos": [],
    })
}

pub(crate) fn dirty_verdict(token: &str, problem: &str) -> Value {
    json!({
        "token": token,
        "cumple": false,
        "criterios": [{"n": 1, "cumple": false, "evidencia": "cargo build falla"}],
        "hallazgos": [{
            "severidad": "bloqueante",
            "criterio": 1,
            "problema": problem,
            "evidencia": "error de compilación",
        }],
    })
}

pub(crate) fn token_of(task: &str) -> String {
    let marker = "Tu token es ";
    let Some(start) = task.find(marker).map(|at| at + marker.len()) else {
        return String::new();
    };
    task[start..].chars().take(16).collect()
}

pub(crate) fn temp_path(name: &str) -> PathBuf {
    let unique = UNIQUE.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("ezagent-a3-{}-{unique}-{name}", std::process::id()))
}
