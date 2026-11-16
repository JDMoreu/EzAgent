//! Las cinco herramientas de 10.5 y sus guardas de activación.

use std::sync::{Arc, Mutex};

use ez_core::pytime::timestamp;
use serde_json::{json, Map, Value};

use crate::autonomous::state::{Mode, Summary, TaskState};
use crate::autonomous::texts;
use crate::autonomous::tools::{authorized, build_autonomous_tools, AutonomousDefaults};
use crate::autonomous::{AutonomousHost, StartSpec, TeamSpec, ToolGuards};
use crate::registry::{ToolResult, ToolReturn, ToolSpec};

#[derive(Default)]
struct FakeHost {
    summary: Mutex<Option<Summary>>,
    started: Mutex<Vec<StartSpec>>,
    teams: Mutex<Vec<TeamSpec>>,
    stopped: Mutex<Vec<(String, bool)>>,
    resumed: Mutex<Vec<(i64, i64)>>,
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

    fn resume(&self, extend_rounds: i64, extend_minutes: i64) -> Result<(), String> {
        lock(&self.resumed).push((extend_rounds, extend_minutes));
        Ok(())
    }

    fn stop(&self, reason: &str, accept: bool) -> Result<(), String> {
        lock(&self.stopped).push((reason.to_string(), accept));
        Ok(())
    }
}

fn lock<T>(cell: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|error| error.into_inner())
}

struct FakeGuards {
    tainted: bool,
    messages: Vec<(String, f64)>,
    answer: bool,
    default_cwd: String,
    defaults: AutonomousDefaults,
    asked: Mutex<Vec<(String, bool)>>,
}

