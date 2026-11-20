//! Pruebas de la app como padre de sus subagentes (NV-3).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use ez_herdr::subagents::{epoch_ms, Change, Record, Registry, TaskState, Validation};
use parking_lot::Mutex;
use serde_json::{json, Map, Value};

use crate::app::core::{RetryRunner, VoiceApp};
use crate::app::states::tests::support::Fixture;

use super::phrases::{news_text, overview_phrase, parse_status_query, status_phrase, StatusQuery};
use super::port::{register_launch, Launch};
use super::VOICE_PARENT;

/// Carpeta temporal propia, borrada al terminar.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "ezagent-nv3-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("carpeta temporal");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// App real con dobles, registro temporal y reintento falso.
struct Ctx {
    app: VoiceApp,
    speaker: Arc<crate::app::states::tests::support::RecordingSpeaker>,
    registry: Arc<Registry>,
    retries: Arc<Mutex<Vec<String>>>,
    _temp: TempDir,
}

fn ctx(tag: &str) -> Ctx {
    let temp = TempDir::new(tag);
    let dir = temp.0.join("subagentes");
    let retries = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&retries);
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    let fixture = Fixture::with_setup(cfg, move |parts| {
        parts.subagents_dir = Some(dir.clone());
        let runner: RetryRunner = Arc::new(move |name: &str| {
            calls.lock().push(name.to_string());
            Ok(format!("{name}-r1"))
        });
        parts.retry_runner = Some(runner);
    });
    let registry = Arc::new(Registry::at(temp.0.join("subagentes")));
    Ctx {
        app: fixture.app.clone(),
        speaker: Arc::clone(&fixture.speaker),
        registry,
        retries,
        _temp: temp,
    }
}

fn record(name: &str, state: TaskState, by: &str) -> Record {
    let now = epoch_ms();
    Record {
        name: name.to_string(),
        terminal: format!("t-{name}"),
        parent: VOICE_PARENT.to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("glm-5.3".to_string()),
        effort: None,
        role: VOICE_PARENT.to_string(),
        cwd: "C:\\work".to_string(),
        brief_head: "Revisa la rama work/NV-3.".to_string(),
        brief_path: None,
        retry_of: None,
        state,
        since_ms: now,
        history: vec![Change {
            state,
            at_ms: now,
            by: by.to_string(),
            detail: format!("detalle de {}", state.as_str()),
        }],
        summary: (state == TaskState::Done).then(|| "Todo en verde.".to_string()),
        report: None,
        validation: (state == TaskState::Validated).then(|| Validation {
            by: "qa".to_string(),
            verdict: true,
            summary: "Sin hallazgos.".to_string(),
            file: None,
            at_ms: now,
        }),
        notified: None,
    }
}

fn notes(app: &VoiceApp) -> Vec<String> {
    app.inner
        .chat
        .chat
        .snapshot()
        .1
        .into_iter()
        .map(|message| message.text)
        .collect()
}

fn has_note(app: &VoiceApp, needle: &str) -> bool {
    notes(app).iter().any(|text| text.contains(needle))
}

fn wait_until(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    check()
}

#[test]
fn parse_status_query_accepts_the_direct_forms() {
    assert_eq!(
        parse_status_query("¿Cómo va ir-2?"),
        Some(StatusQuery::One("ir-2".to_string()))
    );
    assert_eq!(
        parse_status_query("¿Cómo está el subagente dev1-opencode?"),
        Some(StatusQuery::One("dev1-opencode".to_string()))
    );
    assert_eq!(
        parse_status_query("¿cómo está la tarea ir-2?"),
        Some(StatusQuery::One("ir-2".to_string()))
    );
    assert_eq!(
        parse_status_query("¿Cómo van los agentes?"),
        Some(StatusQuery::All)
    );
    assert_eq!(parse_status_query("ok"), None);
    assert_eq!(parse_status_query("¿cómo va?"), None);
    assert_eq!(parse_status_query("¿cómo va todo?"), None);
    assert_eq!(parse_status_query("lista los agentes"), None);
}

