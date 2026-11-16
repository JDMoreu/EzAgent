//! Modo autónomo, versión 2: estado, artefactos, veredicto, textos y
//! herramientas con sus guardas de activación (sección 10 del diseño aprobado).
//!
//! Este crate no ejecuta el bucle: eso es A3 en `ez-app`. Aquí viven las
//! transiciones puras (nunca tocan Herdr ni ventanas), los artefactos en disco y
//! las cinco herramientas, que hablan con el bucle por [`AutonomousHost`].
//!
//! DECISION: el host se inyecta al construir el grupo y hoy solo existe como
//! doble de pruebas; [`tools::install_host`] es el punto que usará A3.

pub mod artifacts;
pub mod audit;
pub mod state;
pub mod texts;
pub mod tools;
pub mod verdict;

#[cfg(test)]
mod tests;

pub use state::{
    Action, AutonomousState, Incident, Limits, Mode, ScopeEntry, ScopeRole, Summary, TaskState,
    Team, TeamRole,
};
pub use tools::{build_autonomous_tools, AutonomousDefaults, ToolGuards};
pub use verdict::{Finding, Severity, Verdict, VerdictError, VerdictSpec};

/// Lo que las herramientas piden al bucle (10.5).
///
/// `start` lanza el ejecutor o crea el equipo y devuelve el id de la tarea;
/// `status` resume la tarea en curso; `resume` y `stop` son los dos verbos del
/// usuario. Ningún método toca la UI: el bucle corre en `ezagent-autonomo`.
// DECISION: `resume` lleva la prórroga y `stop` lleva `accept` porque 10.5 y
// 10.13 los piden como argumentos de la herramienta y el bucle los aplica.
pub trait AutonomousHost: Send + Sync {
    fn start(&self, spec: &StartSpec) -> Result<String, String>;
    fn start_team(&self, spec: &TeamSpec) -> Result<String, String>;
    fn status(&self) -> Option<Summary>;
    /// «Sigue», con la prórroga que pida el usuario (0 = sin prórroga).
    fn resume(&self, extend_rounds: i64, extend_minutes: i64) -> Result<(), String>;
    /// «Para»; con `accept`, da por aceptado el resultado de una tarea cumplida.
    fn stop(&self, reason: &str, accept: bool) -> Result<(), String>;
}

/// Un criterio de terminado (10.3): qué se cumple y cómo se comprueba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub text: String,
    pub check: String,
}

/// Datos de `start_autonomous_task` (10.2 y 10.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartSpec {
    pub task: String,
    pub done_criteria: Vec<Criterion>,
    pub agent: String,
    pub space: String,
    pub workdir: String,
    pub terminal: Option<String>,
    pub verifier_agent: String,
    pub max_rounds: i64,
    pub max_minutes: i64,
    pub max_approvals: i64,
}

/// Datos de `start_autonomous_team` (10.2, 10.3 y 10.18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSpec {
    pub task: String,
    pub done_criteria: Vec<Criterion>,
    pub roles: Vec<RoleSpec>,
    pub space: String,
    pub workdir: String,
    pub max_rounds: i64,
    pub max_minutes: i64,
    pub max_approvals: i64,
}

/// Un papel del equipo autónomo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleSpec {
    pub role: String,
    pub agent: String,
    pub model: Option<String>,
}
