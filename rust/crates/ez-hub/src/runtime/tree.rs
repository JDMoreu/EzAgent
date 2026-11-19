//! Árbol de agentes de la vista, armado desde las etiquetas de herdr-core (§15.6).

use std::collections::{BTreeMap, HashMap, HashSet};

use ez_arbiter::MainAgent;
use ez_term::Terminal;

use crate::runtime::spaces::MainChoice;
use crate::runtime::types::{SpaceTree, TreeNode};

/// Construye el árbol de un espacio. `choice` y `title` resuelven los datos que
/// no están en las etiquetas de hc: la elección del agente y el título de su tarea.
pub(crate) fn build(
    space: &str,
    agents: &[MainAgent],
    terminals: &[Terminal],
    choice: impl Fn(&str) -> Option<MainChoice>,
    title: impl Fn(&str) -> Option<String>,
    role_names: &BTreeMap<String, String>,
) -> SpaceTree {
    let names = role_names;
    let mut nodes = Vec::new();
    for agent in agents.iter().filter(|agent| agent.key.space == space) {
        let id = agent.key.elephant.as_str();
        let selected = choice(id);
        let live = terminal_for(terminals, |labels| {
            labels.get("ez_elephant").map(String::as_str) == Some(id)
                && labels.get("ez_role").map(String::as_str) == Some("main")
        });
        let children = fish_children(id, terminals, names);
        nodes.push(TreeNode {
            id: id.to_string(),
            name: agent.name.clone(),
            role: None,
            role_id: None,
            title: title(id),
            agent: selected
                .as_ref()
                .map(|choice| choice.agent.clone())
                .unwrap_or_default(),
            model: selected
                .as_ref()
                .map(|choice| choice.model.clone())
                .unwrap_or_default(),
            status: status_of(live),
            address: live.map(|terminal| terminal.address.clone()),
            children,
        });
    }
    attach_unlabeled(&mut nodes, terminals);
    SpaceTree { nodes }
}

fn fish_children(
    owner: &str,
    terminals: &[Terminal],
    names: &BTreeMap<String, String>,
) -> Vec<TreeNode> {
    let fish: Vec<&Terminal> = terminals
        .iter()
        .filter(|terminal| {
            terminal.labels.get("ez").map(String::as_str) == Some("1")
                && terminal.labels.get("ez_elephant").map(String::as_str) == Some(owner)
                && terminal.labels.contains_key("ez_fish")
        })
        .collect();
    let mut by_parent: HashMap<&str, Vec<&Terminal>> = HashMap::new();
    for terminal in fish {
        let parent = terminal
            .labels
            .get("ez_parent")
            .map(String::as_str)
            .unwrap_or(owner);
        by_parent.entry(parent).or_default().push(terminal);
    }
    let mut seen = HashSet::new();
    nest(owner, &by_parent, names, &mut seen)
}

fn nest(
    parent: &str,
    by_parent: &HashMap<&str, Vec<&Terminal>>,
    names: &BTreeMap<String, String>,
    seen: &mut HashSet<String>,
) -> Vec<TreeNode> {
    let Some(children) = by_parent.get(parent) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for terminal in children {
        if !seen.insert(terminal.address.clone()) {
            continue;
        }
        let id = terminal
            .labels
            .get("ez_fish")
            .cloned()
            .unwrap_or_else(|| terminal.address.clone());
        let role_id = terminal.labels.get("ez_role").cloned();
        let role = role_id
            .as_ref()
            .and_then(|role| names.get(role).cloned())
            .or_else(|| role_id.clone());
        let nested = nest(&terminal.address, by_parent, names, seen);
        result.push(TreeNode {
            id,
            name: terminal
                .labels
                .get("ez_fish")
                .cloned()
                .unwrap_or_else(|| terminal.address.clone()),
            role,
            role_id,
            title: None,
            agent: label_or(terminal, "ez_agent"),
            model: label_or(terminal, "ez_model"),
            status: terminal
                .status
                .as_ref()
                .map(|status| status.state.clone())
                .unwrap_or_else(|| terminal.state.clone()),
            address: Some(terminal.address.clone()),
            children: nested,
        });
    }
    result
}

/// Terminales abiertas a mano cuyo `parent` apunta a un nodo del árbol cuelgan
/// como hijos «sin papel» (§15.1).
fn attach_unlabeled(nodes: &mut [TreeNode], terminals: &[Terminal]) {
    let loose: Vec<&Terminal> = terminals
        .iter()
        .filter(|terminal| !terminal.labels.contains_key("ez"))
        .collect();
    for node in nodes.iter_mut() {
        attach_to(node, &loose);
    }
}

fn attach_to(node: &mut TreeNode, loose: &[&Terminal]) {
    let mut children = Vec::new();
    for terminal in loose.iter().filter(|terminal| {
        terminal.parent.as_deref() == Some(node.address.as_deref().unwrap_or(""))
            && node.address.is_some()
    }) {
        children.push(TreeNode {
            id: terminal.address.clone(),
            name: terminal
                .name
                .clone()
                .unwrap_or_else(|| terminal.address.clone()),
            role: Some("sin papel".to_string()),
            role_id: None,
            title: None,
            agent: String::new(),
            model: String::new(),
            status: terminal
                .status
                .as_ref()
                .map(|status| status.state.clone())
                .unwrap_or_else(|| terminal.state.clone()),
            address: Some(terminal.address.clone()),
            children: Vec::new(),
        });
    }
    node.children.extend(children);
    for child in node.children.iter_mut() {
        attach_to(child, loose);
    }
}

fn terminal_for<'a>(
    terminals: &'a [Terminal],
    predicate: impl Fn(&BTreeMap<String, String>) -> bool,
) -> Option<&'a Terminal> {
    terminals
        .iter()
        .find(|terminal| predicate(&terminal.labels))
}

fn status_of(terminal: Option<&Terminal>) -> String {
    terminal.map_or_else(|| "sin terminal".to_string(), |item| item.state.clone())
}

fn label_or(terminal: &Terminal, name: &str) -> String {
    terminal.labels.get(name).cloned().unwrap_or_default()
}

/// Nombres de los papeles incluidos y de la biblioteca, por id.
pub(crate) fn role_names() -> BTreeMap<String, String> {
    let roles = ez_team::load_roles(None, &ez_team::library_dir())
        .unwrap_or_else(|_| ez_team::builtin_roles());
    roles
        .into_iter()
        .map(|def| (def.id.as_str().to_string(), def.name))
        .collect()
}