#[test]
fn news_and_status_texts_cover_every_state() {
    let blocked = record("ir-1", TaskState::Blocked, "vigilante");
    let text = news_text(&blocked).expect("aviso de bloqueo");
    assert!(text.contains("ir-1 (OpenCode · glm-5.3) está bloqueado"));
    assert!(text.contains("detalle de bloqueado"));
    assert!(text.contains("¿Lo reintento con la recomendación?"));

    let done = record("ir-2", TaskState::Done, "ez report");
    let text = news_text(&done).expect("aviso de término");
    assert!(text.contains("terminó"));
    assert!(text.contains("Todo en verde"));

    let failed = record("ir-3", TaskState::Failed, "vigilante");
    let text = news_text(&failed).expect("aviso de fallo");
    assert!(text.contains("falló"));
    assert!(text.contains("detalle de fallido"));

    let mut replaced = record("ir-7", TaskState::Failed, "ez retry");
    replaced.history[0].detail = "reemplazado por ir-7-r1".to_string();
    assert_eq!(news_text(&replaced), None, "el reintento ya se contó");

    let validated = record("ir-4", TaskState::Validated, "qa");
    let text = news_text(&validated).expect("aviso de validación");
    assert!(text.contains("quedó validado por qa"));
    assert!(text.contains("Sin hallazgos"));

    let fresh = record("ir-5", TaskState::InProgress, "vigilante");
    assert_eq!(news_text(&fresh), None);

    let mut resumed = record("ir-6", TaskState::InProgress, "vigilante");
    resumed.history.insert(
        0,
        Change {
            state: TaskState::Blocked,
            at_ms: 1,
            by: "vigilante".to_string(),
            detail: "límite".to_string(),
        },
    );
    assert_eq!(
        news_text(&resumed),
        Some("ir-6 (OpenCode · glm-5.3) siguió trabajando.".to_string())
    );

    assert!(status_phrase(&blocked).starts_with("ir-1 (OpenCode · glm-5.3) está bloqueado"));
    assert!(status_phrase(&done).contains("Todo en verde"));
    assert!(status_phrase(&fresh).ends_with("está en proceso."));
    assert!(status_phrase(&validated).contains("quedó validado"));
}

#[test]
fn overview_lists_the_newest_and_counts_the_rest() {
    assert_eq!(overview_phrase(&[]), "No tengo subagentes lanzados por mí.");
    let mut first = record("ir-1", TaskState::Blocked, "vigilante");
    first.since_ms = 10;
    let mut second = record("ir-2", TaskState::Done, "ez report");
    second.since_ms = 20;
    let text = overview_phrase(&[first, second]);
    assert_eq!(text, "Tengo 2: ir-2 terminado, ir-1 bloqueado.");
}

#[test]
fn poll_announces_each_novelty_once_and_marks_it() {
    let ctx = ctx("poll");
    let app = &ctx.app;
    let registry = &ctx.registry;
    registry
        .create(&record("ir-1", TaskState::InProgress, "vigilante"))
        .expect("registro");

    app.activity_tick();
    assert!(!has_note(app, "ir-1"));

    registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::Blocked,
            "vigilante",
            "límite de uso del plan gratuito (se reinicia a las 7:00 p.m.)",
            |_| {},
        )
        .expect("bloqueo");
    app.activity_tick();
    assert!(has_note(app, "ir-1 (OpenCode · glm-5.3) está bloqueado"));
    assert!(has_note(app, "límite de uso del plan gratuito"));
    assert_eq!(
        app.inner.agents.retry_offer.lock().clone(),
        Some("ir-1".to_string())
    );
    let count = notes(app).len();
    app.activity_tick();
    assert_eq!(notes(app).len(), count, "la misma novedad no se repite");

    registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::InProgress,
            "vigilante",
            "ya puede seguir",
            |_| {},
        )
        .expect("retoma");
    app.activity_tick();
    assert!(has_note(app, "siguió trabajando"));

    registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::Done,
            "ez report",
            "encargo entregado",
            |record| record.summary = Some("Todo en verde.".to_string()),
        )
        .expect("termina");
    app.activity_tick();
    assert!(has_note(app, "ir-1 (OpenCode · glm-5.3) terminó"));
    assert!(has_note(app, "Todo en verde"));

    registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::Validated,
            "qa",
            "sin hallazgos",
            |record| {
                record.validation = Some(Validation {
                    by: "qa".to_string(),
                    verdict: true,
                    summary: "Sin hallazgos.".to_string(),
                    file: None,
                    at_ms: epoch_ms(),
                })
            },
        )
        .expect("valida");
    app.activity_tick();
    assert!(has_note(app, "quedó validado por qa"));
    assert!(wait_until(|| registry
        .get(VOICE_PARENT, "ir-1")
        .expect("lectura")
        .expect("registro")
        .notified
        == Some(TaskState::Validated)));
}

