//! Port of `src/ezagent/agent/tools_swarm.py` (F4-13b).
//!
//! Herramienta `swarm`: varios agentes que colaboran con un tablero común.
//!
//! Es la vía cara de delegar, así que solo se ofrece al modelo cuando el usuario
//! habla de trabajo en equipo o ya hay un enjambre en marcha. El modelo solo
//! aporta valores (objetivo, agentes, roles, modelo, esfuerzo); las
//! instrucciones de cada agente, los avisos y los resúmenes los genera el código.

use std::sync::{Arc, OnceLock};

use ez_core::pyvalue::{char_len, head_chars, py_strip, repr_str, strip_accents};
use ez_swarm::db::DbError;
use ez_swarm::models::{
    clip, member_name, role_name, SwarmError, BROADCAST, MAX_GOAL, MAX_REF, MAX_TITLE,
};
use ez_swarm::service::MemberSpec;
use ez_swarm::stack::SwarmStack;
use ez_swarm::view::board;
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::registry::{ToolInfo, ToolResult, ToolSpec};

/// Miembros que admite un enjambre.
pub const MAX_MEMBERS: usize = 8;

const ACTIONS: [&str; 7] = [
    "start", "status", "tell", "resolve", "pause", "resume", "stop",
];
const LEVELS: [(&str, i64); 7] = [
    ("start", 1),
    ("status", 0),
    ("tell", 1),
    ("resolve", 1),
    ("pause", 1),
    ("resume", 1),
    ("stop", 1),
];
/// Sin tildes ni mayúsculas (ver [`role`]). Un rol que no esté aquí se usa tal
/// cual, como nombre corto: el equipo es libre.
const ROLE_ALIASES: [(&str, &str); 13] = [
    (
        "plan",
        "planificar, planificador, planner, planear, planeacion, planificacion",
    ),
    ("arch", "arquitecto, arquitectura, architect, arquitecta"),
    (
        "lead",
        "orquestador, orquestar, coordinador, coordinar, lider, orchestrator",
    ),
    (
        "dev",
        "desarrollar, desarrollador, developer, implementar, programador",
    ),
    ("back", "backend, back-end, servidor, api"),
    ("front", "frontend, front-end, interfaz web, cliente"),
    ("ux", "ui, disenador, diseno, designer, ux/ui, ui/ux"),
    ("data", "datos, data engineer, base de datos"),
    (
        "ops",
        "devops, despliegue, desplegar, deploy, infraestructura",
    ),
    ("docs", "documentar, documentador, documentacion, writer"),
    (
        "qa",
        "validar, validador, auditar, auditor, revisar, reviewer, revisor",
    ),
    ("test", "tester, pruebas, probar, testing"),
    ("sec", "seguridad, security, pentester"),
];

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ACTIONS,
                "description": "start: crea un enjambre; status: cómo va; tell: recado del usuario para el equipo; stop: lo detiene."
            },
            "goal": {
                "type": "string",
                "description": "start: el encargo completo, con las palabras del usuario."
            },
            "members": {
                "type": "array",
                "maxItems": MAX_MEMBERS,
                "description": "start: los agentes, en el orden en que trabajan; cada etapa espera a la anterior salvo los marcados parallel.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": {"type": "string", "description": "Agente del catálogo."},
                        "role": {
                            "type": "string",
                            "description": "plan, arch (arquitecto), lead (orquestador), dev, back (backend), front (frontend), ux, data, ops, docs, qa, test, sec u otro nombre corto."
                        },
                        "task": {
                            "type": "string",
                            "description": "Opcional: qué hace este miembro, si no basta su rol."
                        },
                        "parallel": {
                            "type": "boolean",
                            "description": "true: trabaja a la vez que el miembro anterior."
                        },
                        "model": {"type": "string"},
                        "effort": {"type": "string"},
                        "name": {"type": "string", "description": "Opcional; por defecto, el rol."}
                    },
                    "required": ["agent", "role"]
                }
            },
            "cwd": {"type": "string", "description": "start: carpeta del proyecto."},
            "text": {"type": "string", "description": "tell: el recado, en una línea."},
            "to": {"type": "string", "description": "tell: nombre del miembro; sin él, a todos."},
            "swarm": {"type": "string", "description": "Id del enjambre; sin él, el más reciente."},
            "incident": {"type": "string", "description": "resolve: id del incidente (I3)."},
            "choice": {
                "type": "string",
                "enum": ["wait", "replace", "reopen", "release", "pause"],
                "description": "resolve: wait espera a que vuelva; replace lo cambia por otro agente (agent/model/effort; sin agent, el sugerido); reopen lo reabre; release pasa su trabajo a otro de su rol; pause pausa el enjambre."
            },
            "agent": {"type": "string", "description": "resolve con replace: agente nuevo."},
            "model": {"type": "string", "description": "resolve con replace: modelo."},
            "effort": {"type": "string", "description": "resolve con replace: esfuerzo."}
        },
        "required": ["action"]
    })
}

