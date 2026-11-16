//! `start_autonomous_task` y `start_autonomous_team` (10.2 y 10.3).

use std::time::Instant;

use ez_core::pytime::timestamp;
use serde_json::{json, Map, Value};

use crate::registry::ToolResult;

use super::super::texts;
use super::super::{AutonomousHost, Criterion, RoleSpec, StartSpec, TeamSpec, ToolGuards};
use super::{
    check_activation, limit, still_current, text, MAX_CRITERIA, MAX_MINUTES, MAX_ROLES, MAX_ROUNDS,
    MIN_MINUTES, MIN_ROUNDS,
};

fn criteria(args: &Map<String, Value>) -> Result<Vec<Criterion>, String> {
    let Some(Value::Array(items)) = args.get("done_criteria") else {
        return Err("Indica los criterios en «done_criteria».".to_string());
    };
    if items.len() > MAX_CRITERIA {
        return Err(format!(
            "Un encargo admite como máximo {MAX_CRITERIA} criterios."
        ));
    }
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Some(object) = item.as_object() else {
            return Err("Cada criterio debe ser un objeto con «text» y «check».".to_string());
        };
        let criterion = text(object, "text");
        if criterion.is_empty() {
            return Err("Los criterios no pueden estar vacíos.".to_string());
        }
        let check = text(object, "check");
        if check.is_empty() {
            return Err(texts::MISSING_CHECK.to_string());
        }
        list.push(Criterion {
            text: criterion,
            check,
        });
    }
    if list.is_empty() {
        return Err("Indica al menos un criterio verificable.".to_string());
    }
    Ok(list)
}

fn roles(args: &Map<String, Value>) -> Result<Vec<RoleSpec>, String> {
    let Some(Value::Array(items)) = args.get("roles") else {
        return Err("Indica los papeles del equipo en «roles».".to_string());
    };
    if items.len() as i64 > MAX_ROLES {
        return Err(format!(
            "Un equipo autónomo admite como máximo {MAX_ROLES} papeles."
        ));
    }
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Some(object) = item.as_object() else {
            return Err("Cada papel debe ser un objeto.".to_string());
        };
        let role = text(object, "role");
        let agent = text(object, "agent");
        if role.is_empty() || agent.is_empty() {
            return Err("Cada papel necesita «role» y «agent».".to_string());
        }
        let model = text(object, "model");
        list.push(RoleSpec {
            role,
            agent,
            model: (!model.is_empty()).then_some(model),
        });
    }
    if list.is_empty() {
        return Err("Indica al menos un papel.".to_string());
    }
    Ok(list)
}

/// «plan: codex, dev: opencode (big)» para la confirmación del equipo.
fn roles_label(roles: &[RoleSpec]) -> String {
    roles
        .iter()
        .map(|role| match &role.model {
            Some(model) => format!("{}: {} ({model})", role.role, role.agent),
            None => format!("{}: {}", role.role, role.agent),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// La carpeta la elige el argumento, si no `default_cwd`, si no error (10.2).
fn workdir(args: &Map<String, Value>, guards: &dyn ToolGuards) -> Result<String, String> {
    let value = text(args, "workdir");
    if !value.is_empty() {
        return Ok(value);
    }
    let fallback = guards.default_cwd();
    if fallback.is_empty() {
        Err(texts::WORKDIR_MISSING.to_string())
    } else {
        Ok(fallback)
    }
}

pub fn start_task(
    host: &dyn AutonomousHost,
    guards: &dyn ToolGuards,
    args: &Map<String, Value>,
) -> ToolResult {
    let started = Instant::now();
    let task = text(args, "task");
    if task.is_empty() {
        return ToolResult::err("Falta «task»: el encargo de la tarea.");
    }
    let agent = text(args, "agent");
    let space = text(args, "space");
    if agent.is_empty() || space.is_empty() {
        return ToolResult::err("Indica el agente y el espacio de la tarea.");
    }
    let done_criteria = match criteria(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    if let Err(error) = check_activation(guards, host, timestamp()) {
        return ToolResult::err(error);
    }
    let workdir = match workdir(args, guards) {
        Ok(workdir) => workdir,
        Err(error) => return ToolResult::err(error),
    };
    let defaults = guards.defaults();
    let max_rounds = limit(
        args,
        "max_rounds",
        defaults.max_rounds,
        MIN_ROUNDS,
        MAX_ROUNDS,
    );
    let max_minutes = limit(
        args,
        "max_minutes",
        defaults.max_minutes,
        MIN_MINUTES,
        MAX_MINUTES,
    );
    let verifier = text(args, "verifier_agent");
    let summary = texts::confirm(
        &task,
        &agent,
        &space,
        &workdir,
        &done_criteria,
        max_rounds,
        max_minutes,
    );
    if !guards.confirm(&summary, true) {
        return ToolResult::err(texts::DECLINED);
    }
    if !still_current(started) {
        return ToolResult::err(texts::TIMED_OUT);
    }
    let spec = StartSpec {
        task,
        done_criteria,
        agent: agent.clone(),
        space,
        workdir,
        terminal: Some(text(args, "terminal")).filter(|value| !value.is_empty()),
        verifier_agent: if verifier.is_empty() { agent } else { verifier },
        max_rounds,
        max_minutes,
        max_approvals: defaults.max_approvals,
    };
    match host.start(&spec) {
        Ok(id) => ToolResult::ok(json!({"id": id, "message": texts::START})),
        Err(error) => ToolResult::err(error),
    }
}

pub fn start_team(
    host: &dyn AutonomousHost,
    guards: &dyn ToolGuards,
    args: &Map<String, Value>,
) -> ToolResult {
    let started = Instant::now();
    let task = text(args, "task");
    if task.is_empty() {
        return ToolResult::err("Falta «task»: el encargo de la tarea.");
    }
    let space = text(args, "space");
    if space.is_empty() {
        return ToolResult::err("Indica el espacio de la tarea.");
    }
    let done_criteria = match criteria(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    let roles = match roles(args) {
        Ok(list) => list,
        Err(error) => return ToolResult::err(error),
    };
    if let Err(error) = check_activation(guards, host, timestamp()) {
        return ToolResult::err(error);
    }
    let workdir = match workdir(args, guards) {
        Ok(workdir) => workdir,
        Err(error) => return ToolResult::err(error),
    };
    let defaults = guards.defaults();
    let max_rounds = limit(
        args,
        "max_rounds",
        defaults.max_rounds,
        MIN_ROUNDS,
        MAX_ROUNDS,
    );
    let max_minutes = limit(
        args,
        "max_minutes",
        defaults.max_minutes,
        MIN_MINUTES,
        MAX_MINUTES,
    );
    let summary = texts::confirm_team(
        &task,
        &roles_label(&roles),
        &space,
        &workdir,
        &done_criteria,
        max_rounds,
        max_minutes,
    );
    if !guards.confirm(&summary, true) {
        return ToolResult::err(texts::DECLINED);
    }
    if !still_current(started) {
        return ToolResult::err(texts::TIMED_OUT);
    }
    let spec = TeamSpec {
        task,
        done_criteria,
        roles,
        space,
        workdir,
        max_rounds,
        max_minutes,
        max_approvals: defaults.max_approvals,
    };
    match host.start_team(&spec) {
        Ok(id) => ToolResult::ok(json!({"id": id, "message": texts::START})),
        Err(error) => ToolResult::err(error),
    }
}
