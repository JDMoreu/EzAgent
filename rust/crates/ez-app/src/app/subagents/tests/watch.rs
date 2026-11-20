//! El vigilante de la app con un `hc` de guion fijo.

use std::collections::HashMap;
use std::sync::Arc;

use ez_herdr::subagents::{epoch_ms, HcOutput, WatchRuntime};
use parking_lot::Mutex;
use serde_json::json;

use crate::app::core::SubagentWatch;

use super::*;

/// El `hc` del vigilante con guion fijo: terminales, pantallas y llamadas.
struct ScriptedWatch {
    terminals: Mutex<String>,
    screens: Mutex<HashMap<String, String>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl ScriptedWatch {
    fn new(terminals: Value, screens: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            terminals: Mutex::new(terminals.to_string()),
            screens: Mutex::new(
                screens
                    .iter()
                    .map(|(address, text)| ((*address).to_string(), (*text).to_string()))
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().clone()
    }
}

impl WatchRuntime for ScriptedWatch {
    fn hc(&self, args: &[String]) -> Result<HcOutput, String> {
        self.calls.lock().push(args.to_vec());
        match args.first().map(String::as_str) {
            Some("ls") => Ok(HcOutput {
                code: 0,
                stdout: self.terminals.lock().clone(),
                stderr: String::new(),
            }),
            Some("read") => {
                let address = args.get(1).cloned().unwrap_or_default();
                let screen = self
                    .screens
                    .lock()
                    .get(&address)
                    .cloned()
                    .unwrap_or_default();
                Ok(HcOutput {
                    code: 0,
                    stdout: format!("{screen}\n"),
                    stderr: String::new(),
                })
            }
            // El modo sin-terminal no debe pedir la vida del padre.
            Some("get") => Ok(HcOutput {
                code: 0,
                stdout: "{}".to_string(),
                stderr: String::new(),
            }),
            other => Err(format!("sin guion para {other:?}")),
        }
    }

    fn now_ms(&self) -> u64 {
        epoch_ms()
    }

    fn sleep_ms(&self, _ms: u64) {}
}

#[test]
fn el_vigilante_de_la_app_bloquea_y_avisa_sin_escribir_el_registro() {
    let runtime = ScriptedWatch::new(
        json!({"terminals": [
            {"address": "t1", "state": "live", "status": {"state": "idle"}},
            {"address": "t2", "state": "finished", "exit_code": 1}
        ]}),
        &[("t1", "Free limit reached. Resets at 7:00 p.m.")],
    );
    let ctx = ctx_with("watch", |parts| {
        let novelty = parts
            .subagents_dir
            .as_ref()
            .expect("carpeta de subagentes")
            .with_file_name("novedades.toml");
        let concrete: Arc<ScriptedWatch> = Arc::clone(&runtime);
        let watch_runtime: Arc<dyn WatchRuntime> = concrete;
        parts.subagent_watch = Some(SubagentWatch {
            runtime: watch_runtime,
            novelty_file: Some(novelty),
            interval_ms: 20,
            grace_ms: 0,
        });
    });
    let app = &ctx.app;
    let mut blocked = record("ir-1", TaskState::InProgress, VOICE_PARENT);
    blocked.terminal = "t1".to_string();
    ctx.registry.create(&blocked).expect("registro ir-1");
    let mut dead = record("ir-2", TaskState::InProgress, VOICE_PARENT);
    dead.terminal = "t2".to_string();
    ctx.registry.create(&dead).expect("registro ir-2");

    assert!(
        wait_until(|| has_note(app, "está bloqueado")),
        "notas: {:?}",
        notes(app)
    );
    assert!(
        wait_until(|| has_note(app, "falló")),
        "notas: {:?}",
        notes(app)
    );
    let saved = ctx
        .registry
        .get(VOICE_PARENT, "ir-1")
        .expect("leer")
        .expect("ir-1");
    assert_eq!(saved.state, TaskState::Blocked);
    assert_eq!(saved.history.last().expect("cambio").by, "vigilante");
    assert_eq!(saved.notified, Some(TaskState::Blocked));
    let failed = ctx
        .registry
        .get(VOICE_PARENT, "ir-2")
        .expect("leer")
        .expect("ir-2");
    assert_eq!(failed.state, TaskState::Failed);
    assert_eq!(failed.history.last().expect("cambio").by, "vigilante");
    assert_eq!(
        app.inner.agents.retry_offer.lock().clone(),
        Some("ir-1".to_string())
    );
    let calls = runtime.calls();
    assert!(
        calls
            .iter()
            .all(|call| call.first().map(String::as_str) != Some("get")),
        "sin `hc get` del padre: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| call.first().map(String::as_str) != Some("send")),
        "los avisos van al sink, no a `hc send`"
    );

    assert!(app.answer_retry_offer("dale"));
    assert!(wait_until(|| has_note(app, "sigue como ir-1-r1")));
    assert_eq!(*ctx.retries.lock(), vec!["ir-1".to_string()]);
    app.shutdown();
}