/// `value.strip()` cuando el valor es texto; si no, «».
fn text(args: &Map<String, Value>, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(value)) => py_strip(value).to_string(),
        _ => String::new(),
    }
}

fn is_true(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true)))
}

fn members(stack: &SwarmStack, raw: Option<&Value>) -> Result<Vec<MemberSpec>, SwarmError> {
    let Some(Value::Array(items)) = raw else {
        return Err(SwarmError::new(
            "Indica los agentes del enjambre en «members».",
        ));
    };
    if items.is_empty() {
        return Err(SwarmError::new(
            "Indica los agentes del enjambre en «members».",
        ));
    }
    if items.len() > MAX_MEMBERS {
        return Err(SwarmError::new(format!(
            "Un enjambre admite como máximo {MAX_MEMBERS} agentes."
        )));
    }
    let mut specs: Vec<MemberSpec> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let spec = build_member(stack, index + 1, item, &mut used, !specs.is_empty())?;
        specs.push(spec);
    }
    Ok(specs)
}

fn build_member(
    stack: &SwarmStack,
    index: usize,
    item: &Value,
    used: &mut Vec<String>,
    parallel_allowed: bool,
) -> Result<MemberSpec, SwarmError> {
    let Value::Object(item) = item else {
        return Err(SwarmError::new(format!(
            "El miembro {index} debe ser un objeto."
        )));
    };
    let asked = text(item, "agent");
    let agent = if asked.is_empty() {
        None
    } else {
        stack.host().resolve_agent(&asked)
    };
    let Some(agent) = agent else {
        let mut known = stack.host().agent_names();
        known.sort();
        return Err(SwarmError::new(format!(
            "Agente desconocido: {}. Disponibles: {}.",
            repr_str(&asked),
            known.join(", ")
        )));
    };
    let role = role(&text(item, "role"));
    if role.is_empty() {
        return Err(SwarmError::new(format!(
            "El miembro {index} necesita «role» (p. ej. plan, dev o qa)."
        )));
    }
    let base = member_name(&short_name(&text(item, "name"), &role))?;
    let mut name = base.clone();
    let mut suffix = 2;
    while used.contains(&name) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    used.push(name.clone());
    let (task, _) = clip(&text(item, "task"), MAX_TITLE);
    Ok(MemberSpec {
        name,
        role,
        agent,
        model: text(item, "model"),
        effort: text(item, "effort"),
        task,
        parallel: is_true(item.get("parallel")) && parallel_allowed,
    })
}

/// `_slug(name) or role`.
fn short_name(name: &str, role: &str) -> String {
    let short = slug(name);
    if short.is_empty() {
        role.to_string()
    } else {
        short
    }
}

/// Nombre corto válido para el tablero: sin tildes, a-z, 0-9, «-» y «_».
fn slug(text: &str) -> String {
    let plain = strip_accents(&text.to_lowercase());
    let replaced = non_slug().replace_all(&plain, "-");
    let trimmed = replaced.trim_matches(|character| character == '-' || character == '_');
    head_chars(trimmed, 32).to_string()
}

fn non_slug() -> &'static Regex {
    static NON_SLUG: OnceLock<Regex> = OnceLock::new();
    NON_SLUG.get_or_init(|| Regex::new(r"[^a-z0-9_-]+").expect("regex de nombres"))
}

/// Rol normalizado: alias conocido o nombre corto.
fn role(raw: &str) -> String {
    let plain = strip_accents(&py_strip(raw).to_lowercase());
    let chosen = alias(&plain).unwrap_or_else(|| slug(&plain));
    // DECISION: el resultado de `slug` siempre es válido o vacío; `unwrap_or_default`
    // no oculta ningún error alcanzable.
    role_name(&chosen).unwrap_or_default()
}

fn alias(plain: &str) -> Option<String> {
    for (role, names) in ROLE_ALIASES {
        if names.split(", ").any(|name| name == plain) {
            return Some(role.to_string());
        }
    }
    None
}

