//! Las cuatro herramientas del modo autónomo (9.2) y sus guardas de activación.
//!
//! Las guardas van en el código de la herramienta, antes de preguntar: (1) el
//! turno no puede estar contaminado; (2) el último mensaje del usuario tiene que
//! autorizarlo; (3) solo hay una tarea a la vez; (4) la confirmación única.

use std::sync::{Arc, OnceLock};

use ez_core::pyvalue::py_strip;
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::registry::{ToolResult, ToolSpec};
use crate::runtime::AgentRuntime;

use super::texts;
use super::{AutonomousHost, RoleSpec, StartSpec, TeamSpec};

/// Autorización explícita del usuario (9.2).
pub const AUTHORIZE_RE: &str =
    r"(?i)(aut[oó]nom|autorizo|yolo|sin pedirme permiso|sin preguntarme)";
pub const DEFAULT_MAX_ROUNDS: i64 = 5;
pub const DEFAULT_MAX_MINUTES: i64 = 120;
pub const MAX_CRITERIA: usize = 10;
pub const MAX_ROLES: i64 = 8;
pub const MAX_TASK_CHARS: i64 = 2000;
pub const MAX_CRITERION_CHARS: i64 = 300;

/// Lo que las herramientas necesitan del runtime para las guardas.
pub trait ToolGuards: Send + Sync {
    /// `session.tainted` del turno en curso.
    fn tainted(&self) -> bool;
    /// Último mensaje del usuario.
    fn last_user_text(&self) -> String;
    /// Confirmación única; `false` si el usuario dice «no» o no responde.
    fn confirm(&self, summary: &str) -> bool;
}

/// Guardas sobre un [`AgentRuntime`]: estado de sesión y gestor de confirmación.
pub struct RuntimeGuards {
    runtime: AgentRuntime,
}

impl RuntimeGuards {
    pub fn new(runtime: AgentRuntime) -> Self {
        Self { runtime }
    }
}

impl ToolGuards for RuntimeGuards {
    fn tainted(&self) -> bool {
        self.runtime.tainted()
    }

    fn last_user_text(&self) -> String {
        self.runtime.get_last_user_text()
    }

    fn confirm(&self, summary: &str) -> bool {
        let Some(gate) = self.runtime.confirm_gate() else {
            return false;
        };
        // DECISION (como `execute`): un fallo de la UI no debe tumbar la herramienta.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gate.ask(summary, false, None)
        }))
        .unwrap_or(false)
    }
}

/// Bucle instalado por la app (A3). Es uno por proceso, igual que la tarea.
// DECISION: el grupo de herramientas se construye al crear el runtime, antes de
// que exista el bucle; el slot permite registrarlo donde van los demás grupos y
// que las herramientas aparezcan en cuanto A3 llame a `install_host`.
static HOST: OnceLock<Arc<dyn AutonomousHost>> = OnceLock::new();

/// Instala el bucle para el grupo ya registrado. `false` si ya había uno.
pub fn install_host(host: Arc<dyn AutonomousHost>) -> bool {
    HOST.set(host).is_ok()
}

fn authorize() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(AUTHORIZE_RE).expect("regex de autorización"))
}

fn text(args: &Map<String, Value>, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(value)) => py_strip(value).to_string(),
        _ => String::new(),
    }
}

fn integer(args: &Map<String, Value>, key: &str, default: i64) -> i64 {
    args.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn criteria(args: &Map<String, Value>) -> Result<Vec<String>, String> {
    let Some(Value::Array(items)) = args.get("done_criteria") else {
        return Err("Indica los criterios en «done_criteria».".to_string());
    };
    if items.len() > MAX_CRITERIA {
        return Err(format!(
            "Un encargo admite como máximo {MAX_CRITERIA} criterios."
        ));
    }
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Some(value) = item.as_str() else {
            return Err("Cada criterio debe ser texto.".to_string());
        };
        let value = py_strip(value);
        if value.is_empty() {
            return Err("Los criterios no pueden estar vacíos.".to_string());
        }
        list.push(value.to_string());
    }
    if list.is_empty() {
        return Err("Indica al menos un criterio verificable.".to_string());
    }
    Ok(list)
}

fn roles(args: &Map<String, Value>) -> Result<Vec<RoleSpec>, String> {
    let Some(Value::Array(items)) = args.get("roles") else {
        return Err("Indica los papeles del equipo en «roles».".to_string());
    };
    if items.len() as i64 > MAX_ROLES {
        return Err(format!(
            "Un equipo autónomo admite como máximo {MAX_ROLES} papeles."
        ));
    }
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Some(object) = item.as_object() else {
            return Err("Cada papel debe ser un objeto.".to_string());
        };
        let role = text(object, "role");
        let agent = text(object, "agent");
        if role.is_empty() || agent.is_empty() {
            return Err("Cada papel necesita «role» y «agent».".to_string());
        }
        let model = text(object, "model");
        list.push(RoleSpec {
            role,
            agent,
            model: (!model.is_empty()).then_some(model),
        });
    }
    if list.is_empty() {
        return Err("Indica al menos un papel.".to_string());
    }
    Ok(list)
}