#[test]
fn parent_made_changes_are_not_announced() {
    let ctx = ctx("parent");
    let app = &ctx.app;
    ctx.registry
        .create(&record(
            "voz1-opencode",
            TaskState::InProgress,
            VOICE_PARENT,
        ))
        .expect("registro");
    app.activity_tick();
    assert!(notes(app).is_empty());

    ctx.registry
        .transition(
            VOICE_PARENT,
            "voz1-opencode",
            TaskState::Blocked,
            VOICE_PARENT,
            "lo pedí yo",
            |_| {},
        )
        .expect("bloqueo");
    app.activity_tick();
    assert!(notes(app).is_empty(), "el padre no se avisa de sí mismo");

    ctx.registry
        .transition(
            VOICE_PARENT,
            "voz1-opencode",
            TaskState::Done,
            "ez report",
            "terminado",
            |record| record.summary = Some("Listo.".to_string()),
        )
        .expect("termina");
    app.activity_tick();
    assert!(has_note(app, "terminó"));
}

#[test]
fn retry_offer_accepts_refuses_and_expires() {
    let ctx = ctx("retry");
    let app = &ctx.app;
    let registry = &ctx.registry;
    registry
        .create(&record("ir-1", TaskState::InProgress, "vigilante"))
        .expect("registro");
    let block = |detail: &str| {
        registry
            .transition(
                VOICE_PARENT,
                "ir-1",
                TaskState::Blocked,
                "vigilante",
                detail,
                |_| {},
            )
            .expect("bloqueo");
    };
    let unblock = || {
        registry
            .transition(
                VOICE_PARENT,
                "ir-1",
                TaskState::InProgress,
                "vigilante",
                "sigue",
                |_| {},
            )
            .expect("retoma");
    };

    block("límite de uso");
    app.activity_tick();
    assert!(app.answer_retry_offer("dale"));
    assert!(wait_until(|| !ctx.retries.lock().is_empty()));
    assert_eq!(*ctx.retries.lock(), vec!["ir-1".to_string()]);
    assert!(wait_until(|| has_note(app, "sigue como ir-1-r1")));
    assert!(app.inner.agents.retry_offer.lock().is_none());

    unblock();
    app.activity_tick();
    block("diálogo");
    app.activity_tick();
    let offer = app.inner.agents.retry_offer.lock().clone();
    assert_eq!(
        offer,
        Some("ir-1".to_string()),
        "notas {:?}; anunciados {:?}",
        notes(app),
        *app.inner.agents.announced.lock()
    );
    assert!(app.answer_retry_offer("no"));
    assert!(has_note(app, "lo dejo así"));
    assert_eq!(ctx.retries.lock().len(), 1);

    unblock();
    app.activity_tick();
    block("otra vez");
    app.activity_tick();
    assert!(!app.answer_retry_offer("¿y el clima?"));
    assert!(app.inner.agents.retry_offer.lock().is_none());
    assert_eq!(ctx.retries.lock().len(), 1);
    assert!(
        !app.answer_retry_offer("dale"),
        "sin oferta no hay reintento"
    );
}

#[test]
fn status_answers_come_from_the_registry() {
    let ctx = ctx("status");
    let app = &ctx.app;
    assert_eq!(
        app.subagents_overview_phrase(),
        "No tengo subagentes lanzados por mí."
    );
    assert!(app
        .subagent_status_phrase("ir-9")
        .contains("No tengo el registro de ir-9"));

    ctx.registry
        .create(&record("ir-1", TaskState::InProgress, "vigilante"))
        .expect("registro");
    ctx.registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::Blocked,
            "vigilante",
            "límite de uso",
            |_| {},
        )
        .expect("bloqueo");
    assert!(app
        .subagent_status_phrase("ir-1")
        .contains("está bloqueado"));
    assert!(app.subagents_overview_phrase().contains("ir-1 bloqueado"));

    assert!(app.fast_command("¿cómo va ir-1?"));
    assert!(wait_until(|| ctx.speaker.spoken.lock().iter().any(
        |text| text.contains("ir-1") && text.contains("está bloqueado")
    )));
    assert!(app.fast_command("¿cómo van los agentes?"));
    assert!(wait_until(|| ctx
        .speaker
        .spoken
        .lock()
        .iter()
        .any(|text| text.contains("Tengo 1: ir-1 bloqueado"))));
    assert!(app.fast_command("¿cómo va ir-9?"));
    assert!(wait_until(|| ctx
        .speaker
        .spoken
        .lock()
        .iter()
        .any(|text| text.contains("No tengo el registro de ir-9"))));
    assert!(!app.fast_command("hola mundo"));
}

