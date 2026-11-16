//! Modo autónomo, fase A1: estado, artefactos, veredicto, textos y herramientas
//! con sus guardas de activación (9.2, 9.3 y 9.6 del diseño aprobado).
//!
//! Este crate no ejecuta el bucle: eso es A3 en `ez-app`. Aquí viven las
//! transiciones puras (nunca tocan Herdr ni ventanas), los artefactos en disco y
//! las cuatro herramientas, que hablan con el bucle por [`AutonomousHost`].
//!
//! DECISION: el host se inyecta al construir el grupo y en A1 solo existe como
//! doble de pruebas; [`tools::install_host`] es el punto que usará A3.

pub mod artifacts;
pub mod state;
pub mod texts;
pub mod tools;
pub mod verdict;

#[cfg(test)]
mod tests;

pub use state::{Action, AutonomousState, Incident, Mode, Summary, TaskState, Team, TeamRole};
pub use tools::{build_autonomous_tools, ToolGuards};
pub use verdict::{Finding, Severity, Verdict, VerdictError};

/// Lo que las herramientas piden al bucle (9.2).
///
/// `start` lanza el ejecutor o crea el equipo y devuelve el id de la tarea;
/// `status` resume la tarea en curso; `resume` y `stop` son los dos verbos del
/// usuario. Ningún método toca la UI: el bucle corre en `ezagent-autonomo`.
pub trait AutonomousHost: Send + Sync {
    fn start(&self, spec: &StartSpec) -> Result<String, String>;
    fn start_team(&self, spec: &TeamSpec) -> Result<String, String>;
    fn status(&self) -> Option<Summary>;
    fn resume(&self) -> Result<(), String>;
    fn stop(&self, reason: &str) -> Result<(), String>;
}

/// Datos de `start_autonomous_task` (9.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartSpec {
    pub task: String,
    pub done_criteria: Vec<String>,
    pub agent: String,
    pub space: String,
    pub terminal: Option<String>,
    pub verifier_agent: String,
    pub max_rounds: i64,
    pub max_minutes: i64,
}

/// Datos de `start_autonomous_team` (9.2 y 9.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSpec {
    pub task: String,
    pub done_criteria: Vec<String>,
    pub roles: Vec<RoleSpec>,
    pub space: String,
    pub max_rounds: i64,
    pub max_minutes: i64,
}

/// Un papel del equipo autónomo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleSpec {
    pub role: String,
    pub agent: String,
    pub model: Option<String>,
}
