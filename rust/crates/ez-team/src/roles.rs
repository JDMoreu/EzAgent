//! Papeles del espacio de trabajo (contrato compartido con `ez-arbiter`).
//!
//! Lo fija el elefante en el diseño (`docs/designs/espacio-elefante.md` §2); los
//! peces no cambian sus variantes, ids ni nombres.

use serde::{Deserialize, Serialize};

/// Papel de un pez dorado. El elefante no es un papel de pez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Planner,
    PlanReviewer,
    Developer,
    Qa,
    QaValidator,
}

impl Role {
    /// Todos los papeles, en el orden de una ronda.
    pub const ALL: [Role; 5] = [
        Role::Planner,
        Role::PlanReviewer,
        Role::Developer,
        Role::Qa,
        Role::QaValidator,
    ];

    /// Id del papel en `equipo.toml`, en `ez fish <papel>` y en el protocolo.
    pub fn id(self) -> &'static str {
        match self {
            Role::Planner => "planner",
            Role::PlanReviewer => "plan_reviewer",
            Role::Developer => "developer",
            Role::Qa => "qa",
            Role::QaValidator => "qa_validator",
        }
    }

    /// Prefijo del id de pez (`dev-r1-2`, `qa-r3`…).
    pub fn short(self) -> &'static str {
        match self {
            Role::Planner => "plan",
            Role::PlanReviewer => "revplan",
            Role::Developer => "dev",
            Role::Qa => "qa",
            Role::QaValidator => "valqa",
        }
    }

    /// Nombre que ve la persona.
    pub fn label(self) -> &'static str {
        match self {
            Role::Planner => "Planificador",
            Role::PlanReviewer => "Revisor del plan",
            Role::Developer => "Desarrollador",
            Role::Qa => "QA",
            Role::QaValidator => "Validador de QA",
        }
    }

    /// Papeles verificadores: siempre un pez nuevo, un solo informe y nunca `ez send`.
    pub fn is_verifier(self) -> bool {
        matches!(self, Role::PlanReviewer | Role::Qa | Role::QaValidator)
    }

    /// Papeles obligatorios en un equipo.
    pub fn is_required(self) -> bool {
        !matches!(self, Role::PlanReviewer)
    }

    /// Lee un id (`"qa_validator"`). Devuelve `None` si no es un papel.
    pub fn parse(id: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|role| role.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::Role;

    #[test]
    fn ids_round_trip() {
        for role in Role::ALL {
            assert_eq!(Role::parse(role.id()), Some(role));
        }
        assert_eq!(Role::parse("elephant"), None);
    }

    #[test]
    fn verifiers_and_required() {
        assert!(Role::Qa.is_verifier() && Role::QaValidator.is_verifier());
        assert!(!Role::Developer.is_verifier());
        assert!(!Role::PlanReviewer.is_required());
    }
}
