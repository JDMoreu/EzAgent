use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use serde_json::{json, Map, Value};

use super::*;
use crate::tools_herdr::args::shared_state;

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
}

fn herdr() -> Arc<Herdr> {
    Arc::new(Herdr::new(VoiceConfig::new()))
}

fn call(value: Value) -> ToolResult {
    handle_delegate(
        &herdr(),
        &HerdrCallbacks::default(),
        &shared_state(),
        &args(value),
    )
}

#[test]
fn valida_accion_y_parametros_obligatorios() {
    assert_eq!(
        call(json!({})).error.as_deref(),
        Some(
            "Falta la acción de «delegate_to_agent». Acciones válidas: list, get, read, \
             send, run, open_terminal, focus, kill, watch, agents, configure, keys, guide."
        )
    );
    assert_eq!(
        call(json!({"action": "zzz"})).error.as_deref(),
        Some(
            "Acción desconocida para «delegate_to_agent»: 'zzz'. Acciones válidas: list, get, \
             read, send, run, open_terminal, focus, kill, watch, agents, configure, keys, guide."
        )
    );
    assert_eq!(
        call(json!({"action": "read"})).error.as_deref(),
        Some("Para la acción «read» falta el parámetro «address».")
    );
    assert_eq!(
        call(json!({"action": "open_terminal"})).error.as_deref(),
        Some("Para la acción «open_terminal» falta el parámetro «name».")
    );
}

#[test]
fn valida_los_argumentos_de_run() {
    assert_eq!(
        call(json!({"action": "run"})).error.as_deref(),
        Some("Para la acción «run» falta el parámetro «agent».")
    );
    assert_eq!(
        call(json!({"action": "run", "agent": "nope"}))
            .error
            .as_deref(),
        // DECISION: el resolvedor de nombres sustituye el «Agente desconocido»
        // del Python por la lista de agentes, para que el asistente pregunte.
        Some(
            "No encontré el agente «nope». Agentes disponibles: «Claude Code», «Codex», \
             «MiniMax (mcode)», «MiniMax Code», «OpenCode», «Pi», «Qwen Code»."
        )
    );
    assert_eq!(
        call(json!({"action": "run", "agent": "claude", "then": "mal"}))
            .error
            .as_deref(),
        Some("El parámetro «then» debe ser una lista de pasos.")
    );
    assert_eq!(
        call(json!({"action": "run", "agent": "claude", "notify_on_done": "sí"}))
            .error
            .as_deref(),
        Some("El parámetro «notify_on_done» de la acción «run» debe ser verdadero o falso.")
    );
    assert_eq!(
        call(json!({"action": "run", "agent": "claude", "model": 4}))
            .error
            .as_deref(),
        Some("El parámetro «model» de la acción «run» debe ser texto.")
    );
}

#[test]
fn valida_keys_y_configure() {
    let expected = "Para la acción «keys» indica «keys» (lista de teclas).";
    for value in [
        json!({"action": "keys", "address": "t1"}),
        json!({"action": "keys", "address": "t1", "keys": []}),
        json!({"action": "keys", "address": "t1", "keys": "enter"}),
    ] {
        assert_eq!(call(value).error.as_deref(), Some(expected));
    }
    assert_eq!(
        call(json!({"action": "configure", "address": "t1"}))
            .error
            .as_deref(),
        Some("Para la acción «configure» indica «model» o «effort».")
    );
}

#[test]
fn el_catalogo_y_los_manuales_no_hablan_con_herdr() {
    let catalog = call(json!({"action": "agents"}));
    assert!(catalog.ok);
    let Value::Array(agents) = &catalog.data else {
        panic!("se esperaba una lista");
    };
    assert_eq!(agents.len(), 7);
    assert_eq!(agents[0]["agent"], json!("claude"));
    assert_eq!(agents[0]["label"], json!("Claude Code"));

    let guide = call(json!({"action": "guide", "agent": "Claude Code"}));
    assert!(guide.ok);
    assert_eq!(guide.data["agent"], json!("claude"));
    assert!(!guide.data["guide"].as_str().unwrap_or("").is_empty());
    assert_eq!(
        call(json!({"action": "guide", "agent": "nope"}))
            .error
            .as_deref(),
        Some(
            "No encontré el agente «nope». Agentes disponibles: «Claude Code», «Codex», \
             «MiniMax (mcode)», «MiniMax Code», «OpenCode», «Pi», «Qwen Code»."
        )
    );
}
