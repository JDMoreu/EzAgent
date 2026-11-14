//! Port of `src/ezagent/swarm/view.py` (F3-12).
//!
//! Compact board and health check. All text is built from the stored state;
//! never from a model.

use std::collections::{HashMap, HashSet};

use crate::db::{Db, DbResult};
use crate::incidents::describe;
use crate::models::{swarm_state, Member, Message, SwarmError, Task, BROADCAST};
use crate::protocol::role_label;

/// A member with no signs of life for this long is stalled.
pub const STALL_SECS: f64 = 900.0;
/// A ready task nobody takes for this long is an issue.
pub const UNCLAIMED_SECS: f64 = 180.0;
/// A direct question without answer for this long is an issue.
pub const ASK_SECS: f64 = 600.0;

fn minutes(seconds: f64) -> i64 {
    ((seconds / 60.0).round_ties_even() as i64).max(1)
}

fn parts_by_parent(tasks: &[Task]) -> HashMap<i64, Vec<Task>> {
    let mut grouped: HashMap<i64, Vec<Task>> = HashMap::new();
    for task in tasks {
        if task.parent != 0 {
            grouped.entry(task.parent).or_default().push(task.clone());
        }
    }
    grouped
}

/// Compact board with defaults.
pub fn board(store: &Db, swarm: &str, member: &str) -> DbResult<String> {
    board_recent(store, swarm, member, 4)
}

/// Compact board: state, members, tasks, reservations and last messages.
pub fn board_recent(store: &Db, swarm: &str, member: &str, recent: i64) -> DbResult<String> {
    let info = match store.get(swarm)? {
        Some(info) => info,
        None => {
            return Err(SwarmError::new(format!("No existe el enjambre {}.", repr(swarm))).into())
        }
    };
    let tasks = store.tasks(swarm)?;
    let parts = parts_by_parent(&tasks);
    let leaves: Vec<&Task> = tasks
        .iter()
        .filter(|task| !parts.contains_key(&task.id))
        .collect();
    let done = leaves.iter().filter(|task| task.state == "done").count();
    let mut lines = vec![format!(
        "Enjambre {} · {} · {}",
        info.id,
        swarm_state(&info.status),
        info.goal
    )];
    let members = store.members(swarm)?;
    if !members.is_empty() {
        let labels: Vec<String> = members.iter().map(Member::label).collect();
        lines.push(format!("Miembros: {}", labels.join(" · ")));
    }
    if !tasks.is_empty() {
        lines.push(format!("Tareas {done}/{}:", leaves.len()));
    }
    push_task_lines(&mut lines, &tasks, &parts);
    let leases = store.leases(swarm)?;
    if !leases.is_empty() {
        let items: Vec<String> = leases
            .iter()
            .map(|lease| format!("{} @{}", lease.path, lease.member))
            .collect();
        lines.push(format!("Reservas: {}", items.join(" · ")));
    }
    let last: Vec<String> = store
        .history(swarm, 0, recent)?
        .iter()
        .map(Message::line)
        .collect();
    if !last.is_empty() {
        lines.push("Últimos:".to_string());
        lines.extend(last);
    }
    if !member.is_empty() {
        lines.push(format!(
            "Sin leer para {member}: {}",
            store.unread(swarm, member)?
        ));
    }
    Ok(lines.join("\n"))
}

fn push_task_lines(lines: &mut Vec<String>, tasks: &[Task], parts: &HashMap<i64, Vec<Task>>) {
    for task in tasks {
        if task.parent != 0 {
            continue;
        }
        let below = parts.get(&task.id).cloned().unwrap_or_default();
        let count = if below.is_empty() {
            String::new()
        } else {
            let done_parts = below.iter().filter(|part| part.state == "done").count();
            format!("{done_parts}/{}", below.len())
        };
        lines.push(task.line(&count));
        for part in &below {
            lines.push(format!("  {}", part.line("")));
        }
    }
}

/// Something that needs attention. `key` does not change with the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Stable key.
    pub key: String,
    /// Text shown to the user.
    pub text: String,
    /// Related member, if any.
    pub member: String,
}

