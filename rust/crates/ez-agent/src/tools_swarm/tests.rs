//! Tests of the `swarm` tool.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ez_core::pyvalue::char_len;
use serde_json::{json, Map, Value};

use super::*;
use crate::registry::ToolReturn;
use ez_swarm::supervisor::{LaunchRequest, SwarmHost};

#[derive(Default)]
struct FakeHost {
    names: Mutex<Vec<String>>,
    default_cwd: Mutex<String>,
    saved: Mutex<Vec<String>>,
    save_path: Mutex<Option<String>>,
    screens: Mutex<HashMap<String, String>>,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            names: Mutex::new(vec![
                "claude".to_string(),
                "codex".to_string(),
                "opencode".to_string(),
            ]),
            default_cwd: Mutex::new("C:/proyecto".to_string()),
            ..Self::default()
        }
    }

    fn lock<T>(cell: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        cell.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl SwarmHost for FakeHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(Vec::new())
    }

    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, String> {
        Err("no existe".to_string())
    }

    fn read_agent(&self, address: &str, _lines: i64) -> Result<String, String> {
        Self::lock(&self.screens)
            .get(address)
            .cloned()
            .ok_or_else(|| "sin pantalla".to_string())
    }

    fn send_task(&self, _address: &str, _line: &str) -> Result<Map<String, Value>, String> {
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        Ok(row)
    }

    fn run_agent(
        &self,
        _agent: &str,
        _task: &str,
        _request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        Err("no se abre en las pruebas".to_string())
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        Self::lock(&self.names)
            .iter()
            .find(|known| known.as_str() == name)
            .cloned()
    }

    fn swarm_args(&self, _agent: &str) -> Vec<String> {
        Vec::new()
    }

    fn save_context(&self, text: &str) -> Option<String> {
        Self::lock(&self.saved).push(text.to_string());
        Self::lock(&self.save_path).clone()
    }

    fn default_cwd(&self) -> String {
        Self::lock(&self.default_cwd).clone()
    }

    fn agent_names(&self) -> Vec<String> {
        Self::lock(&self.names).clone()
    }
}

struct Fixture {
    stack: Arc<SwarmStack>,
    host: Arc<FakeHost>,
    base: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        remove_tree(&self.base);
    }
}

fn remove_tree(folder: &PathBuf) {
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                remove_tree(&path);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let _ = std::fs::remove_dir(folder);
}

fn fixture() -> Fixture {
    let base =
        std::env::temp_dir().join(format!("ez-agent-swarm-{}", uuid::Uuid::new_v4().simple()));
    let host = Arc::new(FakeHost::new());
    let host_dyn: Arc<dyn SwarmHost> = Arc::clone(&host) as Arc<dyn SwarmHost>;
    let announce: ez_swarm::supervisor::Announce = Arc::new(|_| {});
    let mut stack = SwarmStack::new(&base, host_dyn, announce, None, "ezs")
        .unwrap_or_else(|error| panic!("{error}"));
    // En las pruebas no hay `ezs.exe` junto al binario ni en el PATH.
    stack.command_dir = "C:/Programas/EzAgent".to_string();
    Fixture {
        stack: Arc::new(stack),
        host,
        base,
    }
}

fn tool(fixture: &Fixture) -> ToolSpec {
    build_swarm_tools(Some(Arc::clone(&fixture.stack)))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("falta la herramienta"))
}

