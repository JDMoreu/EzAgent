//! Las cinco herramientas del modo autónomo (10.5) y sus guardas de activación.
//!
//! Las guardas van en el código de la herramienta, antes de preguntar: (1) el
//! turno no puede estar contaminado; (2) los últimos mensajes del usuario tienen
//! que autorizarlo y ninguno puede negarlo; (3) solo hay una tarea a la vez;
//! (4) la confirmación única, fuerte.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use ez_core::pyvalue::{py_strip, python_str, truthy};
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::registry::{ToolResult, ToolSpec};
use crate::runtime::AgentRuntime;

use super::texts;
use super::AutonomousHost;

mod actions;
mod schema;
mod start;

pub use actions::{resume, status, stop};
pub use start::{start_task, start_team};

/// Autorización explícita del usuario (10.4).
pub const AUTHORIZE_RE: &str =
    r"(?i)(aut[oó]nom|autorizo|yolo|sin pedirme permiso|sin preguntarme)";
/// Negación explícita que anula la autorización (10.4).
pub const NEGATION_RE: &str =
    r"(?i)\bno\s+(te\s+)?autoriz|\bsin\s+autoriz|\bno\s+(lo\s+)?hagas\s+(en\s+modo\s+)?aut[oó]nom";
/// Ventana de los mensajes que cuentan para la autorización (10.4).
pub const AUTHORIZE_WINDOW_S: f64 = 600.0;
/// Cuántos mensajes del usuario se miran (10.4).
pub const AUTHORIZE_MESSAGES: usize = 3;

pub const DEFAULT_MAX_ROUNDS: i64 = 5;
pub const DEFAULT_MAX_MINUTES: i64 = 120;
pub const DEFAULT_MAX_APPROVALS: i64 = 200;
pub const MIN_ROUNDS: i64 = 1;
pub const MAX_ROUNDS: i64 = 20;
pub const MIN_MINUTES: i64 = 5;
pub const MAX_MINUTES: i64 = 480;
pub const MAX_CRITERIA: usize = 10;
pub const MAX_ROLES: i64 = 8;
pub const MAX_TASK_CHARS: i64 = 2000;
pub const MAX_CRITERION_CHARS: i64 = 300;
/// Timeout del registro para las herramientas de arranque (10.5).
pub const START_TIMEOUT_S: f64 = 120.0;
/// Timeout del registro para estado, parada y reanudación.
pub const ACTION_TIMEOUT_S: f64 = 20.0;

/// Topes por defecto; los argumentos y `voice.json` mandan (10.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousDefaults {
    pub max_rounds: i64,
    pub max_minutes: i64,
    pub max_approvals: i64,
}

impl Default for AutonomousDefaults {
    fn default() -> Self {
        Self {
            max_rounds: DEFAULT_MAX_ROUNDS,
            max_minutes: DEFAULT_MAX_MINUTES,
            max_approvals: DEFAULT_MAX_APPROVALS,
        }
    }
}

/// Lo que las herramientas necesitan del runtime para las guardas.
pub trait ToolGuards: Send + Sync {
    /// `session.tainted` del turno en curso.
    fn tainted(&self) -> bool;
    /// Mensajes recientes del usuario con su instante.
    fn recent_user_messages(&self) -> Vec<(String, f64)>;
    /// Confirmación única y fuerte; `false` si dice «no» o no responde.
    fn confirm(&self, summary: &str, strong: bool) -> bool;
    /// `default_cwd` de `voice.json`, vacío si no está configurado (10.2).
    fn default_cwd(&self) -> String;
    /// Topes de `voice.json`, con los valores de fábrica si faltan (10.13).
    fn defaults(&self) -> AutonomousDefaults;
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

fn voice_integer(runtime: &AgentRuntime, key: &str, default: i64) -> i64 {
    let config = runtime.voice_config();
    config
        .get(key)
        .filter(|value| truthy(value))
        .map_or(default, |value| ez_core::pyvalue::int_value(value, default))
}

impl ToolGuards for RuntimeGuards {
    fn tainted(&self) -> bool {
        self.runtime.tainted()
    }

    fn recent_user_messages(&self) -> Vec<(String, f64)> {
        let messages = self.runtime.recent_user_texts();
        if !messages.is_empty() {
            return messages;
        }
        // DECISION: sin historial (turnos de prueba o arranque) se usa el último
        // mensaje, como hacía la guarda de A1.
        let last = self.runtime.get_last_user_text();
        if last.is_empty() {
            Vec::new()
        } else {
            vec![(last, ez_core::pytime::timestamp())]
        }
    }

    fn confirm(&self, summary: &str, strong: bool) -> bool {
        let Some(gate) = self.runtime.confirm_gate() else {
            return false;
        };
        // DECISION (como `execute`): un fallo de la UI no debe tumbar la herramienta.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gate.ask(summary, strong, None)
        }))
        .unwrap_or(false)
    }

    fn default_cwd(&self) -> String {
        self.runtime
            .voice_config()
            .get("default_cwd")
            .filter(|value| truthy(value))
            .map_or_else(String::new, python_str)
    }

    fn defaults(&self) -> AutonomousDefaults {
        let defaults = AutonomousDefaults::default();
        AutonomousDefaults {
            max_rounds: voice_integer(&self.runtime, "autonomous_max_rounds", defaults.max_rounds)
                .clamp(MIN_ROUNDS, MAX_ROUNDS),
            max_minutes: voice_integer(
                &self.runtime,
                "autonomous_max_minutes",
                defaults.max_minutes,
            )
            .clamp(MIN_MINUTES, MAX_MINUTES),
            max_approvals: voice_integer(
                &self.runtime,
                "autonomous_max_approvals",
                defaults.max_approvals,
            )
            .max(1),
        }
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

/// ¿Hay una tarea autónoma en curso? La selección de herramientas lo usa (10.5).
pub fn task_active() -> bool {
    HOST.get()
        .and_then(|host| host.status())
        .is_some_and(|summary| summary.state.unfinished())
}

fn authorize() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(AUTHORIZE_RE).expect("regex de autorización"))
}

