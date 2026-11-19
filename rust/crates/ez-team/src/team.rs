//! `equipo.toml`: lectura, validación y tarjeta de aprobación (diseño §15.3).

use serde::Deserialize;

use crate::contract::Seat;
use crate::error::TeamError;
use crate::family::family;
use crate::inventory::Inventory;
use crate::roles::{builtin, RoleDef, RoleId};

/// Problemas de un equipo propuesto: los errores bloquean, los avisos solo se muestran.
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
    etapa: Option<u32>,
    #[serde(default)]
    delega_en: Vec<String>,
}

fn default_count() -> u32 {
    1
}

/// Lee `equipo.toml` y comprueba los papeles contra `roles` (los incluidos hoy).
/// Los errores nombran la fila responsable.
pub fn parse(text: &str, roles: &[RoleDef]) -> Result<Vec<Seat>, TeamError> {
    let raw: RawTeam = toml::from_str(text)
        .map_err(|error| TeamError::Invalid(format!("equipo.toml no es válido: {error}.")))?;
    let mut seats: Vec<Seat> = Vec::with_capacity(raw.seats.len());
    for (index, row) in raw.seats.iter().enumerate() {
        let number = index + 1;
        let role = RoleId::parse(&row.papel).map_err(|_| {
            let known: Vec<&str> = roles.iter().map(|def| def.id.as_str()).collect();
            TeamError::Invalid(format!(
                "fila {number}: papel desconocido «{}». Usa uno de: {}.",
                row.papel,
                known.join(", ")
            ))
        })?;
        let def = definition(roles, &role).ok_or_else(|| {
            TeamError::Invalid(format!(
                "fila {number}: el papel «{role}» no está disponible en esta tarea."
            ))
        })?;
        if row.cantidad == 0 {
            return Err(TeamError::Invalid(format!(
                "fila {number}: cantidad debe ser al menos 1."
            )));
        }
        if row.cantidad > 1 && def.is_verifier() {
            // DECISION: los verificadores informan una sola vez (§15.2), así que
            // `cantidad > 1` solo tiene sentido en papeles ejecutores.
            return Err(TeamError::Invalid(format!(
                "fila {number}: solo los papeles ejecutores admiten cantidad mayor que 1."
            )));
        }
        if seats.iter().any(|seat| seat.role == role) {
            return Err(TeamError::Invalid(format!(
                "fila {number}: el papel {role} está repetido."
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
        let stage = row.etapa.unwrap_or(def.default_stage);
        if stage == 0 {
            return Err(TeamError::Invalid(format!(
                "fila {number}: etapa debe ser al menos 1."
            )));
        }
        let mut delegates_to = Vec::with_capacity(row.delega_en.len());
        for name in &row.delega_en {
            let target = RoleId::parse(name).map_err(|_| {
                TeamError::Invalid(format!(
                    "fila {number}: «{name}» no es un id de papel válido en delega_en."
                ))
            })?;
            if !roles.iter().any(|def| def.id == target) {
                return Err(TeamError::Invalid(format!(
                    "fila {number}: el papel «{target}» de delega_en no está disponible."
                )));
            }
            delegates_to.push(target);
        }
        if !delegates_to.is_empty() && !def.can_delegate {
            return Err(TeamError::Invalid(format!(
                "fila {number}: el papel {role} no puede delegar (puede_delegar = false)."
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
            stage,
            delegates_to,
        });
    }
    for seat in &seats {
        for target in &seat.delegates_to {
            if !seats.iter().any(|other| other.role == *target) {
                return Err(TeamError::Invalid(format!(
                    "delega_en del papel {} nombra a «{target}», que no está en el equipo.",
                    seat.role
                )));
            }
        }
    }
    Ok(seats)
}

fn definition<'a>(roles: &'a [RoleDef], id: &RoleId) -> Option<&'a RoleDef> {
    roles.iter().find(|def| def.id == *id)
}

/// Aplica las reglas de §15.3 y las comprobaciones de inventario de §6.
pub fn validate(seats: &[Seat], inventory: &Inventory, roles: &[RoleDef]) -> TeamCheck {
    let mut check = TeamCheck::default();
    let has_executor = seats
        .iter()
        .any(|seat| definition(roles, &seat.role).is_some_and(|def| !def.is_verifier()));
    let has_verifier = seats
        .iter()
        .any(|seat| definition(roles, &seat.role).is_some_and(RoleDef::is_verifier));
    if !has_executor {
        check
            .errors
            .push("Falta al menos un papel ejecutor en el equipo.".to_string());
    }
    if !has_verifier {
        check
            .errors
            .push("Falta al menos un papel verificador en el equipo.".to_string());
    }
    for seat in seats {
        let Some(def) = definition(roles, &seat.role) else {
            check.errors.push(format!(
                "El papel «{}» no está disponible en esta tarea.",
                seat.role
            ));
            continue;
        };
        check_agent(seat, def, inventory, &mut check);
    }
    let executors: Vec<&Seat> = seats
        .iter()
        .filter(|seat| definition(roles, &seat.role).is_some_and(|def| !def.is_verifier()))
        .collect();
    for seat in seats {
        let Some(def) = definition(roles, &seat.role) else {
            continue;
        };
        if !def.is_verifier() {
            continue;
        }
        if executors.iter().any(|other| other.model == seat.model) {
            check.errors.push(format!(
                "{} y un papel ejecutor usan el mismo modelo «{}»; elige modelos distintos.",
                def.name, seat.model
            ));
        }
        warn_same_family(def, seat, &executors, inventory, &mut check);
    }
    if let (Some(qa), Some(validator)) =
        (find(seats, builtin::QA), find(seats, builtin::QA_VALIDATOR))
    {
        if qa.model == validator.model {
            check.warnings.push(format!(
                "Validador de QA y QA usan el mismo modelo «{}»; conviene que el validador use otro.",
                qa.model
            ));
        }
    }
    check
}

fn find<'a>(seats: &'a [Seat], role: &str) -> Option<&'a Seat> {
    seats.iter().find(|seat| seat.role.as_str() == role)
}

fn check_agent(seat: &Seat, def: &RoleDef, inventory: &Inventory, check: &mut TeamCheck) {
    let Some(agent) = inventory.find(&seat.agent) else {
        check.errors.push(format!(
            "El agente «{}» del papel {} no está disponible.",
            seat.agent, def.name
        ));
        return;
    };
    if !agent.models.is_empty() && !agent.models.contains(&seat.model) {
        check.errors.push(format!(
            "El modelo «{}» no está en la lista del agente {}.",
            seat.model, agent.profile.name
        ));
    }
    let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) else {
        return;
    };
    let can_apply = !agent.profile.effort_args.is_empty()
        || agent
            .profile
            .env
            .values()
            .any(|value| value.contains("{esfuerzo}") || value.contains("{variante}"));
    if !can_apply {
        // DECISION: el mensaje usa el nombre humano del perfil: el id sería ambiguo
        // para la persona que lee la tarjeta.
        check.errors.push(format!(
            "{} no puede aplicar el esfuerzo «{effort}»: quita `esfuerzo` o configura `esfuerzo_args` en agentes.toml.",
            agent.profile.name
        ));
    }
}

fn warn_same_family(
    def: &RoleDef,
    seat: &Seat,
    executors: &[&Seat],
    inventory: &Inventory,
    check: &mut TeamCheck,
) {
    // DECISION: el mismo modelo ya es un error; el aviso compara solo modelos
    // distintos de una misma familia para no repetir el problema.
    if executors.iter().any(|other| other.model == seat.model) {
        return;
    }
    let Some(agent) = inventory.find(&seat.agent) else {
        return;
    };
    for executor in executors {
        let Some(executor_agent) = inventory.find(&executor.agent) else {
            continue;
        };
        let executor_family = family(&executor_agent.profile, &executor.model);
        let another_family = inventory.agents.iter().any(|available| {
            available
                .families
                .iter()
                .any(|name| *name != executor_family)
        });
        if another_family && family(&agent.profile, &seat.model) == executor_family {
            check.warnings.push(format!(
                "{} y {} son de la misma familia («{executor_family}»); hay otra familia disponible.",
                def.name,
                executor.model
            ));
        }
    }
}

/// Tarjeta de aprobación: una línea por puesto con ` · etapa {n}` y los avisos.
pub fn card(
    task_title: &str,
    seats: &[Seat],
    check: &TeamCheck,
    inventory: &Inventory,
    roles: &[RoleDef],
) -> String {
    let mut text = format!("Equipo propuesto para «{task_title}»:");
    for seat in seats {
        let name = definition(roles, &seat.role)
            .map(|def| def.name.as_str())
            .unwrap_or(seat.role.as_str());
        let agent_name = inventory
            .find(&seat.agent)
            .map(|agent| agent.profile.name.as_str())
            .unwrap_or(&seat.agent);
        let mut line = format!(
            "- {name} ({role}): {agent_name} · {model} · etapa {stage}",
            role = seat.role,
            model = seat.model,
            stage = seat.stage
        );
        if let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) {
            line.push_str(&format!(" · esfuerzo {effort}"));
        }
        // DECISION: `×{cantidad}` solo aparece cuando es mayor que 1; imprimir
        // `×1` en todas las líneas es ruido.
        if seat.count > 1 {
            line.push_str(&format!(" · ×{}", seat.count));
        }
        if !seat.delegates_to.is_empty() {
            let names: Vec<&str> = seat.delegates_to.iter().map(|id| id.as_str()).collect();
            line.push_str(&format!(" · delega en {}", names.join(", ")));
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
