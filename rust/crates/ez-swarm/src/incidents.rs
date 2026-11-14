//! Port of `src/ezagent/swarm/incidents.py` (F3-12).
//!
//! What happened to an agent and what the user is offered. The agent screen is
//! untrusted data: only a pattern is looked up to choose a label. The worst a
//! malicious text can do is produce a notice; never an action.

use std::collections::HashSet;

use chrono::{Local, TimeZone, Timelike};

use crate::models::{Incident, Member};
use crate::protocol::role_label;

mod patterns;

pub use patterns::{classify, parse_reset};

/// Lines of the screen that are inspected.
pub const TAIL_LINES: usize = 18;
/// Kinds that need a decision from the user (`ASK_KINDS`).
pub const ASK_KINDS: [&str; 5] = ["quota", "auth", "closed", "exited", "error"];
/// Kind labels used in sentences (`KIND_LABELS`).
pub const KIND_LABELS: [(&str, &str); 6] = [
    ("quota", "se quedó sin uso"),
    ("context", "se quedó sin contexto"),
    ("auth", "perdió la sesión con su proveedor"),
    ("closed", "tiene su ventana cerrada"),
    ("exited", "se cerró solo dos veces"),
    ("error", "tuvo un error"),
];

fn kind_label(kind: &str) -> &str {
    KIND_LABELS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(kind, |(_, label)| *label)
}

/// What was recognized on an agent screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    /// Recognized kind.
    pub kind: String,
    /// Last matching line, clipped to 200 characters.
    pub detail: String,
    /// Reset time (epoch); 0 when unknown.
    pub resets: f64,
}

/// «las 7:10 p. m.» or «mañana a las 7:10 p. m.» (relative to `reference`);
/// empty when unknown.
pub fn when(resets: f64, reference: f64) -> String {
    if resets <= 0.0 {
        return String::new();
    }
    let Some(moment) = Local.timestamp_opt(resets as i64, 0).earliest() else {
        return String::new();
    };
    let base = if reference > 0.0 {
        Local
            .timestamp_opt(reference as i64, 0)
            .earliest()
            .map_or_else(|| Local::now().naive_local(), |value| value.naive_local())
    } else {
        Local::now().naive_local()
    };
    let hour = moment.hour() % 12;
    let hour = if hour == 0 { 12 } else { hour };
    let suffix = if moment.hour() < 12 { "a. m." } else { "p. m." };
    let clock = format!("las {hour}:{:02} {suffix}", moment.minute());
    if moment.date_naive() > base.date() {
        format!("mañana a {clock}")
    } else {
        clock
    }
}

/// Another member (with another agent) whose agent and model can replace the
/// failed one.
pub fn suggest<'a>(
    members: &'a [Member],
    failing: &Member,
    busy: &HashSet<String>,
) -> Option<&'a Member> {
    members.iter().find(|member| {
        member.name != failing.name
            && !busy.contains(&member.name)
            && member.state != "gone"
            && !member.agent.is_empty()
            && member.agent != failing.agent
    })
}

/// What happened, in one sentence.
pub fn describe(incident: &Incident, member: &Member, tasks: &[String]) -> String {
    let label = kind_label(&incident.kind);
    let until = when(incident.resets, incident.created);
    let mut text = format!(
        "{} ({}, {}) {label}",
        member.name,
        member.agent,
        role_label(&member.role)
    );
    if !until.is_empty() && incident.kind == "quota" {
        text.push_str(&format!(" hasta {until}"));
    }
    if !tasks.is_empty() {
        text.push_str(&format!(". Su trabajo pendiente: {}", tasks.join(", ")));
    }
    text.push('.');
    text
}

/// Question to the user with the options; built by code, never by the LLM.
pub fn question(
    incident: &Incident,
    member: &Member,
    tasks: &[String],
    alt: Option<&Member>,
) -> String {
    let head = format!("Enjambre: {}", describe(incident, member, tasks));
    let swap = match alt {
        Some(alt) if !alt.model.is_empty() => {
            format!("lo reemplazo con {} {}", alt.agent, alt.model)
        }
        Some(alt) => format!("lo reemplazo con {}", alt.agent),
        None => "lo reemplazo con otro agente".to_string(),
    };
    if incident.kind == "quota" {
        let wait = if incident.resets != 0.0 {
            "espero a que se restablezca"
        } else {
            "espero a que vuelva"
        };
        return format!("{head} ¿{}, {swap} o pauso el enjambre?", capitalize(wait));
    }
    if incident.kind == "closed" {
        return format!("{head} ¿Lo reabro, {swap} o pauso el enjambre?");
    }
    if incident.kind == "auth" {
        return format!("{head} Inicia sesión en su ventana y lo retomo, o dime si {swap}.");
    }
    format!("{head} ¿Lo reabro, {swap} o pauso el enjambre?")
}

/// Short notice for the board.
pub fn board_note(incident: &Incident, member: &Member, tasks: &[String]) -> String {
    let until = when(incident.resets, incident.created);
    let wait = if !until.is_empty() && incident.kind == "quota" {
        format!(" hasta {until}")
    } else {
        String::new()
    };
    let held = if tasks.is_empty() {
        String::new()
    } else {
        format!("; {} en espera", tasks.join(", "))
    };
    format!(
        "{} ({}) {}{wait}{held}.",
        member.name,
        member.agent,
        kind_label(&incident.kind)
    )
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

#[cfg(test)]
mod tests;
