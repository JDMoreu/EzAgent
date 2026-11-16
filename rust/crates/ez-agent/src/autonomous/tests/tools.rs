//! Las cuatro herramientas de 9.2 y sus guardas de activación.

use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::autonomous::state::{Mode, Summary, TaskState};
use crate::autonomous::texts;
use crate::autonomous::tools::{build_autonomous_tools, AUTHORIZE_RE};
use crate::autonomous::{AutonomousHost, StartSpec, TeamSpec, ToolGuards};
use crate::registry::{ToolResult, ToolReturn, ToolSpec};

#[derive(Default)]
struct FakeHost {
    summary: Mutex<Option<Summary>>,
    started: Mutex<Vec<StartSpec>>,
    teams: Mutex<Vec<TeamSpec>>,
    stopped: Mutex<Vec<String>>,
    resumed: Mutex<usize>,
}

impl AutonomousHost for FakeHost {
    fn start(&self, spec: &StartSpec) -> Result<String, String> {
        lock(&self.started).push(spec.clone());
        Ok("auto-1".to_string())
    }

    fn start_team(&self, spec: &TeamSpec) -> Result<String, String> {
        lock(&self.teams).push(spec.clone());
        Ok("auto-2".to_string())
    }

    fn status(&self) -> Option<Summary> {
        lock(&self.summary).clone()
    }

    fn resume(&self) -> Result<(), String> {
        *lock(&self.resumed) += 1;
        Ok(())
    }

    fn stop(&self, reason: &str) -> Result<(), String> {
        lock(&self.stopped).push(reason.to_string());
        Ok(())
    }
}

fn lock<T>(cell: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|error| error.into_inner())
}

struct FakeGuards {
    tainted: bool,
    last: String,
    answer: bool,
    asked: Mutex<Vec<String>>,
}

impl FakeGuards {
    fn allowed(last: &str) -> Arc<Self> {
        Arc::new(Self {
            tainted: false,
            last: last.to_string(),
            answer: true,
            asked: Mutex::new(Vec::new()),
        })
    }

    fn rejecting(last: &str) -> Arc<Self> {
        Arc::new(Self {
            tainted: true,
            last: last.to_string(),
            answer: true,
            asked: Mutex::new(Vec::new()),
        })
    }
}

impl ToolGuards for FakeGuards {
    fn tainted(&self) -> bool {
        self.tainted
    }

    fn last_user_text(&self) -> String {
        self.last.clone()
    }

    fn confirm(&self, summary: &str) -> bool {
        lock(&self.asked).push(summary.to_string());
        self.answer
    }
}

fn summary(state: TaskState) -> Summary {
    Summary {
        id: "auto-1".to_string(),
        mode: Mode::Single,
        state,
        round: 2,
        max_rounds: 5,
        approvals: 1,
        clean_streak: 1,
        pause_reason: None,
    }
}

fn build(host: Arc<FakeHost>, guards: Arc<FakeGuards>) -> Vec<ToolSpec> {
    build_autonomous_tools(
        Some(host as Arc<dyn AutonomousHost>),
        guards as Arc<dyn ToolGuards>,
    )
}

fn run(tool: &ToolSpec, args: Value) -> ToolResult {
    let map: Map<String, Value> = args.as_object().cloned().expect("objeto");
    match (tool.handler)(&map).expect("manejador") {
        ToolReturn::Result(result) => result,
        ToolReturn::Value(value) => panic!("resultado inesperado: {value}"),
    }
}

fn task_args() -> Value {
    json!({
        "task": "migrar la API",
        "done_criteria": ["compila", "pasan las pruebas"],
        "agent": "codex",
        "space": "proyecto"
    })
}

fn error_of(result: &ToolResult) -> &str {
    result.error.as_deref().unwrap_or_default()
}

fn default_host() -> Arc<FakeHost> {
    Arc::new(FakeHost::default())
}

#[test]
fn registers_four_tools_at_level_two() {
    let specs = build(default_host(), FakeGuards::allowed("modo autónomo"));
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "start_autonomous_task",
            "start_autonomous_team",
            "autonomous_status",
            "stop_autonomous_task"
        ]
    );
    assert!(specs.iter().all(|spec| spec.level == 2));
    assert!(build_autonomous_tools(None, FakeGuards::allowed("x")).is_empty());
}

#[test]
fn tainted_turn_rejects_activation() {
    let host = default_host();
    let specs = build(host, FakeGuards::rejecting("modo autónomo, autorizo"));
    let result = run(&specs[0], task_args());
    assert_eq!(error_of(&result), texts::TAINTED);
}

#[test]
fn missing_authorization_rejects_activation() {
    let host = default_host();
    let specs = build(host, FakeGuards::allowed("haz esa tarea"));
    let result = run(&specs[0], task_args());
    assert_eq!(error_of(&result), texts::NOT_AUTHORIZED);
}

#[test]
fn an_unfinished_task_blocks_another_one() {
    let host = Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(TaskState::Running))),
        ..FakeHost::default()
    });
    let specs = build(host, FakeGuards::allowed("modo autónomo, autorizo"));
    let result = run(&specs[0], task_args());
    assert_eq!(error_of(&result), texts::ALREADY_ACTIVE);
    // Una tarea cumplida no bloquea la siguiente.
    let done = Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(TaskState::Done))),
        ..FakeHost::default()
    });
    let result = run(
        &build(done, FakeGuards::allowed("autorizo"))[0],
        task_args(),
    );
    assert!(result.ok);
}

#[test]
fn a_no_in_the_confirmation_does_not_start() {
    let host = default_host();
    let guards = Arc::new(FakeGuards {
        tainted: false,
        last: "modo autónomo, autorizo".to_string(),
        answer: false,
        asked: Mutex::new(Vec::new()),
    });
    let result = run(&build(host, guards)[0], task_args());
    assert_eq!(error_of(&result), texts::DECLINED);
}