/// Las tres guardas previas a la confirmación (9.2).
fn check_activation(guards: &dyn ToolGuards, host: &dyn AutonomousHost) -> Result<(), String> {
    if guards.tainted() {
        return Err(texts::TAINTED.to_string());
    }
    if !authorize().is_match(&guards.last_user_text()) {
        return Err(texts::NOT_AUTHORIZED.to_string());
    }
    if host
        .status()
        .is_some_and(|summary| summary.state.unfinished())
    {
        return Err(texts::ALREADY_ACTIVE.to_string());
    }
    Ok(())
}

/// Guarda 4: la confirmación única con el texto de 9.6.
fn confirmed(guards: &dyn ToolGuards, summary: &str) -> bool {
    guards.confirm(summary)
}

fn start_task(
    host: &dyn AutonomousHost,
    guards: &dyn ToolGuards,
    args: &Map<String, Value>,
) -> ToolResult {
    let task = text(args, "task");
    if task.is_empty() {
        return ToolResult::err("Falta «task»: el encargo de la tarea.");
    }
    let agent = text(args, "agent");
    let space = text(args, "space");
    if agent.is_empty() || space.is_empty() {
        return ToolResult::err("Indica el agente y el espacio de la tarea.");
    }
    let done_criteria = match criteria(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    if let Err(error) = check_activation(guards, host) {
        return ToolResult::err(error);
    }
    let verifier = text(args, "verifier_agent");
    let max_rounds = integer(args, "max_rounds", DEFAULT_MAX_ROUNDS);
    let max_minutes = integer(args, "max_minutes", DEFAULT_MAX_MINUTES);
    let spec = StartSpec {
        task: task.clone(),
        done_criteria,
        agent: agent.clone(),
        space: space.clone(),
        terminal: Some(text(args, "terminal")).filter(|value| !value.is_empty()),
        verifier_agent: if verifier.is_empty() { agent } else { verifier },
        max_rounds,
        max_minutes,
    };
    let summary = texts::confirm(&task, &spec.agent, &space, max_rounds, max_minutes);
    if !confirmed(guards, &summary) {
        return ToolResult::err(texts::DECLINED);
    }
    match host.start(&spec) {
        Ok(id) => ToolResult::ok(json!({"id": id, "message": texts::START})),
        Err(error) => ToolResult::err(error),
    }
}

fn start_team(
    host: &dyn AutonomousHost,
    guards: &dyn ToolGuards,
    args: &Map<String, Value>,
) -> ToolResult {
    let task = text(args, "task");
    if task.is_empty() {
        return ToolResult::err("Falta «task»: el encargo de la tarea.");
    }
    let space = text(args, "space");
    if space.is_empty() {
        return ToolResult::err("Indica el espacio de la tarea.");
    }
    let done_criteria = match criteria(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    let roles = match roles(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    if let Err(error) = check_activation(guards, host) {
        return ToolResult::err(error);
    }
    let max_rounds = integer(args, "max_rounds", DEFAULT_MAX_ROUNDS);
    let max_minutes = integer(args, "max_minutes", DEFAULT_MAX_MINUTES);
    let summary = texts::confirm(&task, &roles[0].agent, &space, max_rounds, max_minutes);
    if !confirmed(guards, &summary) {
        return ToolResult::err(texts::DECLINED);
    }
    let spec = TeamSpec {
        task,
        done_criteria,
        roles,
        space,
        max_rounds,
        max_minutes,
    };
    match host.start_team(&spec) {
        Ok(id) => ToolResult::ok(json!({"id": id, "message": texts::START})),
        Err(error) => ToolResult::err(error),
    }
}

fn status(host: &dyn AutonomousHost) -> ToolResult {
    let Some(summary) = host.status() else {
        return ToolResult::ok(json!({"active": false, "message": texts::NO_TASK}));
    };
    let message = summary.describe();
    ToolResult::ok(json!({
        "active": summary.state.unfinished(),
        "id": summary.id,
        "mode": summary.mode.as_str(),
        "state": summary.state.as_str(),
        "round": summary.round,
        "max_rounds": summary.max_rounds,
        "approvals": summary.approvals,
        "clean_streak": summary.clean_streak,
        "pause_reason": summary.pause_reason,
        "message": message,
    }))
}

fn stop(host: &dyn AutonomousHost, args: &Map<String, Value>) -> ToolResult {
    if host.status().is_none() {
        return ToolResult::ok(json!({"stopped": false, "message": texts::NO_TASK}));
    }
    match host.stop(&text(args, "reason")) {
        Ok(()) => ToolResult::ok(json!({"stopped": true, "message": texts::STOPPED})),
        Err(error) => ToolResult::err(error),
    }
}

fn tool(
    name: &str,
    description: &str,
    parameters: Value,
    timeout_s: f64,
    handler: impl Fn(&Map<String, Value>) -> ToolResult + Send + Sync + 'static,
) -> ToolSpec {
    let mut spec = ToolSpec::new(
        name,
        description,
        parameters,
        2,
        Arc::new(move |args| Ok(handler(args).into())),
    );
    spec.timeout_s = timeout_s;
    spec
}

fn task_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_TASK_CHARS,
                "description": texts::TASK_DESCRIPTION
            },
            "done_criteria": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_CRITERIA,
                "description": texts::CRITERIA_DESCRIPTION,
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_CRITERION_CHARS
                }
            },
            "agent": {"type": "string", "description": texts::AGENT_DESCRIPTION},
            "space": {"type": "string", "description": texts::SPACE_DESCRIPTION},
            "terminal": {"type": "string", "description": texts::TERMINAL_DESCRIPTION},
            "verifier_agent": {
                "type": "string",
                "description": texts::VERIFIER_DESCRIPTION
            },
            "max_rounds": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "default": DEFAULT_MAX_ROUNDS,
                "description": texts::ROUNDS_DESCRIPTION
            },
            "max_minutes": {
                "type": "integer",
                "minimum": 5,
                "maximum": 480,
                "default": DEFAULT_MAX_MINUTES,
                "description": texts::MINUTES_DESCRIPTION
            }
        },
        "required": ["task", "done_criteria", "agent", "space"]
    })
}

