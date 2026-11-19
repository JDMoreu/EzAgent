//! Datos compartidos con `ez-arbiter` y `ez-hub` (contrato fijado en
//! `docs/designs/espacio-elefante.md` §15.3 y §15.7). Los peces no cambian estos tipos.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::roles::RoleId;

/// Un puesto del equipo: una fila `[[papel]]` de `equipo.toml` ya validada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub role: RoleId,
    /// Id del agente en `agentes.toml` (`"opencode"`, `"claude"`…).
    pub agent: String,
    pub model: String,
    pub effort: Option<String>,
    /// Subagentes simultáneos de este papel (≥ 1).
    pub count: u32,
    /// Orden dentro de la ronda (≥ 1).
    pub stage: u32,
    /// Papeles que puede lanzar (solo si el papel puede delegar).
    pub delegates_to: Vec<RoleId>,
}

/// Lo que necesitan las plantillas para escribir `<id>.instrucciones.md`.
/// Todas las rutas son absolutas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionContext {
    pub role: RoleId,
    /// `{papel_nombre}`: el `nombre` del papel.
    pub role_name: String,
    /// `{id}`: id del subagente (`dev-r1-2`).
    pub fish: String,
    /// Id del subagente que lo delegó; `None` si lo delegó el agente principal.
    pub parent: Option<String>,
    /// `{ronda}`.
    pub round: u32,
    /// `{carpeta}`: carpeta de trabajo (el worktree del agente principal).
    pub folder: PathBuf,
    /// `{encargo}`: `.ez/tareas/<tarea>/encargo.md`.
    pub brief: PathBuf,
    /// `{criterios}`: `.ez/tareas/<tarea>/criterios.toml`.
    pub criteria: PathBuf,
    /// `{plan}`: `.ez/tareas/<tarea>/plan.md` (puede no existir todavía).
    pub plan: PathBuf,
    /// `{mi_encargo}`: `.ez/tareas/<tarea>/ronda-<n>/<id>.encargo.md`.
    pub fish_brief: PathBuf,
    /// `{informe}`: `.ez/tareas/<tarea>/ronda-<n>/<id>.informe.md`.
    pub report: PathBuf,
    /// `{informe_qa}`: informe del verificador anterior que debe validar (papeles con
    /// `mutaciones`); `None` para los demás.
    pub qa_report: Option<PathBuf>,
    /// `{mutaciones_minimas}`.
    pub min_mutations: u32,
    /// `{papeles_delegables}`: papeles que puede lanzar (vacío si no delega).
    pub delegable_roles: Vec<RoleId>,
    /// `{borradores}`: `.ez/borrador/<id>/` para los encargos de sus subagentes.
    pub drafts: PathBuf,
}