impl Issue {
    fn new(key: impl Into<String>, text: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: text.into(),
            member: member.into(),
        }
    }
}

/// Result of the health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// `empty`, `ok`, `attention` or `done`.
    pub state: String,
    /// Finished leaf tasks.
    pub done: usize,
    /// Leaf tasks.
    pub total: usize,
    /// Members working by task.
    pub working: Vec<String>,
    /// Issues found.
    pub found: Vec<Issue>,
}

impl Health {
    /// Texts of the issues.
    pub fn issues(&self) -> Vec<String> {
        self.found.iter().map(|issue| issue.text.clone()).collect()
    }

    /// Short phrase for voice.
    pub fn spoken(&self) -> String {
        if self.state == "empty" {
            return "El enjambre todavía no tiene tareas.".to_string();
        }
        if self.state == "done" {
            return format!(
                "El enjambre terminó: {} de {} tareas hechas.",
                self.total, self.total
            );
        }
        let mut text = format!("Van {} de {} tareas.", self.done, self.total);
        if !self.working.is_empty() {
            text.push_str(" Trabajando: ");
            text.push_str(&self.working.join(", "));
            text.push('.');
        }
        let issues = self.issues();
        if issues.is_empty() {
            return format!("{text} Todo en orden.");
        }
        let extra = issues.len() - 1;
        let more = if extra > 0 {
            format!(" Y {extra} aviso{} más.", if extra > 1 { "s" } else { "" })
        } else {
            String::new()
        };
        format!("{text} Atención: {}{more}", issues[0])
    }
}

fn task_issues(
    tasks: &[Task],
    members: &[Member],
    parts: &HashMap<i64, Vec<Task>>,
    now: f64,
) -> Vec<Issue> {
    let by_id: HashMap<i64, &Task> = tasks.iter().map(|task| (task.id, task)).collect();
    let by_name: HashMap<&str, &Member> = members
        .iter()
        .map(|member| (member.name.as_str(), member))
        .collect();
    let roles: HashSet<&str> = members
        .iter()
        .filter(|member| member.state != "gone")
        .map(|member| member.role.as_str())
        .collect();
    let mut issues = Vec::new();
    for task in tasks {
        if let Some(issue) = task_issue(task, &by_id, &by_name, &roles, parts, now) {
            issues.push(issue);
        }
    }
    issues
}

fn task_issue(
    task: &Task,
    by_id: &HashMap<i64, &Task>,
    by_name: &HashMap<&str, &Member>,
    roles: &HashSet<&str>,
    parts: &HashMap<i64, Vec<Task>>,
    now: f64,
) -> Option<Issue> {
    let key = format!("T{}", task.id);
    if task.state == "failed" {
        let detail = if task.result.is_empty() {
            &task.title
        } else {
            &task.result
        };
        return Some(Issue::new(
            format!("failed:{key}"),
            format!("{key} falló: {detail}"),
            &task.owner,
        ));
    }
    if task.state == "claimed" {
        let idle = by_name
            .get(task.owner.as_str())
            .map_or(0.0, |owner| now - owner.seen);
        return (idle > STALL_SECS).then(|| {
            Issue::new(
                format!("stall:{key}"),
                format!(
                    "{} lleva {} min sin señales en {key}.",
                    task.owner,
                    minutes(idle)
                ),
                &task.owner,
            )
        });
    }
    if task.state != "pending" || parts.contains_key(&task.id) {
        return None;
    }
    pending_issue(task, by_id, by_name, roles)
}