fn team_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_TASK_CHARS,
                "description": texts::TASK_DESCRIPTION
            },
            "done_criteria": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_CRITERIA,
                "description": texts::CRITERIA_DESCRIPTION,
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_CRITERION_CHARS
                }
            },
            "roles": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_ROLES,
                "description": texts::ROLES_DESCRIPTION,
                "items": {
                    "type": "object",
                    "properties": {
                        "role": {"type": "string"},
                        "agent": {"type": "string"},
                        "model": {"type": "string"}
                    },
                    "required": ["role", "agent"]
                }
            },
            "space": {"type": "string", "description": texts::SPACE_DESCRIPTION},
            "max_rounds": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "default": DEFAULT_MAX_ROUNDS,
                "description": texts::ROUNDS_DESCRIPTION
            },
            "max_minutes": {
                "type": "integer",
                "minimum": 5,
                "maximum": 480,
                "default": DEFAULT_MAX_MINUTES,
                "description": texts::MINUTES_DESCRIPTION
            }
        },
        "required": ["task", "done_criteria", "roles", "space"]
    })
}

fn stop_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reason": {"type": "string", "description": texts::REASON_DESCRIPTION}
        }
    })
}

const START_TASK_DESCRIPTION: &str = "Activa el modo autónomo para una tarea: vigilo al \
    ejecutor, apruebo una sola vez los permisos que pidan los agentes de esta tarea y verifico \
    el resultado con un verificador nuevo en cada ronda. Solo funciona si el usuario lo pidió \
    explícitamente en su último mensaje.";
const START_TEAM_DESCRIPTION: &str = "Activa el modo autónomo con un equipo de varios papeles \
    (plan, dev, qa…) sobre el enjambre: el equipo ejecuta y yo verifico contra los criterios. \
    Solo funciona si el usuario lo pidió explícitamente en su último mensaje.";
const STATUS_DESCRIPTION: &str = "Dice cómo va la tarea autónoma: estado, ronda, permisos \
    aprobados y motivo de la pausa.";
const STOP_DESCRIPTION: &str = "Detiene el modo autónomo y retira la autorización al instante.";

/// Las cuatro herramientas; vacío mientras A3 no instale el bucle.
pub fn build_autonomous_tools(
    host: Option<Arc<dyn AutonomousHost>>,
    guards: Arc<dyn ToolGuards>,
) -> Vec<ToolSpec> {
    let Some(host) = host else {
        return Vec::new();
    };
    let task_host = Arc::clone(&host);
    let task_guards = Arc::clone(&guards);
    let team_host = Arc::clone(&host);
    let team_guards = Arc::clone(&guards);
    let status_host = Arc::clone(&host);
    let stop_host = Arc::clone(&host);
    vec![
        tool(
            "start_autonomous_task",
            START_TASK_DESCRIPTION,
            task_parameters(),
            120.0,
            move |args| start_task(task_host.as_ref(), task_guards.as_ref(), args),
        ),
        tool(
            "start_autonomous_team",
            START_TEAM_DESCRIPTION,
            team_parameters(),
            120.0,
            move |args| start_team(team_host.as_ref(), team_guards.as_ref(), args),
        ),
        tool(
            "autonomous_status",
            STATUS_DESCRIPTION,
            json!({"type": "object", "properties": {}}),
            20.0,
            move |_| status(status_host.as_ref()),
        ),
        tool(
            "stop_autonomous_task",
            STOP_DESCRIPTION,
            stop_parameters(),
            20.0,
            move |args| stop(stop_host.as_ref(), args),
        ),
    ]
}

/// Grupo para el registro del runtime; usa el bucle instalado por [`install_host`].
pub fn build_for_runtime(runtime: &AgentRuntime) -> Vec<ToolSpec> {
    build_autonomous_tools(
        HOST.get().cloned(),
        Arc::new(RuntimeGuards::new(runtime.clone())),
    )
}
