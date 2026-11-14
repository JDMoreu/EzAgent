use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use serde_json::{json, Map, Value};

use super::*;
use crate::registry::{ToolReturn, ToolSpec};

fn build() -> Vec<ToolSpec> {
    build_herdr_tools(
        Arc::new(Herdr::new(VoiceConfig::new())),
        HerdrCallbacks::default(),
    )
}

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
}

#[test]
fn construye_las_dos_herramientas() {
    let tools = build();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "delegate_to_agent");
    assert_eq!(tools[0].level, 2);
    assert!(tools[0].untrusted_output);
    assert_eq!(tools[0].timeout_s, 120.0);
    assert!(tools[0]
        .description
        .contains("Delega en un agente de Herdr"));
    assert_eq!(tools[1].name, "voice_control");
    assert_eq!(tools[1].level, 1);
    assert!(!tools[1].untrusted_output);
    assert_eq!(tools[1].timeout_s, 10.0);
    assert!(tools[1]
        .description
        .contains("Controla la escucha y los avisos de voz"));
}

#[test]
fn los_esquemas_declaran_las_acciones() {
    let tools = build();
    let delegate = &tools[0].parameters;
    assert_eq!(delegate["type"], json!("object"));
    assert_eq!(delegate["required"], json!(["action"]));
    let actions = delegate["properties"]["action"]["enum"]
        .as_array()
        .expect("enum");
    assert_eq!(actions.len(), 13);
    assert_eq!(actions[0], json!("list"));
    let then = &delegate["properties"]["then"];
    assert_eq!(then["maxItems"], json!(5));
    assert_eq!(then["items"]["required"], json!(["agent"]));
    assert_eq!(delegate["properties"]["keys"]["maxItems"], json!(12));
    assert_eq!(delegate["properties"]["lines"]["minimum"], json!(10));
    assert_eq!(delegate["properties"]["lines"]["maximum"], json!(120));

    let voice = &tools[1].parameters;
    let actions = voice["properties"]["action"]["enum"]
        .as_array()
        .expect("enum");
    assert_eq!(actions.len(), 5);
    assert_eq!(voice["properties"]["minutes"]["minimum"], json!(1));
    assert_eq!(voice["properties"]["minutes"]["maximum"], json!(480));
}

#[test]
fn describe_usa_el_nivel_real_de_cada_accion() {
    let tools = build();
    let list = tools[0].describe(&args(json!({"action": "list"})));
    assert_eq!(list.level, 0);
    assert!(!list.always_confirm);
    assert_eq!(list.summary, "Listar los agentes vivos de Herdr.");
    let kill = tools[0].describe(&args(json!({"action": "kill"})));
    assert_eq!(kill.level, 2);
    assert!(kill.always_confirm);
    let voice = tools[1].describe(&args(json!({"action": "repeat_last"})));
    assert_eq!(voice.level, 1);
    assert_eq!(voice.summary, "Repetir el último resumen.");
}

#[test]
fn el_manejador_devuelve_el_catalogo() {
    let tools = build();
    let returned = (tools[0].handler)(&args(json!({"action": "agents"}))).expect("manejador");
    let ToolReturn::Result(result) = returned else {
        panic!("se esperaba un ToolResult");
    };
    assert!(result.ok);
    let Value::Array(agents) = &result.data else {
        panic!("se esperaba una lista");
    };
    assert_eq!(agents.len(), 7);
}