fn outcome(row: Value) -> Map<String, Value> {
    row.as_object().cloned().expect("objeto")
}

#[test]
fn a_launch_leaves_a_record_under_the_voice_parent() {
    let temp = TempDir::new("port");
    let registry = Registry::at(temp.0.join("subagentes"));
    let herdr = Herdr::new(VoiceConfig::new());
    let launch = Launch {
        agent: "opencode".to_string(),
        model: Some("glm-5.3".to_string()),
        effort: None,
        cwd: Some("C:\\work".to_string()),
        name: Some("ir-9".to_string()),
        hc_name: None,
        task: "Revisa la rama work/NV-3.".to_string(),
    };
    let row = outcome(json!({
        "ok": true,
        "terminal": "t1",
        "agent": "opencode",
        "model": "glm-5.3",
        "cwd": "C:\\work",
        "launching": true
    }));
    registry
        .create(&record("probe", TaskState::InProgress, VOICE_PARENT))
        .expect("registro directo");
    let created = register_launch(&herdr, &registry, &launch, &row);
    assert!(created.is_ok(), "registro: {created:?}");
    let listed = registry.list_lenient(VOICE_PARENT).expect("lista");
    assert!(
        listed.records.len() == 2,
        "sonda + lanzamiento; inválidos: {:?}",
        listed.invalid
    );
    let saved = registry
        .get(VOICE_PARENT, "ir-9")
        .expect("lectura")
        .expect("registro");
    assert_eq!(saved.terminal, "t1");
    assert_eq!(saved.parent, VOICE_PARENT);
    assert_eq!(saved.agent, "opencode");
    assert_eq!(saved.state, TaskState::InProgress);
    assert_eq!(saved.brief_head, "Revisa la rama work/NV-3.");
    assert_eq!(saved.history[0].by, VOICE_PARENT);
    assert_eq!(saved.notified, None);

    // Un segundo lanzamiento con el mismo nombre visible usa el de §6.
    let again = register_launch(&herdr, &registry, &launch, &row);
    assert!(again.is_ok(), "segundo registro: {again:?}");
    let fallback = registry
        .get(VOICE_PARENT, "voz1-opencode")
        .expect("lectura")
        .expect("nombre automático");
    assert_eq!(fallback.terminal, "t1");
    assert_eq!(
        registry.list(VOICE_PARENT).expect("lista").len(),
        3,
        "dos lanzamientos, dos registros (más la sonda)"
    );

    // Un fallo de `hc` no deja registro.
    let failed = outcome(json!({"ok": false, "error": "sin hc"}));
    assert!(register_launch(&herdr, &registry, &launch, &failed).is_err());
    assert_eq!(registry.list(VOICE_PARENT).expect("lista").len(), 3);
}

#[test]
fn the_status_tool_answers_with_the_record_text() {
    let temp = TempDir::new("tool");
    let registry = Arc::new(Registry::at(temp.0.join("subagentes")));
    registry
        .create(&record("ir-1", TaskState::InProgress, "vigilante"))
        .expect("registro");
    registry
        .transition(
            VOICE_PARENT,
            "ir-1",
            TaskState::Done,
            "ez report",
            "terminado",
            |record| record.summary = Some("Todo en verde.".to_string()),
        )
        .expect("termina");
    let tool = super::status_tool(Arc::clone(&registry));
    let handler = tool.handler.clone();
    let result = handler(
        &json!({"name": "ir-1"})
            .as_object()
            .cloned()
            .expect("objeto"),
    )
    .expect("herramienta");
    let text = match result {
        ez_agent::registry::ToolReturn::Result(result) => result.data,
        other => panic!("resultado inesperado: {other:?}"),
    };
    let message = text
        .get("mensaje")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(message.contains("ir-1 (OpenCode · glm-5.3) terminó"));
    assert!(message.contains("Todo en verde"));
}
