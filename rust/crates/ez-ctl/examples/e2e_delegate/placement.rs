//! La ubicación de los hijos en Herdr: el árbol, las pestañas y los paneles.

use serde_json::Value;

use super::checks::Report;
use super::children::{app_json, text, Child, Parent};
use super::setup::Sandbox;

/// Comprueba que los hijos están donde se pidió (`herdr-app ls`).
pub fn check(sandbox: &Sandbox, parent: &Parent, children: &[Child], report: &mut Report) {
    let listing = match app_json(sandbox, &["ls".to_string()]) {
        Ok(value) => value,
        Err(message) => {
            report.fail("la lista de Herdr", message);
            return;
        }
    };
    let Some(space) = find_space(&listing, &parent.space_id, &sandbox.id) else {
        report.fail("el espacio de la prueba", "no aparece en `herdr-app ls`");
        return;
    };
    let tabs = match space.get("tabs").and_then(Value::as_array) {
        Some(tabs) => tabs,
        None => {
            report.fail("el espacio de la prueba", "no tiene pestañas");
            return;
        }
    };
    let parent_tab = tab_of(tabs, &parent.address);
    let Some(parent_tab) = parent_tab else {
        report.fail(
            "la terminal del padre",
            "no aparece en ninguna pestaña del espacio",
        );
        return;
    };
    let sub_agents = sub_agents(parent_tab, &parent.address);
    for child in children {
        if child.terminal.is_empty() {
            continue;
        }
        report.check(
            &format!("{} está bajo su padre", child.name),
            sub_agents.contains(&child.terminal),
            format!("{} no está en sub_agents ({sub_agents:?})", child.terminal),
        );
    }
    check_placed(sandbox, tabs, parent_tab, children, report);
}

/// La ubicación de cada hijo: su pestaña y la del padre.
fn check_placed(
    sandbox: &Sandbox,
    tabs: &[Value],
    parent_tab: &Value,
    children: &[Child],
    report: &mut Report,
) {
    let parent_tab_id = text(parent_tab, "id");
    for child in children {
        if child.terminal.is_empty() {
            continue;
        }
        match child.show.as_str() {
            "tree" => {
                report.check(
                    &format!("{} queda en el árbol", child.name),
                    child.placed == "tree",
                    format!("placed={}", child.placed),
                );
            }
            "tab" => {
                let tab = tab_of(tabs, &child.terminal);
                let own = tab.is_some_and(|tab| text(tab, "id") != parent_tab_id);
                report.check(
                    &format!("{} tiene pestaña propia", child.name),
                    own && child.placed == "tab",
                    format!(
                        "placed={}; {}",
                        child.placed,
                        match tab {
                            Some(tab) => format!("pestaña {}", text(tab, "id")),
                            None => "no aparece en ninguna pestaña".to_string(),
                        }
                    ),
                );
            }
            other => {
                let tab = tab_of(tabs, &child.terminal);
                let same = tab.is_some_and(|tab| text(tab, "id") == parent_tab_id);
                report.check(
                    &format!("{} está en la pestaña del padre", child.name),
                    same && child.placed == other,
                    format!(
                        "placed={}; {}",
                        child.placed,
                        match tab {
                            Some(tab) => format!("pestaña {}", text(tab, "id")),
                            None => "no aparece en ninguna pestaña".to_string(),
                        }
                    ),
                );
            }
        }
    }
    let _ = sandbox;
}

/// El espacio de la prueba (por identificador o por nombre).
fn find_space(listing: &Value, space_id: &str, name: &str) -> Option<Value> {
    let spaces = listing.get("spaces").and_then(Value::as_array)?;
    let mut by_name = None;
    for space in spaces {
        let id = text(space, "id");
        if !space_id.is_empty() && id == space_id {
            return Some(space.clone());
        }
        if by_name.is_none() && text(space, "name") == name {
            by_name = Some(space.clone());
        }
    }
    by_name
}

/// La pestaña donde está una terminal.
fn tab_of<'a>(tabs: &'a [Value], terminal: &str) -> Option<&'a Value> {
    tabs.iter().find(|tab| {
        tab.get("terminals")
            .and_then(Value::as_array)
            .is_some_and(|terminals| {
                terminals
                    .iter()
                    .any(|item| text(item, "terminal") == terminal)
            })
    })
}

/// Los subagentes que Herdr lista bajo una terminal.
fn sub_agents(tab: &Value, terminal: &str) -> Vec<String> {
    tab.get("terminals")
        .and_then(Value::as_array)
        .map(|terminals| {
            terminals
                .iter()
                .find(|item| text(item, "terminal") == terminal)
                .map(|item| {
                    item.get("sub_agents")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .map(|value| value.as_str().unwrap_or_default().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default()
}
