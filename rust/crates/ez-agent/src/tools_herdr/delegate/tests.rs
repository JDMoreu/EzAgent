use std::sync::{Arc, Mutex};

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

#[test]
fn la_cadena_pendiente_copia_los_campos_del_python() {
    let run = RunArgs {
        agent: "claude".to_string(),
        task: "haz X".to_string(),
        steps: Vec::new(),
        background: true,
        gui: true,
        close: false,
        notify: true,
        deliver: true,
        cwd: Some("/tmp".to_string()),
        name: None,
        space: Some("EzAgent".to_string()),
        model: None,
        effort: Some("high".to_string()),
        input_from: Some("t2".to_string()),
    };
    let spec = pending_chain(&run, "t2");
    assert_eq!(spec["source"], json!("t2"));
    assert_eq!(spec["agent"], json!("claude"));
    assert_eq!(spec["task"], json!("haz X"));
    assert_eq!(spec["cwd"], json!("/tmp"));
    assert_eq!(spec["name"], Value::Null);
    assert_eq!(spec["background"], json!(true));
    assert_eq!(spec["gui"], json!(true));
    assert_eq!(spec["model"], json!(""));
    assert_eq!(spec["effort"], json!("high"));
    assert_eq!(spec["notify_on_done"], json!(true));
    assert_eq!(spec["deliver_answer"], json!(true));
    assert!(spec.get("then").is_none());
}

#[test]
fn la_direccion_lanzada_prefiere_address_y_recorta() {
    let result = args(json!({"address": " t1 ", "terminal": "t2"}));
    assert_eq!(launched_address(&result), "t1");
    let result = args(json!({"address": "", "terminal": "t2"}));
    assert_eq!(launched_address(&result), "t2");
    assert_eq!(launched_address(&Map::new()), "");
}

#[derive(Default)]
struct Recorded {
    calls: Mutex<Vec<String>>,
    specs: Mutex<Vec<Value>>,
}

impl Recorded {
    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn specs(&self) -> Vec<Value> {
        self.specs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

fn recorded_callbacks(recorded: &Arc<Recorded>) -> HerdrCallbacks {
    let watch = Arc::clone(recorded);
    let remember = Arc::clone(recorded);
    let deliver = Arc::clone(recorded);
    let failed = Arc::clone(recorded);
    let chain = Arc::clone(recorded);
    HerdrCallbacks {
        watch: Arc::new(move |address| {
            watch
                .calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("watch {address}"));
        }),
        remember_task: Arc::new(move |address, agent, task| {
            remember
                .calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("remember {address} {agent} {task}"));
        }),
        deliver_answer: Arc::new(move |address| {
            deliver
                .calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("deliver {address}"));
        }),
        launch_failed: Arc::new(move |address, agent, error| {
            failed
                .calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("failed {address} {agent} {error}"));
        }),
        enqueue_chain: Arc::new(move |spec| {
            chain
                .specs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(spec);
        }),
        ..HerdrCallbacks::default()
    }
}

fn context(recorded: &Arc<Recorded>, steps: Vec<crate::orchestration::Step>) -> LaunchContext {
    LaunchContext {
        callbacks: recorded_callbacks(recorded),
        agent: "claude".to_string(),
        task: "haz X".to_string(),
        steps,
        cwd: None,
        name: None,
        space: None,
        notify: true,
        deliver: true,
        chain_id: String::new(),
    }
}

#[test]
fn after_ready_recuerda_la_tarea_y_vigila() {
    let recorded = Arc::new(Recorded::default());
    after_ready(
        &context(&recorded, Vec::new()),
        &args(json!({"ok": true, "address": " t1 "})),
    );
    assert_eq!(
        recorded.calls(),
        vec![
            "remember t1 claude haz X".to_string(),
            "deliver t1".to_string(),
            "watch t1".to_string(),
        ]
    );
    assert!(recorded.specs().is_empty());
}

#[test]
fn after_ready_sin_aviso_no_hace_nada() {
    let recorded = Arc::new(Recorded::default());
    let mut context = context(&recorded, Vec::new());
    context.notify = false;
    context.deliver = false;
    after_ready(&context, &args(json!({"ok": true, "address": "t1"})));
    assert!(recorded.calls().is_empty());
}

#[test]
fn after_ready_encadena_los_pasos() {
    let recorded = Arc::new(Recorded::default());
    let steps = parse_steps(&json!([{"agent": "codex", "task": "sigue"}])).expect("pasos");
    after_ready(
        &context(&recorded, steps),
        &args(json!({"ok": true, "address": "t1"})),
    );
    let specs = recorded.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0]["source"], json!("t1"));
    assert_eq!(specs[0]["agent"], json!("codex"));
    assert_eq!(specs[0]["task"], json!("sigue"));
    assert_eq!(specs[0]["notify_final"], json!(true));
    assert_eq!(specs[0]["flow"], json!(["claude"]));
    assert!(recorded.calls().is_empty());
}

#[test]
fn after_ready_informa_del_fallo_de_lanzamiento() {
    let recorded = Arc::new(Recorded::default());
    after_ready(
        &context(&recorded, Vec::new()),
        &args(json!({"ok": false, "address": "t1", "error": "boom"})),
    );
    assert_eq!(recorded.calls(), vec!["failed t1 claude boom".to_string()]);
}