#[test]
fn all_guards_pass_and_yes_starts_the_task() {
    let host = default_host();
    let guards = FakeGuards::allowed("hazlo en modo autónomo, autorizo los permisos");
    let result = run(&build(Arc::clone(&host), guards)[0], task_args());
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["id"], "auto-1");
    assert_eq!(result.data["message"], texts::START);
    let started = lock(&host.started);
    assert_eq!(started.len(), 1);
    let spec = &started[0];
    assert_eq!(spec.task, "migrar la API");
    assert_eq!(spec.done_criteria.len(), 2);
    assert_eq!(spec.agent, "codex");
    assert_eq!(spec.space, "proyecto");
    assert_eq!(spec.verifier_agent, "codex");
    assert_eq!(spec.terminal, None);
    assert_eq!(spec.max_rounds, 5);
    assert_eq!(spec.max_minutes, 120);
}

#[test]
fn the_confirmation_text_uses_the_design_literals() {
    let host = default_host();
    let guards = FakeGuards::allowed("yolo");
    run(&build(host, Arc::clone(&guards))[0], task_args());
    let asked = lock(&guards.asked);
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0],
        texts::confirm("migrar la API", "codex", "proyecto", 5, 120)
    );
    assert!(asked[0].starts_with("Modo autónomo: haré «migrar la API» con codex"));
    assert!(asked[0].ends_with("¿Lo activo?"));
}

#[test]
fn optional_values_reach_the_spec() {
    let host = default_host();
    let guards = FakeGuards::allowed("autorizo el modo autónomo");
    let args = json!({
        "task": "migrar la API",
        "done_criteria": ["compila"],
        "agent": "codex",
        "space": "proyecto",
        "terminal": "t1",
        "verifier_agent": "claude",
        "max_rounds": 3,
        "max_minutes": 30
    });
    let result = run(&build(Arc::clone(&host), guards)[0], args);
    assert!(result.ok, "{:?}", result.error);
    let started = lock(&host.started);
    assert_eq!(started[0].terminal.as_deref(), Some("t1"));
    assert_eq!(started[0].verifier_agent, "claude");
    assert_eq!(started[0].max_rounds, 3);
    assert_eq!(started[0].max_minutes, 30);
}

#[test]
fn team_tool_starts_the_team() {
    let host = default_host();
    let guards = FakeGuards::allowed("modo autónomo");
    let args = json!({
        "task": "migrar la API",
        "done_criteria": ["compila"],
        "roles": [
            {"role": "plan", "agent": "codex"},
            {"role": "dev", "agent": "opencode", "model": "big"}
        ],
        "space": "proyecto"
    });
    let result = run(&build(Arc::clone(&host), guards)[1], args);
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["id"], "auto-2");
    let teams = lock(&host.teams);
    assert_eq!(teams[0].roles.len(), 2);
    assert_eq!(teams[0].roles[1].model.as_deref(), Some("big"));
}

#[test]
fn status_reports_the_task_or_its_absence() {
    let empty = build(default_host(), FakeGuards::allowed("x"));
    let result = run(&empty[2], json!({}));
    assert_eq!(result.data["active"], false);
    assert_eq!(result.data["message"], texts::NO_TASK);

    let host = Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(TaskState::Paused))),
        ..FakeHost::default()
    });
    let result = run(&build(host, FakeGuards::allowed("x"))[2], json!({}));
    assert_eq!(result.data["active"], true);
    assert_eq!(result.data["state"], "paused");
    assert_eq!(result.data["round"], 2);
    assert_eq!(result.data["message"], "Autónomo · ronda 2 · en pausa");
}

#[test]
fn stop_passes_the_reason_and_handles_the_empty_case() {
    let empty = build(default_host(), FakeGuards::allowed("x"));
    let result = run(&empty[3], json!({}));
    assert_eq!(result.data["stopped"], false);
    assert_eq!(result.data["message"], texts::NO_TASK);

    let host = Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(TaskState::Running))),
        ..FakeHost::default()
    });
    let result = run(
        &build(Arc::clone(&host), FakeGuards::allowed("x"))[3],
        json!({"reason": "por voz"}),
    );
    assert_eq!(result.data["stopped"], true);
    assert_eq!(result.data["message"], texts::STOPPED);
    assert_eq!(lock(&host.stopped)[0], "por voz");

    // Una tarea cumplida ya no se detiene.
    let done = Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(TaskState::Done))),
        ..FakeHost::default()
    });
    let result = run(
        &build(Arc::clone(&done), FakeGuards::allowed("x"))[3],
        json!({}),
    );
    assert_eq!(result.data["stopped"], false);
    assert!(lock(&done.stopped).is_empty());
}

#[test]
fn authorize_regex_covers_the_design_words() {
    let authorize = |text: &str| {
        regex::Regex::new(AUTHORIZE_RE)
            .expect("regex")
            .is_match(text)
    };
    assert!(authorize("hazlo en modo autonomo"));
    assert!(authorize("AUTORIZO los permisos"));
    assert!(authorize("yolo"));
    assert!(authorize("sin pedirme permiso"));
    assert!(authorize("hazlo sin preguntarme"));
    assert!(!authorize("haz esa tarea"));
}

#[test]
fn validation_rejects_empty_task_and_criteria() {
    let guards = FakeGuards::allowed("modo autónomo, autorizo");
    let specs = build(default_host(), guards);
    let mut args = task_args();
    args["task"] = json!("   ");
    assert!(!run(&specs[0], args).ok);
    let mut args = task_args();
    args["done_criteria"] = json!(["  "]);
    assert!(!run(&specs[0], args).ok);
}
