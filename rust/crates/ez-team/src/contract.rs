//! Datos compartidos con `ez-arbiter` (contrato fijado por el elefante en
//! `docs/designs/espacio-elefante.md` §6 y §8). Los peces no cambian estos tipos.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::roles::Role;

/// Un puesto del equipo: una fila `[[papel]]` de `equipo.toml` ya validada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub role: Role,
    /// Id del agente en `agentes.toml` (`"opencode"`, `"claude"`…).
    pub agent: String,
    pub model: String,
    pub effort: Option<String>,
    /// Peces simultáneos; solo `Developer` admite más de 1.
    pub count: u32,
}

/// Lo que necesita la plantilla `roles/<papel>.md` para escribir
/// `<pez>.instrucciones.md`. Todas las rutas son absolutas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionContext {
    pub role: Role,
    /// Id del pez (`dev-r1-2`).
    pub fish: String,
    pub round: u32,
    /// Carpeta del espacio (raíz del proyecto).
    pub folder: PathBuf,
    /// `.ez/tareas/<tarea>/encargo.md`.
    pub brief: PathBuf,
    /// `.ez/tareas/<tarea>/criterios.toml`.
    pub criteria: PathBuf,
    /// `.ez/tareas/<tarea>/plan.md` (puede no existir todavía).
    pub plan: PathBuf,
    /// `.ez/tareas/<tarea>/ronda-<n>/<pez>.encargo.md`.
    pub fish_brief: PathBuf,
    /// `.ez/tareas/<tarea>/ronda-<n>/<pez>.informe.md`.
    pub report: PathBuf,
    /// Informe del QA de esta ronda; solo para `QaValidator`.
    pub qa_report: Option<PathBuf>,
    /// `[validacion] mutaciones_minimas` de los criterios.
    pub min_mutations: u32,
}