/// Un encargo largo va a un archivo; el tablero guarda el inicio y la ruta.
fn goal(stack: &SwarmStack, goal: &str) -> String {
    if char_len(goal) <= MAX_GOAL {
        return goal.to_string();
    }
    let path = stack
        .host()
        .save_context(goal)
        .filter(|path| char_len(path) <= MAX_REF);
    let Some(path) = path else {
        return goal.to_string(); // el tablero lo recorta
    };
    let note = format!(" [Encargo completo, léelo antes de empezar: {path}]");
    let head = clip(goal, MAX_GOAL.saturating_sub(char_len(&note))).0;
    format!("{head}{note}")
}

fn start(stack: &SwarmStack, args: &Map<String, Value>) -> Result<ToolResult, ToolError> {
    let goal_text = text(args, "goal");
    if goal_text.is_empty() {
        return Err(SwarmError::new("Falta «goal»: el encargo para el enjambre.").into());
    }
    let specs = members(stack, args.get("members"))?;
    if stack.command_dir.is_empty() {
        return Err(SwarmError::new(
            "No encuentro el comando ezs; reinstala EzAgent con «uv sync».",
        )
        .into());
    }
    let mut cwd = text(args, "cwd");
    if cwd.is_empty() {
        cwd = stack.host().default_cwd();
    }
    let (swarm, _) = stack
        .service
        .start(&goal(stack, &goal_text), &specs, &cwd, None)
        .map_err(db_error)?;
    stack.supervisor.poke();
    let team = specs
        .iter()
        .map(|spec| {
            let model = if spec.model.is_empty() {
                String::new()
            } else {
                format!(" {}", spec.model)
            };
            format!("{} ({}{model})", spec.name, spec.agent)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let names: Vec<String> = specs.iter().map(|spec| spec.name.clone()).collect();
    Ok(ToolResult::ok(json!({
        "swarm": swarm,
        "members": names,
        "message": format!(
            "Enjambre {swarm} en marcha: {team}. Abro a cada agente cuando le toca; \
             se coordinan solos y aviso al terminar o si algo necesita atención."
        ),
    })))
}

fn target(stack: &SwarmStack, args: &Map<String, Value>) -> Result<String, ToolError> {
    let mut swarm = text(args, "swarm");
    if swarm.is_empty() {
        swarm = stack.service.latest().map_err(db_error)?;
    }
    if swarm.is_empty() {
        swarm = stack.store().latest().map_err(db_error)?;
    }
    if swarm.is_empty() || stack.store().get(&swarm).map_err(db_error)?.is_none() {
        return Err(SwarmError::new("No hay ningún enjambre.").into());
    }
    Ok(swarm)
}

/// Aplica la decisión del usuario sobre un incidente.
fn resolve_one(
    stack: &SwarmStack,
    swarm: &str,
    args: &Map<String, Value>,
) -> Result<ToolResult, ToolError> {
    let mut raw = text(args, "incident")
        .to_uppercase()
        .trim_start_matches('I')
        .to_string();
    let digits = !raw.is_empty() && raw.chars().all(|character| character.is_ascii_digit());
    if !digits {
        let found = stack.store().incidents(swarm, true).map_err(db_error)?;
        if found.len() != 1 {
            return Err(SwarmError::new("Indica el incidente (por ejemplo I3).").into());
        }
        raw = found[0].id.to_string();
    }
    let id: i64 = raw.parse().map_err(|_| {
        ToolError::Other(format!(
            "Error del enjambre: ValueError: invalid literal for int() with base 10: {}",
            repr_str(&raw)
        ))
    })?;
    let message = stack
        .recovery()
        .resolve(
            id,
            &text(args, "choice"),
            &text(args, "agent"),
            &text(args, "model"),
            &text(args, "effort"),
        )
        .map_err(db_error)?;
    stack.supervisor.poke();
    Ok(ToolResult::ok(json!({"swarm": swarm, "message": message})))
}

/// `ToolResult(ok=False, error=…)`.
#[derive(Debug)]
enum ToolError {
    Swarm(SwarmError),
    Other(String),
}

impl From<SwarmError> for ToolError {
    fn from(error: SwarmError) -> Self {
        Self::Swarm(error)
    }
}

impl ToolError {
    fn text(self) -> String {
        match self {
            Self::Swarm(error) => error.to_string(),
            Self::Other(text) => text,
        }
    }
}

fn db_error(error: DbError) -> ToolError {
    match error.as_swarm() {
        Some(swarm) => ToolError::Swarm(swarm.clone()),
        // DECISION: el Python imprime el tipo de la excepción; aquí se conserva
        // el mismo prefijo y el texto del error interno.
        None => ToolError::Other(format!("Error del enjambre: {error}")),
    }
}

fn handle(stack: &SwarmStack, args: &Map<String, Value>) -> ToolResult {
    match dispatch(stack, args) {
        Ok(result) => result,
        Err(error) => ToolResult::err(error.text()),
    }
}

fn dispatch(stack: &SwarmStack, args: &Map<String, Value>) -> Result<ToolResult, ToolError> {
    let action = text(args, "action").to_lowercase();
    if action == "start" {
        return start(stack, args);
    }
    if !ACTIONS.contains(&action.as_str()) {
        return Err(SwarmError::new(format!(
            "Acción desconocida: {}. Usa: {}.",
            repr_str(&action),
            ACTIONS.join(", ")
        ))
        .into());
    }
    let swarm = target(stack, args)?;
    match action.as_str() {
        "status" => Ok(ToolResult::ok(json!({
            "swarm": swarm,
            "spoken": stack.service.summary(&swarm).map_err(db_error)?,
            "board": board(stack.store(), &swarm, "").map_err(db_error)?,
        }))),
        "resolve" => resolve_one(stack, &swarm, args),
        "pause" => {
            stack.service.pause(&swarm).map_err(db_error)?;
            Ok(ToolResult::ok(
                json!({"swarm": swarm, "message": "Enjambre en pausa."}),
            ))
        }
        "resume" => {
            stack.service.resume(&swarm).map_err(db_error)?;
            stack.supervisor.poke();
            Ok(ToolResult::ok(
                json!({"swarm": swarm, "message": "Enjambre retomado."}),
            ))
        }
        "tell" => {
            let body = text(args, "text");
            if body.is_empty() {
                return Err(SwarmError::new("Falta «text»: el recado para el equipo.").into());
            }
            let to = text(args, "to").to_lowercase();
            let target = if to.is_empty() { BROADCAST } else { &to };
            stack
                .service
                .tell(&swarm, &body, target)
                .map_err(db_error)?;
            stack.supervisor.poke();
            Ok(ToolResult::ok(
                json!({"swarm": swarm, "message": "Recado entregado."}),
            ))
        }
        _ => {
            stack.service.stop(&swarm).map_err(db_error)?;
            Ok(ToolResult::ok(json!({
                "swarm": swarm,
                "message": "Enjambre detenido. Las terminales de los agentes siguen abiertas.",
            })))
        }
    }
}

fn level(action: &str) -> i64 {
    LEVELS
        .iter()
        .find(|(name, _)| *name == action)
        .map_or(2, |(_, level)| *level)
}

fn summary(action: &str) -> &'static str {
    match action {
        "start" => "Crear un enjambre de agentes que colaboran en una tarea.",
        "status" => "Consultar cómo va el enjambre.",
        "tell" => "Pasar un recado al enjambre.",
        "resolve" => "Decidir qué hacer con un incidente del enjambre.",
        "pause" => "Pausar el enjambre.",
        "resume" => "Retomar el enjambre.",
        "stop" => "Detener el enjambre.",
        _ => "Acción desconocida del enjambre.",
    }
}

fn info(args: &Map<String, Value>) -> ToolInfo {
    let action = text(args, "action").to_lowercase();
    ToolInfo {
        level: level(&action),
        always_confirm: LEVELS.iter().all(|(name, _)| *name != action),
        summary: summary(&action).to_string(),
        ..ToolInfo::default()
    }
}

/// Herramienta del enjambre; lista vacía si la app no montó el modo.
pub fn build_swarm_tools(stack: Option<Arc<SwarmStack>>) -> Vec<ToolSpec> {
    let Some(stack) = stack else {
        return Vec::new();
    };
    let mut tool = ToolSpec::new(
        "swarm",
        "Enjambre: varios agentes con roles (planificador, arquitecto, orquestador, \
         backend, frontend, QA…) que colaboran solos con un tablero común, se hablan \
         entre sí y se devuelven correcciones. EzAgent avisa cada etapa. start lo crea \
         con goal y members en orden de trabajo; status dice cómo va; tell pasa un \
         recado del usuario; resolve aplica la decisión del usuario sobre un incidente \
         (I3); pause/resume; stop lo detiene. Úsalo solo si el usuario pide trabajo en \
         equipo o validación con ida y vuelta; para una tarea suelta usa delegate_to_agent.",
        parameters(),
        2,
        Arc::new(move |args| Ok(handle(&stack, args).into())),
    );
    // El tablero trae texto escrito por los agentes: dato no confiable.
    tool.untrusted_output = true;
    tool.timeout_s = 20.0;
    tool.describe_fn = Some(Arc::new(info));
    vec![tool]
}

#[cfg(test)]
mod tests;
