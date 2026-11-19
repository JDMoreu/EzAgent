//! `equipo.toml`: parse, validate against the inventory and build the approval card
//! (design §6).

use serde::Deserialize;

use crate::contract::Seat;
use crate::error::TeamError;
use crate::family::family;
use crate::inventory::Inventory;
use crate::roles::Role;

/// Problems found in a proposed team. Errors block the approval; warnings only show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamCheck {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct RawTeam {
    #[serde(default, rename = "papel")]
    seats: Vec<RawSeat>,
}

#[derive(Deserialize)]
struct RawSeat {
    papel: String,
    agente: String,
    modelo: String,
    #[serde(default)]
    esfuerzo: Option<String>,
    #[serde(default = "default_count")]
    cantidad: u32,
}

fn default_count() -> u32 {
    1
}

/// Parses `equipo.toml`. Unknown or repeated roles, `cantidad` out of range and empty
/// fields are errors that name the offending row.
pub fn parse(text: &str) -> Result<Vec<Seat>, TeamError> {
    let raw: RawTeam = toml::from_str(text)
        .map_err(|error| TeamError::Invalid(format!("equipo.toml no es válido: {error}.")))?;
    let mut seats: Vec<Seat> = Vec::with_capacity(raw.seats.len());
    for (index, row) in raw.seats.iter().enumerate() {
        let number = index + 1;
        let role = Role::parse(&row.papel).ok_or_else(|| {
            let known: Vec<&str> = Role::ALL.iter().map(|role| role.id()).collect();
            TeamError::Invalid(format!(
                "fila {number}: papel desconocido «{}». Usa uno de: {}.",
                row.papel,
                known.join(", ")
            ))
        })?;
        if row.cantidad == 0 {
            return Err(TeamError::Invalid(format!(
                "fila {number}: cantidad debe ser al menos 1."
            )));
        }
        if row.cantidad > 1 && role != Role::Developer {
            return Err(TeamError::Invalid(format!(
                "fila {number}: solo {} admite cantidad mayor que 1.",
                Role::Developer.label()
            )));
        }
        if seats.iter().any(|seat| seat.role == role) {
            return Err(TeamError::Invalid(format!(
                "fila {number}: el papel {} está repetido.",
                role.label()
            )));
        }
        if row.agente.trim().is_empty() || row.modelo.trim().is_empty() {
            return Err(TeamError::Invalid(format!(
                "fila {number}: agente y modelo son obligatorios."
            )));
        }
        let effort = row.esfuerzo.as_deref().map(str::trim);
        if effort.is_some_and(str::is_empty) {
            return Err(TeamError::Invalid(format!(
                "fila {number}: esfuerzo no puede estar vacío."
            )));
        }
        seats.push(Seat {
            role,
            agent: row.agente.trim().to_string(),
            model: row.modelo.trim().to_string(),
            effort: row
                .esfuerzo
                .as_deref()
                .map(str::trim)
                .filter(|effort| !effort.is_empty())
                .map(str::to_string),
            count: row.cantidad,
        });
    }
    Ok(seats)
}

/// Applies the rules of design §6: missing required roles, unavailable agents, models
/// outside the agent's list, QA sharing the developer's model, and the family warnings.
pub fn validate(seats: &[Seat], inventory: &Inventory) -> TeamCheck {
    let mut check = TeamCheck::default();
    for role in Role::ALL.into_iter().filter(|role| role.is_required()) {
        if !seats.iter().any(|seat| seat.role == role) {
            check
                .errors
                .push(format!("Falta el papel obligatorio: {}.", role.label()));
        }
    }
    for seat in seats {
        let Some(agent) = inventory.find(&seat.agent) else {
            check.errors.push(format!(
                "El agente «{}» del papel {} no está disponible.",
                seat.agent,
                seat.role.label()
            ));
            continue;
        };
        if !agent.models.is_empty() && !agent.models.contains(&seat.model) {
            check.errors.push(format!(
                "El modelo «{}» no está en la lista del agente {}.",
                seat.model, agent.profile.name
            ));
        }
        check_effort(seat, inventory, &mut check);
    }
    for role in [Role::Qa, Role::QaValidator] {
        let (Some(other), Some(developer)) = (find(seats, role), find(seats, Role::Developer))
        else {
            continue;
        };
        if other.model == developer.model {
            check.errors.push(format!(
                "{} y {} usan el mismo modelo «{}»; elige modelos distintos.",
                role.label(),
                Role::Developer.label(),
                other.model
            ));
        }
    }
    for role in [Role::Qa, Role::QaValidator] {
        warn_same_family(role, seats, inventory, &mut check);
    }
    if let (Some(qa), Some(validator)) = (find(seats, Role::Qa), find(seats, Role::QaValidator)) {
        if qa.model == validator.model {
            check.warnings.push(format!(
                "{} y {} usan el mismo modelo «{}»; conviene que el validador use otro.",
                Role::QaValidator.label(),
                Role::Qa.label(),
                qa.model
            ));
        }
    }
    check
}

