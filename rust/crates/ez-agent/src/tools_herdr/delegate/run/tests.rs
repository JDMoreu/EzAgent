//! Pruebas del lanzamiento y de los encadenados de `run` (F4-12).

use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use super::*;
use crate::orchestration::Step;

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
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

fn context(recorded: &Arc<Recorded>, steps: Vec<Step>) -> LaunchContext {
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