fn pending_issue(
    task: &Task,
    by_id: &HashMap<i64, &Task>,
    by_name: &HashMap<&str, &Member>,
    roles: &HashSet<&str>,
) -> Option<Issue> {
    let key = format!("T{}", task.id);
    let mut deps: HashSet<i64> = task.deps.iter().copied().collect();
    if let Some(above) = by_id.get(&task.parent) {
        deps.extend(above.deps.iter().copied());
    }
    let broken = deps
        .iter()
        .filter(|dep| by_id.get(dep).is_some_and(|task| task.state == "failed"))
        .min()
        .copied();
    if let Some(broken) = broken {
        return Some(Issue::new(
            format!("broken:{key}"),
            format!("{key} no puede empezar: depende de T{broken}, que falló."),
            "",
        ));
    }
    if !task.owner.is_empty() && !by_name.contains_key(task.owner.as_str()) {
        return Some(Issue::new(
            format!("orphan:{key}"),
            format!("{key} está a nombre de {}, que no es miembro.", task.owner),
            "",
        ));
    }
    if task.owner.is_empty() && !task.role.is_empty() && !roles.contains(task.role.as_str()) {
        return Some(Issue::new(
            format!("orphan:{key}"),
            format!("{key} es para el rol «{}» y nadie lo tiene.", task.role),
            "",
        ));
    }
    None
}

fn unclaimed_issues(
    store: &Db,
    swarm: &str,
    tasks: &[Task],
    members: &[Member],
    now: f64,
) -> DbResult<Vec<Issue>> {
    let by_id: HashMap<i64, &Task> = tasks.iter().map(|task| (task.id, task)).collect();
    let mut issues = Vec::new();
    let mut flagged: HashSet<i64> = HashSet::new();
    for member in members {
        if ["gone", "working"].contains(&member.state.as_str()) {
            continue;
        }
        for task in store.ready(swarm, &member.name)? {
            if flagged.contains(&task.id) {
                continue;
            }
            let mut deps: HashSet<i64> = task.deps.iter().copied().collect();
            if let Some(above) = by_id.get(&task.parent) {
                deps.extend(above.deps.iter().copied());
            }
            let since = std::iter::once(task.updated)
                .chain(
                    deps.iter()
                        .filter_map(|dep| by_id.get(dep))
                        .map(|task| task.updated),
                )
                .fold(f64::NEG_INFINITY, f64::max);
            let waiting = now - since.max(member.seen);
            if waiting > UNCLAIMED_SECS {
                flagged.insert(task.id);
                issues.push(Issue::new(
                    format!("unclaimed:T{}", task.id),
                    format!(
                        "T{} lleva {} min lista y {} no la toma.",
                        task.id,
                        minutes(waiting),
                        member.name
                    ),
                    &member.name,
                ));
            }
        }
    }
    Ok(issues)
}

fn ask_issues(messages: &[Message], now: f64) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.kind != "ask" || message.recipient == BROADCAST {
            continue;
        }
        let age = now - message.ts;
        let answered = messages[index + 1..].iter().any(|later| {
            later.sender == message.recipient
                && (later.recipient == message.sender || later.recipient == BROADCAST)
        });
        if age > ASK_SECS && !answered {
            issues.push(Issue::new(
                format!("ask:{}", message.id),
                format!(
                    "{} no respondió a {} en {} min.",
                    message.recipient,
                    message.sender,
                    minutes(age)
                ),
                &message.recipient,
            ));
        }
    }
    issues
}

fn incident_issues(
    store: &Db,
    swarm: &str,
    members: &[Member],
    issues: Vec<Issue>,
) -> DbResult<Vec<Issue>> {
    let by_name: HashMap<&str, &Member> = members
        .iter()
        .map(|member| (member.name.as_str(), member))
        .collect();
    let found: Vec<_> = store
        .incidents(swarm, true)?
        .into_iter()
        .filter(|incident| by_name.contains_key(incident.member.as_str()))
        .collect();
    let held: HashSet<&str> = found
        .iter()
        .map(|incident| incident.member.as_str())
        .collect();
    let mut first = Vec::new();
    for incident in &found {
        let member = by_name[incident.member.as_str()];
        let tasks: Vec<String> = store
            .mine(swarm, &incident.member)?
            .iter()
            .map(|task| format!("T{}", task.id))
            .collect();
        first.push(Issue::new(
            format!("incident:{}", incident.id),
            describe(incident, member, &tasks),
            &incident.member,
        ));
    }
    if store
        .get(swarm)?
        .is_some_and(|info| info.status == "paused")
    {
        first.insert(0, Issue::new("paused", "El enjambre está en pausa.", ""));
    }
    first.extend(
        issues
            .into_iter()
            .filter(|issue| !held.contains(issue.member.as_str())),
    );
    Ok(first)
}