fn find(seats: &[Seat], role: Role) -> Option<&Seat> {
    seats.iter().find(|seat| seat.role == role)
}

/// A seat with an effort needs a profile that can apply it (design §14.2).
fn check_effort(seat: &Seat, inventory: &Inventory, check: &mut TeamCheck) {
    let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) else {
        return;
    };
    // An unavailable agent already has its own error; do not pile a second one on it.
    let Some(agent) = inventory.find(&seat.agent) else {
        return;
    };
    let can_apply = !agent.profile.effort_args.is_empty()
        || agent
            .profile
            .env
            .values()
            .any(|value| value.contains("{esfuerzo}") || value.contains("{variante}"));
    if !can_apply {
        // DECISION: the name in the message is the profile's human name when the agent
        // is known; the id would be ambiguous for the person reading the card.
        check.errors.push(format!(
            "{} no puede aplicar el esfuerzo «{effort}»: quita `esfuerzo` o configura `esfuerzo_args` en agentes.toml.",
            agent.profile.name
        ));
    }
}

fn warn_same_family(role: Role, seats: &[Seat], inventory: &Inventory, check: &mut TeamCheck) {
    let (Some(seat), Some(developer)) = (find(seats, role), find(seats, Role::Developer)) else {
        return;
    };
    // DECISION: the same model is already an error; the warning is for different models
    // of one family only, so the person does not see the same problem twice.
    if seat.model == developer.model {
        return;
    }
    let (Some(agent), Some(developer_agent)) = (
        inventory.find(&seat.agent),
        inventory.find(&developer.agent),
    ) else {
        return;
    };
    let developer_family = family(&developer_agent.profile, &developer.model);
    let another_family = inventory.agents.iter().any(|available| {
        available
            .families
            .iter()
            .any(|name| *name != developer_family)
    });
    if another_family && family(&agent.profile, &seat.model) == developer_family {
        check.warnings.push(format!(
            "{} y {} son de la misma familia («{developer_family}»); hay otra familia disponible.",
            role.label(),
            Role::Developer.label()
        ));
    }
}

/// Approval card, with the exact format of design §6: the seat lines and then the
/// warnings. Errors are not part of the card: a proposal with errors is rejected to the
/// elephant instead of shown for approval.
pub fn card(task_title: &str, seats: &[Seat], check: &TeamCheck, inventory: &Inventory) -> String {
    let mut text = format!("Equipo propuesto para «{task_title}»:");
    for seat in seats {
        let name = inventory
            .find(&seat.agent)
            .map(|agent| agent.profile.name.as_str())
            .unwrap_or(&seat.agent);
        let mut line = format!("- {}: {name} · {}", seat.role.label(), seat.model);
        if let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) {
            line.push_str(&format!(" · esfuerzo {effort}"));
        }
        // DECISION: `×{cantidad}` appears only when `cantidad > 1`; §6 marks it optional
        // and printing `×1` on every line is noise.
        if seat.count > 1 {
            line.push_str(&format!(" · ×{}", seat.count));
        }
        text.push('\n');
        text.push_str(&line);
    }
    for warning in &check.warnings {
        text.push_str(&format!("\nAviso: {warning}"));
    }
    text
}

#[cfg(test)]
mod tests;
