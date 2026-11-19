//! Datos del modo demostración: árbol, traza y tarjeta (brief ECO-6b).
//!
//! No necesita herdr-core ni el motor: la ventana pinta estos datos tal cual.
//! Los eventos llevan su texto ya traducido por `texts::event_text`, además de
//! los campos que la vista necesita para traducirlos por su cuenta.

use serde_json::{json, Value};

use crate::hub;
use crate::runtime::{SpaceTree, TreeNode};
use crate::texts;

fn subagent(id: &str, role: &str, name: &str, status: &str) -> TreeNode {
    TreeNode {
        id: id.to_string(),
        name: name.to_string(),
        role: Some(role.to_string()),
        role_id: Some(id.split('-').next().unwrap_or("").to_string()),
        title: None,
        agent: String::new(),
        model: String::new(),
        status: status.to_string(),
        address: Some(format!("demo-{id}")),
        children: Vec::new(),
    }
}

/// Árbol de ejemplo: un espacio con dos agentes principales; el primero con
/// tres subagentes, uno de ellos con un hijo. Seis nodos más el espacio = 7.
pub fn tree() -> SpaceTree {
    let mut reviewer = subagent("qa-r1", "QA", "QA", "idle");
    reviewer.children.push(subagent(
        "valqa-r1",
        "Validador de QA",
        "Validador de QA",
        "blocked",
    ));
    let mut architect = TreeNode {
        id: "e1a2b3c4".to_string(),
        name: "Arquitecto".to_string(),
        role: None,
        role_id: None,
        title: Some("Transcriptor con whisper.cpp".to_string()),
        agent: "OpenCode".to_string(),
        model: "deepseek-v4.1-flash".to_string(),
        status: "working".to_string(),
        address: Some("demo-main-1".to_string()),
        children: vec![
            subagent("dev-r1-1", "Desarrollador", "Desarrollador", "working"),
            subagent("dev-r1-2", "Desarrollador", "Desarrollador", "done"),
            reviewer,
        ],
    };
    // DECISION: el subagente con hijo lleva agente y modelo para que la pestaña
    // abierta por defecto tenga con qué pintar su cabecera.
    architect.children[0].agent = "OpenCode".to_string();
    architect.children[0].model = "deepseek-v4.1-flash".to_string();
    SpaceTree {
        nodes: vec![
            architect,
            TreeNode {
                id: "e5f6a7b8".to_string(),
                name: "Revisor".to_string(),
                role: None,
                role_id: None,
                title: Some("Revisión de lanzamiento".to_string()),
                agent: "Codex".to_string(),
                model: "gpt-6-sol".to_string(),
                status: "idle".to_string(),
                address: Some("demo-main-2".to_string()),
                children: Vec::new(),
            },
        ],
    }
}

fn event(time: &str, event_type: &str, data: Value) -> Value {
    let mut value = data;
    value["type"] = json!(event_type);
    value["time"] = json!(time);
    value["texto"] = json!(texts::event_text(event_type, &value));
    value
}

/// Traza de ejemplo: seis eventos y una tarjeta de equipo, con la forma que
/// devuelve el motor (`{eventos, tarjetas, tarea}`) para usar un solo pintor.
pub fn trace() -> Value {
    let card = hub::team_card(
        "Transcriptor con whisper.cpp",
        &[
            hub::SeatLine {
                role: "Planificador",
                agent: "OpenCode",
                model: "opencode-go/deepseek-v4.1-flash",
                effort: None,
                count: 1,
            },
            hub::SeatLine {
                role: "Desarrollador",
                agent: "OpenCode",
                model: "opencode-go/deepseek-v4.1-flash",
                effort: Some("max"),
                count: 1,
            },
            hub::SeatLine {
                role: "QA",
                agent: "Codex",
                model: "gpt-6-sol",
                effort: None,
                count: 1,
            },
        ],
    );
    json!({
        "eventos": [
            event("09:41:03", "task_approved", json!({})),
            event("09:41:20", "team_approved", json!({})),
            event("09:42:05", "round_started", json!({ "round": 1 })),
            event("09:42:11", "fish_launched", json!({
                "role": "Desarrollador", "fish": "dev-r1-1",
                "agente": "OpenCode", "modelo": "deepseek-v4.1-flash"
            })),
            event("09:47:58", "fish_reported", json!({
                "fish": "qa-r1", "verdict": "pass", "blocking": 0, "major": 0
            })),
            event("09:48:02", "verification", json!({ "aprobados": 3, "total": 3 })),
        ],
        "tarjetas": [json!({
            "id": card.id, "kind": "team", "title": card.title, "body": card.body
        })],
        "tarea": Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(nodes: &[TreeNode]) -> usize {
        nodes.iter().map(|node| 1 + count(&node.children)).sum()
    }

    #[test]
    fn the_demo_tree_has_seven_nodes_including_nesting() {
        let tree = tree();
        // Seis nodos del árbol más el espacio que los contiene.
        assert_eq!(1 + count(&tree.nodes), 7);
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].children.len(), 3);
        assert_eq!(tree.nodes[0].children[2].children.len(), 1);
        assert!(tree.nodes[1].children.is_empty());
    }

    #[test]
    fn the_demo_trace_has_six_events_and_one_team_card() {
        let trace = trace();
        assert_eq!(trace["eventos"].as_array().map(Vec::len), Some(6));
        assert_eq!(trace["tarjetas"].as_array().map(Vec::len), Some(1));
        assert_eq!(trace["tarjetas"][0]["kind"], "team");
        assert_eq!(
            trace["eventos"][3]["texto"],
            "Desarrollador dev-r1-1 en marcha (OpenCode · deepseek-v4.1-flash)"
        );
    }
}