/// Is everything fine? Progress, who works and what needs attention.
pub fn health(store: &Db, swarm: &str, now: f64) -> DbResult<Health> {
    if store.get(swarm)?.is_none() {
        return Err(SwarmError::new(format!("No existe el enjambre {}.", repr(swarm))).into());
    }
    let tasks = store.tasks(swarm)?;
    let members = store.members(swarm)?;
    let parts = parts_by_parent(&tasks);
    let leaves: Vec<&Task> = tasks
        .iter()
        .filter(|task| !parts.contains_key(&task.id))
        .collect();
    let done = leaves.iter().filter(|task| task.state == "done").count();
    let held: HashSet<String> = store
        .incidents(swarm, true)?
        .into_iter()
        .map(|incident| incident.member)
        .collect();
    let working: Vec<String> = leaves
        .iter()
        .filter(|task| task.state == "claimed" && !held.contains(&task.owner))
        .map(|task| format!("{} en T{}", task.owner, task.id))
        .collect();
    if leaves.is_empty() {
        return Ok(Health {
            state: "empty".to_string(),
            done: 0,
            total: 0,
            working: Vec::new(),
            found: Vec::new(),
        });
    }
    if done == leaves.len() {
        return Ok(Health {
            state: "done".to_string(),
            done,
            total: leaves.len(),
            working: Vec::new(),
            found: Vec::new(),
        });
    }
    let mut issues = task_issues(&tasks, &members, &parts, now);
    issues.extend(blocked_issues(store, swarm, &tasks, &members)?);
    issues.extend(unclaimed_issues(store, swarm, &tasks, &members, now)?);
    issues.extend(ask_issues(&store.history(swarm, 0, 200)?, now));
    let issues = incident_issues(store, swarm, &members, issues)?;
    let state = if issues.is_empty() { "ok" } else { "attention" };
    Ok(Health {
        state: state.to_string(),
        done,
        total: leaves.len(),
        working,
        found: issues,
    })
}

fn blocked_issues(
    store: &Db,
    swarm: &str,
    tasks: &[Task],
    members: &[Member],
) -> DbResult<Vec<Issue>> {
    let mut issues = Vec::new();
    for member in members {
        if member.state != "blocked" {
            continue;
        }
        let has_failed = tasks.iter().any(|task| {
            task.owner == member.name && ["claimed", "failed"].contains(&task.state.as_str())
        });
        let ready = store.ready(swarm, &member.name)?;
        if !has_failed && ready.is_empty() {
            continue;
        }
        issues.push(Issue::new(
            format!("blocked:{}", member.name),
            format!("{} está bloqueado.", member.name),
            &member.name,
        ));
    }
    Ok(issues)
}

/// Final summary for voice: progress and the result of each stage.
pub fn summary(store: &Db, swarm: &str, now: f64) -> DbResult<String> {
    let state = health(store, swarm, now)?;
    let stages: Vec<String> = store
        .tasks(swarm)?
        .iter()
        .filter(|task| task.parent == 0 && !task.result.is_empty())
        .map(|task| format!("{}: {}", capitalize(role_label(&task.role)), task.result))
        .collect();
    let mut all = vec![state.spoken()];
    for stage in stages {
        all.push(if stage.ends_with('.') {
            stage
        } else {
            format!("{stage}.")
        });
    }
    Ok(all.join(" "))
}

fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => {
            let mut result: String = first.to_uppercase().collect();
            result.push_str(&characters.as_str().to_lowercase());
            result
        }
        None => String::new(),
    }
}

/// `repr(text)` of Python.
fn repr(text: &str) -> String {
    ez_core::pyvalue::repr_str(text)
}

#[cfg(test)]
mod tests;