fn negation() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(NEGATION_RE).expect("regex de negación"))
}

/// ¿Los últimos mensajes del usuario autorizan el modo? (10.4)
///
/// Se miran los tres últimos de los últimos diez minutos; cualquier negación
/// entre ellos anula la autorización.
pub fn authorized(messages: &[(String, f64)], now: f64) -> bool {
    let recent: Vec<&(String, f64)> = messages
        .iter()
        .filter(|(_, at)| now - at <= AUTHORIZE_WINDOW_S)
        .collect();
    let first = recent.len().saturating_sub(AUTHORIZE_MESSAGES);
    let last = &recent[first..];
    if last.iter().any(|(text, _)| negation().is_match(text)) {
        return false;
    }
    last.iter().any(|(text, _)| authorize().is_match(text))
}

fn text(args: &Map<String, Value>, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(value)) => py_strip(value).to_string(),
        _ => String::new(),
    }
}

fn integer(args: &Map<String, Value>, key: &str, default: i64) -> i64 {
    match args.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.trunc() as i64))
            .unwrap_or(default),
        _ => default,
    }
}

fn flag(args: &Map<String, Value>, key: &str) -> bool {
    args.get(key).is_some_and(truthy)
}

/// El argumento manda; si falta, el valor de `voice.json`, acotado (10.13).
fn limit(args: &Map<String, Value>, key: &str, default: i64, low: i64, high: i64) -> i64 {
    match args.get(key) {
        Some(Value::Number(_)) => integer(args, key, default).clamp(low, high),
        _ => default.clamp(low, high),
    }
}

/// Las tres guardas previas a la confirmación (10.4).
fn check_activation(
    guards: &dyn ToolGuards,
    host: &dyn AutonomousHost,
    now: f64,
) -> Result<(), String> {
    if guards.tainted() {
        return Err(texts::TAINTED.to_string());
    }
    if !authorized(&guards.recent_user_messages(), now) {
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

/// Antes de `host.start`: el registro no puede haber cortado ya la llamada (10.5).
fn still_current(started: Instant) -> bool {
    started.elapsed().as_secs_f64() < START_TIMEOUT_S
}

fn tool(
    name: &str,
    description: &str,
    parameters: Value,
    level: i64,
    self_confirmed: bool,
    timeout_s: f64,
    handler: impl Fn(&Map<String, Value>) -> ToolResult + Send + Sync + 'static,
) -> ToolSpec {
    let mut spec = ToolSpec::new(
        name,
        description,
        parameters,
        level,
        Arc::new(move |args| Ok(handler(args).into())),
    );
    spec.self_confirmed = self_confirmed;
    spec.timeout_s = timeout_s;
    spec
}

const START_TASK_DESCRIPTION: &str = "Activa el modo autónomo para una tarea: vigilo al \
    ejecutor, apruebo una sola vez los permisos que pidan los agentes de esta tarea y verifico \
    el resultado con un verificador nuevo en cada ronda. Solo funciona si el usuario lo pidió \
    explícitamente en sus últimos mensajes.";
const START_TEAM_DESCRIPTION: &str = "Activa el modo autónomo con un equipo de varios papeles \
    (plan, dev, qa…) sobre el enjambre: el equipo ejecuta y yo verifico contra los criterios. \
    Solo funciona si el usuario lo pidió explícitamente en sus últimos mensajes.";
const STATUS_DESCRIPTION: &str = "Dice cómo va la tarea autónoma: estado, ronda, permisos \
    aprobados y motivo de la pausa.";
const STOP_DESCRIPTION: &str = "Detiene el modo autónomo y retira la autorización al instante; \
    con «accept» da por aceptado el resultado de una tarea cumplida.";
const RESUME_DESCRIPTION: &str = "Reanuda una tarea autónoma en pausa, con la prórroga de rondas \
    o de minutos que se indique.";

/// Las cinco herramientas; vacío mientras A3 no instale el bucle.
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
    let resume_host = Arc::clone(&host);
    vec![
        tool(
            "start_autonomous_task",
            START_TASK_DESCRIPTION,
            schema::task_parameters(),
            2,
            true,
            START_TIMEOUT_S,
            move |args| start_task(task_host.as_ref(), task_guards.as_ref(), args),
        ),
        tool(
            "start_autonomous_team",
            START_TEAM_DESCRIPTION,
            schema::team_parameters(),
            2,
            true,
            START_TIMEOUT_S,
            move |args| start_team(team_host.as_ref(), team_guards.as_ref(), args),
        ),
        tool(
            "autonomous_status",
            STATUS_DESCRIPTION,
            json!({"type": "object", "properties": {}}),
            0,
            false,
            ACTION_TIMEOUT_S,
            move |_| status(status_host.as_ref()),
        ),
        tool(
            "stop_autonomous_task",
            STOP_DESCRIPTION,
            schema::stop_parameters(),
            0,
            false,
            ACTION_TIMEOUT_S,
            move |args| stop(stop_host.as_ref(), args),
        ),
        tool(
            "resume_autonomous_task",
            RESUME_DESCRIPTION,
            schema::resume_parameters(),
            1,
            false,
            ACTION_TIMEOUT_S,
            move |args| resume(resume_host.as_ref(), args),
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
