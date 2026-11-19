//! Papeles de los subagentes (contrato compartido con `ez-arbiter` y `ez-hub`).
//!
//! Lo fija el agente principal del desarrollo en `docs/designs/espacio-elefante.md`
//! §15.2 y §15.7. Los papeles son abiertos: los cinco incluidos viven en
//! `src/roles/<id>.md` y la persona o el agente principal de una tarea pueden añadir
//! más con el mismo formato (cabecera TOML entre líneas `+++` y cuerpo Markdown).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Ids de los papeles incluidos.
pub mod builtin {
    pub const PLANNER: &str = "planner";
    pub const PLAN_REVIEWER: &str = "plan_reviewer";
    pub const DEVELOPER: &str = "developer";
    pub const QA: &str = "qa";
    pub const QA_VALIDATOR: &str = "qa_validator";
    /// En el orden de sus etapas sugeridas.
    pub const ALL: [&str; 5] = [PLANNER, PLAN_REVIEWER, DEVELOPER, QA, QA_VALIDATOR];
}

/// Id de un papel: el nombre de su archivo sin `.md`, `[a-z][a-z0-9_]{1,23}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoleId(String);

impl RoleId {
    /// Valida y crea un id. Devuelve el motivo si no es válido.
    pub fn parse(id: &str) -> Result<RoleId, String> {
        let mut chars = id.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        let len = id.chars().count();
        if first_ok && rest_ok && (2..=24).contains(&len) {
            Ok(RoleId(id.to_string()))
        } else {
            Err(format!(
                "«{id}» no es un id de papel válido: usa de 2 a 24 minúsculas, números o «_», empezando por letra."
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Prefijo del id de subagente (`dev-r1-2`, `qa-r3`): los incluidos tienen uno corto;
    /// los demás usan su id recortado a 10 caracteres.
    pub fn short(&self) -> String {
        match self.0.as_str() {
            builtin::PLANNER => "plan".into(),
            builtin::PLAN_REVIEWER => "revplan".into(),
            builtin::DEVELOPER => "dev".into(),
            builtin::QA => "qa".into(),
            builtin::QA_VALIDATOR => "valqa".into(),
            other => other.chars().take(10).collect(),
        }
    }
}

impl TryFrom<String> for RoleId {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        RoleId::parse(&value)
    }
}

impl From<RoleId> for String {
    fn from(value: RoleId) -> Self {
        value.0
    }
}

impl fmt::Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Ejecutor (cambia el proyecto) o verificador (lo revisa sin cambiarlo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleKind {
    #[serde(rename = "ejecutor")]
    Executor,
    #[serde(rename = "verificador")]
    Verifier,
}

/// De dónde salió la definición (de mayor a menor prioridad).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleOrigin {
    /// `.ez/tareas/<tarea>/papeles/`, propuesto por el agente principal.
    Task,
    /// `%APPDATA%\EzAgent\hub\papeles\`, de la persona.
    Library,
    /// `rust/crates/ez-team/src/roles/`, incluido.
    Builtin,
}

/// Un papel ya leído de su archivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDef {
    pub id: RoleId,
    /// `nombre`: lo que ve la persona («Auditor de seguridad»).
    pub name: String,
    /// `tipo`.
    pub kind: RoleKind,
    /// `descripcion`.
    pub description: String,
    /// `puede_delegar` (solo ejecutores).
    pub can_delegate: bool,
    /// `escribe_pruebas`: sus archivos quedan protegidos frente a los ejecutores.
    pub writes_tests: bool,
    /// `mutaciones`: su informe exige mutaciones probadas y detectadas.
    pub mutations: bool,
    /// `solo_primera_ronda`.
    pub first_round_only: bool,
    /// `etapa` sugerida si el equipo no la da (por omisión: 1 ejecutor, 2 verificador).
    pub default_stage: u32,
    /// Instrucciones (cuerpo Markdown, con los marcadores de la plantilla).
    pub body: String,
    pub origin: RoleOrigin,
}

impl RoleDef {
    pub fn is_verifier(&self) -> bool {
        self.kind == RoleKind::Verifier
    }
}