fn call(tool: &ToolSpec, args: Value) -> ToolResult {
    let args = match args {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    match (tool.handler)(&args) {
        Ok(ToolReturn::Result(result)) => result,
        Ok(ToolReturn::Value(_)) => panic!("la herramienta devolvió un valor suelto"),
        Err(error) => panic!("el manejador falló: {error}"),
    }
}

fn member_args(name: &str, role: &str, agent: &str) -> Value {
    json!({"agent": agent, "role": role, "name": name})
}

fn start_swarm(fixture: &Fixture) -> String {
    let result = call(
        &tool(fixture),
        json!({
            "action": "start",
            "goal": "Migrar el núcleo",
            "members": [member_args("ana", "dev", "claude")],
        }),
    );
    assert!(result.ok, "{:?}", result.error);
    result.data["swarm"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn without_a_stack_there_are_no_tools() {
    assert!(build_swarm_tools(None).is_empty());
}

#[test]
fn the_tool_declares_its_schema_and_summaries() {
    let fixture = fixture();
    let tool = tool(&fixture);
    assert_eq!(tool.name, "swarm");
    assert_eq!(
        tool.description,
        "Enjambre: varios agentes con roles (planificador, arquitecto, orquestador, backend, frontend, QA…) que colaboran solos con un tablero común, se hablan entre sí y se devuelven correcciones. EzAgent avisa cada etapa. start lo crea con goal y members en orden de trabajo; status dice cómo va; tell pasa un recado del usuario; resolve aplica la decisión del usuario sobre un incidente (I3); pause/resume; stop lo detiene. Úsalo solo si el usuario pide trabajo en equipo o validación con ida y vuelta; para una tarea suelta usa delegate_to_agent."
    );
    assert_eq!(tool.level, 2);
    assert!(tool.untrusted_output);
    assert_eq!(tool.timeout_s, 20.0);
    assert_eq!(tool.parameters["additionalProperties"], Value::Null);
    assert_eq!(tool.parameters["required"], json!(["action"]));
    assert_eq!(
        tool.parameters["properties"]["members"]["maxItems"],
        json!(MAX_MEMBERS)
    );
    let start = tool.describe(
        &json!({"action": "START"})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    );
    assert_eq!(start.level, 1);
    assert_eq!(
        start.summary,
        "Crear un enjambre de agentes que colaboran en una tarea."
    );
    assert!(!start.always_confirm);
    let status = tool.describe(
        &json!({"action": "status"})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    );
    assert_eq!(status.level, 0);
    let unknown = tool.describe(
        &json!({"action": "raro"})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    );
    assert_eq!(unknown.level, 2);
    assert!(unknown.always_confirm);
    assert_eq!(unknown.summary, "Acción desconocida del enjambre.");
}

#[test]
fn start_creates_the_swarm_and_reports_the_team() {
    let fixture = fixture();
    let result = call(
        &tool(&fixture),
        json!({
            "action": "start",
            "goal": "Migrar el núcleo",
            "members": [
                member_args("Ana", "desarrollador", "claude"),
                {"agent": "codex", "role": "QA", "parallel": true},
            ],
        }),
    );
    assert!(result.ok, "{:?}", result.error);
    let swarm = result.data["swarm"].as_str().unwrap_or_default();
    assert_eq!(result.data["members"], json!(["ana", "qa"]));
    assert!(result.data["message"]
        .as_str()
        .unwrap_or_default()
        .contains("ana (claude), qa (codex)"));
    let info = fixture
        .stack
        .store()
        .get(swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el enjambre"));
    assert_eq!(info.goal, "Migrar el núcleo");
    assert_eq!(info.cwd, "C:/proyecto");
    let members = fixture
        .stack
        .store()
        .members(swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(members[0].role, "dev");
    assert_eq!(members[1].role, "qa");
    let roles = fixture
        .stack
        .store()
        .tasks(swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .iter()
        .map(|task| task.role.clone())
        .collect::<Vec<_>>();
    assert_eq!(roles, ["dev", "qa"]);
}

#[test]
fn start_rejects_bad_members_with_python_texts() {
    let fixture = fixture();
    let tool = tool(&fixture);
    let cases = [
        (
            json!({"action": "start", "goal": "x"}),
            "Indica los agentes del enjambre en «members».",
        ),
        (
            json!({"action": "start", "goal": "x", "members": []}),
            "Indica los agentes del enjambre en «members».",
        ),
        (
            json!({"action": "start", "goal": "x", "members": [3]}),
            "El miembro 1 debe ser un objeto.",
        ),
        (
            json!({"action": "start", "goal": "x", "members": [{"agent": "nope", "role": "dev"}]}),
            "Agente desconocido: 'nope'. Disponibles: claude, codex, opencode.",
        ),
        (
            json!({"action": "start", "goal": "x", "members": [{"agent": "claude", "role": " "}]}),
            "El miembro 1 necesita «role» (p. ej. plan, dev o qa).",
        ),
        (
            json!({"action": "start", "members": [member_args("a", "dev", "claude")]}),
            "Falta «goal»: el encargo para el enjambre.",
        ),
    ];
    for (args, message) in cases {
        let result = call(&tool, args);
        assert!(!result.ok);
        assert_eq!(result.error.as_deref(), Some(message));
    }
    let too_many = (0..9)
        .map(|index| member_args(&format!("m{index}"), "dev", "claude"))
        .collect::<Vec<_>>();
    let result = call(
        &tool,
        json!({"action": "start", "goal": "x", "members": too_many}),
    );
    assert_eq!(
        result.error.as_deref(),
        Some("Un enjambre admite como máximo 8 agentes.")
    );
}

#[test]
fn start_uses_the_default_cwd_and_keeps_the_long_goal_file() {
    let fixture = fixture();
    let long = "x".repeat(MAX_GOAL + 20);
    let result = call(
        &tool(&fixture),
        json!({
            "action": "start",
            "goal": long,
            "members": [member_args("ana", "dev", "claude")],
            "cwd": "  ",
        }),
    );
    assert!(result.ok, "{:?}", result.error);
    let swarm = result.data["swarm"].as_str().unwrap_or_default();
    let info = fixture
        .stack
        .store()
        .get(swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el enjambre"));
    assert_eq!(info.cwd, "C:/proyecto");
    assert_eq!(char_len(&info.goal), MAX_GOAL);
    assert!(info.goal.ends_with('…'));
    assert!(!fixture
        .host
        .saved
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_empty());

    *fixture
        .host
        .save_path
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some("C:/ctx/encargo.md".to_string());
    let long = "y".repeat(MAX_GOAL + 20);
    let result = call(
        &tool(&fixture),
        json!({
            "action": "start",
            "goal": long,
            "members": [member_args("bo", "dev", "claude")],
        }),
    );
    assert!(result.ok, "{:?}", result.error);
    let swarm = result.data["swarm"].as_str().unwrap_or_default();
    let goal = fixture
        .stack
        .store()
        .get(swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el enjambre"))
        .goal;
    assert!(goal.ends_with(" [Encargo completo, léelo antes de empezar: C:/ctx/encargo.md]"));
    assert_eq!(char_len(&goal), MAX_GOAL);
}

#[test]
fn names_are_deduplicated_and_roles_are_normalized() {
    let fixture = fixture();
    let result = call(
        &tool(&fixture),
        json!({
            "action": "start",
            "goal": "x",
            "members": [
                member_args("Ana", "planificador", "claude"),
                member_args("ANA", "ui", "codex"),
            ],
        }),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["members"], json!(["ana", "ana2"]));
    let members = fixture
        .stack
        .store()
        .members(result.data["swarm"].as_str().unwrap_or_default())
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(members[0].role, "plan");
    assert_eq!(members[1].role, "ux");
}

#[test]
fn status_tell_pause_resume_and_stop_work_on_the_latest_swarm() {
    let fixture = fixture();
    let swarm = start_swarm(&fixture);
    let tool = tool(&fixture);
    let status = call(&tool, json!({"action": "status"}));
    assert!(status.ok, "{:?}", status.error);
    assert_eq!(status.data["swarm"], json!(swarm));
    assert!(status.data["spoken"]
        .as_str()
        .unwrap_or_default()
        .contains("tareas"));
    assert!(status.data["board"]
        .as_str()
        .unwrap_or_default()
        .contains("Enjambre"));
    let told = call(&tool, json!({"action": "tell", "text": "  hola equipo  "}));
    assert!(told.ok, "{:?}", told.error);
    assert_eq!(told.data["message"], json!("Recado entregado."));
    let empty = call(&tool, json!({"action": "tell"}));
    assert_eq!(
        empty.error.as_deref(),
        Some("Falta «text»: el recado para el equipo.")
    );
    let paused = call(&tool, json!({"action": "pause"}));
    assert!(paused.ok, "{:?}", paused.error);
    assert_eq!(paused.data["message"], json!("Enjambre en pausa."));
    let resumed = call(&tool, json!({"action": "resume"}));
    assert!(resumed.ok, "{:?}", resumed.error);
    assert_eq!(resumed.data["message"], json!("Enjambre retomado."));
    let stopped = call(&tool, json!({"action": "stop"}));
    assert!(stopped.ok, "{:?}", stopped.error);
    assert_eq!(
        stopped.data["message"],
        json!("Enjambre detenido. Las terminales de los agentes siguen abiertas.")
    );
    let unknown = call(&tool, json!({"action": "raro"}));
    assert_eq!(
        unknown.error.as_deref(),
        Some("Acción desconocida: 'raro'. Usa: start, status, tell, resolve, pause, resume, stop.")
    );
    let missing = call(&tool, json!({"action": "status", "swarm": "nope"}));
    assert_eq!(missing.error.as_deref(), Some("No hay ningún enjambre."));
}

#[test]
fn resolve_takes_the_only_incident_or_requires_a_number() {
    let fixture = fixture();
    let members = [
        ez_swarm::service::MemberSpec::new("ana", "dev", "claude"),
        ez_swarm::service::MemberSpec::new("bo", "dev", "opencode"),
    ];
    let swarm = fixture
        .stack
        .service
        .start("Migrar", &members, "", None)
        .unwrap_or_else(|error| panic!("{error}"))
        .0;
    fixture
        .stack
        .store()
        .open_incident(&swarm, "ana", "quota", "detalle", 0.0)
        .unwrap_or_else(|error| panic!("{error}"));
    let tool = tool(&fixture);
    let result = call(
        &tool,
        json!({"action": "resolve", "swarm": swarm, "choice": "wait"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(
        result.data["message"],
        json!("Espero a ana; el resto sigue.")
    );
    let second = fixture
        .stack
        .store()
        .open_incident(&swarm, "bo", "context", "detalle", 0.0)
        .unwrap_or_else(|error| panic!("{error}"))
        .0;
    let ambiguous = call(&tool, json!({"action": "resolve", "choice": "wait"}));
    assert_eq!(
        ambiguous.error.as_deref(),
        Some("Indica el incidente (por ejemplo I3).")
    );
    let by_id = call(
        &tool,
        json!({"action": "resolve", "incident": format!("I{}", second.id), "choice": "reopen"}),
    );
    assert!(by_id.ok, "{:?}", by_id.error);
    assert!(by_id.data["message"]
        .as_str()
        .unwrap_or_default()
        .starts_with("Reabro a bo"));
    let bad_choice = call(
        &tool,
        json!({"action": "resolve", "incident": "1", "choice": "xyz"}),
    );
    assert_eq!(
        bad_choice.error.as_deref(),
        Some("Opción desconocida: 'xyz'. Usa: wait, replace, reopen, release, pause.")
    );
}

#[test]
fn slug_and_role_match_the_python_rules() {
    assert_eq!(slug(" Diseñador UI "), "disenador-ui");
    assert_eq!(slug("--Añadir_3--"), "anadir_3");
    assert_eq!(slug("ñ"), "n");
    assert_eq!(slug(&"x".repeat(40)).chars().count(), 32);
    assert_eq!(role(" Desarrollador "), "dev");
    assert_eq!(role("Diseñador"), "ux");
    assert_eq!(role("backend"), "back");
    assert_eq!(role("Mi Rol"), "mi-rol");
    assert_eq!(role(""), "");
    assert_eq!(role("ñ"), "n");
}

#[test]
fn a_member_that_is_not_a_member_is_reported() {
    let fixture = fixture();
    let swarm = start_swarm(&fixture);
    let result = call(
        &tool(&fixture),
        json!({"action": "resolve", "swarm": swarm, "incident": "9", "choice": "wait"}),
    );
    assert_eq!(
        result.error.as_deref(),
        Some("No hay un incidente abierto I9.")
    );
}