impl FakeGuards {
    fn allowed(last: &str) -> Arc<Self> {
        Arc::new(Self {
            tainted: false,
            messages: vec![(last.to_string(), timestamp())],
            answer: true,
            default_cwd: String::new(),
            defaults: AutonomousDefaults::default(),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn rejecting(last: &str) -> Arc<Self> {
        Arc::new(Self {
            tainted: true,
            messages: vec![(last.to_string(), timestamp())],
            answer: true,
            default_cwd: String::new(),
            defaults: AutonomousDefaults::default(),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn with_default_cwd(self: Arc<Self>, workdir: &str) -> Arc<Self> {
        Arc::new(Self {
            default_cwd: workdir.to_string(),
            ..Self::clone_for(&self)
        })
    }

    fn clone_for(other: &Self) -> Self {
        Self {
            tainted: other.tainted,
            messages: other.messages.clone(),
            answer: other.answer,
            default_cwd: other.default_cwd.clone(),
            defaults: other.defaults,
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl ToolGuards for FakeGuards {
    fn tainted(&self) -> bool {
        self.tainted
    }

    fn recent_user_messages(&self) -> Vec<(String, f64)> {
        self.messages.clone()
    }

    fn confirm(&self, summary: &str, strong: bool) -> bool {
        lock(&self.asked).push((summary.to_string(), strong));
        self.answer
    }

    fn default_cwd(&self) -> String {
        self.default_cwd.clone()
    }

    fn defaults(&self) -> AutonomousDefaults {
        self.defaults
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
        workdir: "C:/proyecto".to_string(),
        accepted: false,
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

fn criteria() -> Value {
    json!([
        {"text": "compila", "check": "cargo build"},
        {"text": "pasan las pruebas", "check": "cargo test"}
    ])
}

fn task_args() -> Value {
    json!({
        "task": "migrar la API",
        "done_criteria": criteria(),
        "agent": "codex",
        "space": "proyecto",
        "workdir": "C:/proyecto"
    })
}

fn error_of(result: &ToolResult) -> &str {
    result.error.as_deref().unwrap_or_default()
}

fn default_host() -> Arc<FakeHost> {
    Arc::new(FakeHost::default())
}

fn host_with(state: TaskState) -> Arc<FakeHost> {
    Arc::new(FakeHost {
        summary: Mutex::new(Some(summary(state))),
        ..FakeHost::default()
    })
}

#[test]
fn registers_five_tools_with_their_levels_and_self_confirmation() {
    let specs = build(default_host(), FakeGuards::allowed("modo autónomo"));
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "start_autonomous_task",
            "start_autonomous_team",
            "autonomous_status",
            "stop_autonomous_task",
            "resume_autonomous_task"
        ]
    );
    let levels: Vec<i64> = specs.iter().map(|spec| spec.level).collect();
    assert_eq!(levels, vec![2, 2, 0, 0, 1]);
    let self_confirmed: Vec<bool> = specs.iter().map(|spec| spec.self_confirmed).collect();
    assert_eq!(self_confirmed, vec![true, true, false, false, false]);
    assert_eq!(specs[0].timeout_s, 120.0);
    assert_eq!(specs[1].timeout_s, 120.0);
    assert_eq!(specs[3].timeout_s, 20.0);
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
fn authorization_looks_at_the_last_three_messages_of_ten_minutes() {
    let now = 1_000_000.0;
    let message = |text: &str, at: f64| (text.to_string(), at);
    assert!(authorized(
        &[message("hazlo en modo autónomo", now - 5.0)],
        now
    ));
    // Una aclaración posterior no borra la autorización de hace dos mensajes.
    assert!(authorized(
        &[
            message("hazlo en modo autónomo", now - 60.0),
            message("¿qué pruebas uso?", now - 30.0),
            message("las de integración", now - 10.0),
        ],
        now
    ));
    // El cuarto mensaje ya no cuenta.
    assert!(!authorized(
        &[
            message("autorizo", now - 40.0),
            message("uno", now - 30.0),
            message("dos", now - 20.0),
            message("tres", now - 10.0),
        ],
        now
    ));
    // Fuera de la ventana de diez minutos.
    assert!(!authorized(&[message("modo autónomo", now - 601.0)], now));
    // Una negación entre los últimos tres anula la autorización.
    assert!(!authorized(
        &[
            message("hazlo en modo autónomo", now - 60.0),
            message("no autorizo los permisos", now - 10.0),
        ],
        now
    ));
    assert!(!authorized(
        &[message("hazlo sin autorización mía", now - 5.0)],
        now
    ));
    assert!(!authorized(
        &[message("no lo hagas en modo autónomo", now - 5.0)],
        now
    ));
    assert!(authorized(
        &[message("hazlo sin pedirme permiso", now - 5.0)],
        now
    ));
    assert!(authorized(&[message("yolo", now - 5.0)], now));
}

#[test]
fn an_unfinished_task_blocks_another_one() {
    for state in [
        TaskState::Preparing,
        TaskState::Running,
        TaskState::Paused,
        TaskState::Verifying,
    ] {
        let result = run(
            &build(host_with(state), FakeGuards::allowed("modo autónomo"))[0],
            task_args(),
        );
        assert_eq!(error_of(&result), texts::ALREADY_ACTIVE, "{state:?}");
    }
    let done = run(
        &build(host_with(TaskState::Done), FakeGuards::allowed("autorizo"))[0],
        task_args(),
    );
    assert!(done.ok, "{:?}", done.error);
}

#[test]
fn a_no_in_the_confirmation_does_not_start() {
    let host = default_host();
    let guards = Arc::new(FakeGuards {
        answer: false,
        ..FakeGuards::clone_for(&FakeGuards::allowed("modo autónomo, autorizo"))
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
    assert_eq!(spec.done_criteria[0].check, "cargo build");
    assert_eq!(spec.agent, "codex");
    assert_eq!(spec.space, "proyecto");
    assert_eq!(spec.workdir, "C:/proyecto");
    assert_eq!(spec.verifier_agent, "codex");
    assert_eq!(spec.terminal, None);
    assert_eq!(spec.max_rounds, 5);
    assert_eq!(spec.max_minutes, 120);
    assert_eq!(spec.max_approvals, 200);
}

#[test]
fn the_confirmation_enumerates_criteria_and_is_strong() {
    let host = default_host();
    let guards = FakeGuards::allowed("yolo");
    run(&build(host, Arc::clone(&guards))[0], task_args());
    let asked = lock(&guards.asked);
    assert_eq!(asked.len(), 1);
    let (text, strong) = &asked[0];
    assert!(*strong, "la activación se confirma en modo fuerte (10.3)");
    assert_eq!(
        *text,
        texts::confirm(
            "migrar la API",
            "codex",
            "proyecto",
            "C:/proyecto",
            &[
                crate::autonomous::Criterion {
                    text: "compila".to_string(),
                    check: "cargo build".to_string(),
                },
                crate::autonomous::Criterion {
                    text: "pasan las pruebas".to_string(),
                    check: "cargo test".to_string(),
                },
            ],
            5,
            120,
        )
    );
    assert!(text.contains("en la carpeta «C:/proyecto»"));
    assert!(text.contains("1. compila (se comprueba con: cargo build)"));
    assert!(text.ends_with("¿Lo activo?"));
}

#[test]
fn workdir_comes_from_the_argument_or_default_cwd() {
    let host = default_host();
    let guards = FakeGuards::allowed("modo autónomo");
    let mut args = task_args();
    args.as_object_mut().expect("objeto").remove("workdir");
    let result = run(&build(Arc::clone(&host), guards)[0], args.clone());
    assert_eq!(error_of(&result), texts::WORKDIR_MISSING);

    let guards = FakeGuards::allowed("modo autónomo").with_default_cwd("D:/trabajo");
    let result = run(&build(Arc::clone(&host), guards)[0], args);
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(lock(&host.started)[0].workdir, "D:/trabajo");
}

#[test]
fn criteria_need_text_and_check() {
    let guards = FakeGuards::allowed("modo autónomo, autorizo");
    let specs = build(default_host(), guards);
    let mut args = task_args();
    args["done_criteria"] = json!(["compila"]);
    assert!(!run(&specs[0], args).ok);

    let mut args = task_args();
    args["done_criteria"] = json!([{"text": "compila"}]);
    let result = run(&specs[0], args);
    assert_eq!(error_of(&result), texts::MISSING_CHECK);

    let mut args = task_args();
    args["done_criteria"] = json!([{"text": "  ", "check": "cargo build"}]);
    assert!(!run(&specs[0], args).ok);
}

#[test]
fn optional_values_reach_the_spec() {
    let host = default_host();
    let guards = FakeGuards::allowed("autorizo el modo autónomo");
    let args = json!({
        "task": "migrar la API",
        "done_criteria": criteria(),
        "agent": "codex",
        "space": "proyecto",
        "workdir": "C:/proyecto",
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
fn voice_defaults_apply_only_when_the_argument_is_missing() {
    let host = default_host();
    let mut guards = FakeGuards::clone_for(&FakeGuards::allowed("modo autónomo"));
    guards.defaults = AutonomousDefaults {
        max_rounds: 7,
        max_minutes: 240,
        max_approvals: 40,
    };
    let result = run(&build(Arc::clone(&host), Arc::new(guards))[0], task_args());
    assert!(result.ok, "{:?}", result.error);
    let started = lock(&host.started);
    assert_eq!(started[0].max_rounds, 7);
    assert_eq!(started[0].max_minutes, 240);
    assert_eq!(started[0].max_approvals, 40);

    let host = default_host();
    let guards = FakeGuards::allowed("modo autónomo");
    let mut args = task_args();
    args["max_rounds"] = json!(2);
    run(&build(Arc::clone(&host), guards)[0], args);
    assert_eq!(lock(&host.started)[0].max_rounds, 2);
}

#[test]
fn team_tool_starts_the_team_and_enumerates_its_roles() {
    let host = default_host();
    let guards = FakeGuards::allowed("modo autónomo");
    let args = json!({
        "task": "migrar la API",
        "done_criteria": criteria(),
        "roles": [
            {"role": "plan", "agent": "codex"},
            {"role": "dev", "agent": "opencode", "model": "big"}
        ],
        "space": "proyecto",
        "workdir": "C:/proyecto"
    });
    let result = run(&build(Arc::clone(&host), Arc::clone(&guards))[1], args);
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["id"], "auto-2");
    let teams = lock(&host.teams);
    assert_eq!(teams[0].roles.len(), 2);
    assert_eq!(teams[0].roles[1].model.as_deref(), Some("big"));
    assert_eq!(teams[0].workdir, "C:/proyecto");
    let asked = lock(&guards.asked);
    assert!(asked[0]
        .0
        .contains("con el equipo plan: codex, dev: opencode (big)"));
    assert!(asked[0].1, "la activación del equipo también es fuerte");
}

#[test]
fn status_reports_the_task_or_its_absence() {
    let empty = build(default_host(), FakeGuards::allowed("x"));
    let result = run(&empty[2], json!({}));
    assert_eq!(result.data["active"], false);
    assert_eq!(result.data["message"], texts::NO_TASK);

    let host = host_with(TaskState::Paused);
    let result = run(&build(host, FakeGuards::allowed("x"))[2], json!({}));
    assert_eq!(result.data["active"], true);
    assert_eq!(result.data["state"], "paused");
    assert_eq!(result.data["round"], 2);
    assert_eq!(result.data["workdir"], "C:/proyecto");
    assert_eq!(result.data["message"], "Autónomo · ronda 2 · en pausa");
}

#[test]
fn stop_passes_the_reason_and_accepts_a_done_task() {
    let empty = build(default_host(), FakeGuards::allowed("x"));
    let result = run(&empty[3], json!({}));
    assert_eq!(result.data["stopped"], false);
    assert_eq!(result.data["message"], texts::NO_TASK);

    let host = host_with(TaskState::Running);
    let result = run(
        &build(Arc::clone(&host), FakeGuards::allowed("x"))[3],
        json!({"reason": "por voz"}),
    );
    assert_eq!(result.data["stopped"], true);
    assert_eq!(result.data["message"], texts::STOPPED);
    assert_eq!(lock(&host.stopped)[0], ("por voz".to_string(), false));

    // Parar una tarea cumplida no la acepta.
    let done = host_with(TaskState::Done);
    let result = run(
        &build(Arc::clone(&done), FakeGuards::allowed("x"))[3],
        json!({}),
    );
    assert_eq!(result.data["stopped"], false);
    assert!(lock(&done.stopped).is_empty());

    // Con «accept» sí, y nunca pregunta.
    let done = host_with(TaskState::Done);
    let guards = FakeGuards::allowed("x");
    let result = run(
        &build(Arc::clone(&done), Arc::clone(&guards))[3],
        json!({"accept": true}),
    );
    assert_eq!(result.data["accepted"], true);
    assert_eq!(result.data["message"], texts::ACCEPTED);
    assert!(lock(&done.stopped)[0].1);
    assert!(lock(&guards.asked).is_empty(), "aceptar nunca pregunta");
}

#[test]
fn resume_only_from_paused_and_passes_the_extension() {
    let running = host_with(TaskState::Running);
    let result = run(
        &build(Arc::clone(&running), FakeGuards::allowed("x"))[4],
        json!({}),
    );
    assert_eq!(error_of(&result), texts::NOT_PAUSED);
    assert!(lock(&running.resumed).is_empty());

    let paused = host_with(TaskState::Paused);
    let result = run(
        &build(Arc::clone(&paused), FakeGuards::allowed("x"))[4],
        json!({"extend_rounds": 2, "extend_minutes": 30}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["message"], texts::RESUMED);
    assert_eq!(lock(&paused.resumed)[0], (2, 30));

    let empty = build(default_host(), FakeGuards::allowed("x"));
    let result = run(&empty[4], json!({}));
    assert_eq!(error_of(&result), texts::NO_TASK);
}

#[test]
fn waits_for_a_pending_confirmation_before_opening_its_own() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;

    let calls = StdArc::new(AtomicUsize::new(0));
    let pending = {
        let calls = StdArc::clone(&calls);
        move || calls.fetch_add(1, Ordering::SeqCst) < 2
    };
    assert!(crate::autonomous::tools::wait_for_free(&pending, 1.0));
    assert!(!crate::autonomous::tools::wait_for_free(&|| true, 0.05));
}

#[test]
fn validation_rejects_empty_task_and_criteria() {
    let guards = FakeGuards::allowed("modo autónomo, autorizo");
    let specs = build(default_host(), guards);
    let mut args = task_args();
    args["task"] = json!("   ");
    assert!(!run(&specs[0], args).ok);
    let mut args = task_args();
    args["done_criteria"] = json!([]);
    assert!(!run(&specs[0], args).ok);
    let mut args = task_args();
    args["done_criteria"] = json!(criteria().as_array().cloned().unwrap_or_default());
    assert!(run(&specs[0], args).ok);
}
