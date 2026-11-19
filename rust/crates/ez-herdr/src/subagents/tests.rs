//! Pruebas del registro de subagentes y de los textos de aviso (§13).

use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{notice_text, one_line, pending_notices, Change, Record, TaskState, Validation};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Carpeta temporal con nombre único; se borra al caer.
pub(super) struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    pub(super) fn new() -> Self {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ez-subagents-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("crear carpeta temporal de prueba");
        Self { path }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Registro de ejemplo con valores coherentes y sin estado avisado.
pub(super) fn rec(name: &str, parent: &str, terminal: &str, state: TaskState) -> Record {
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: parent.to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("deepseek-v4.1-flash".to_string()),
        effort: None,
        role: "developer".to_string(),
        cwd: r"C:\work\proj".to_string(),
        brief_head: "Encargo de prueba".to_string(),
        state,
        since_ms: 1,
        history: Vec::new(),
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// Cambio de estado de ejemplo.
pub(super) fn change(state: TaskState, at_ms: u64, by: &str, detail: &str) -> Change {
    Change {
        state,
        at_ms,
        by: by.to_string(),
        detail: detail.to_string(),
    }
}

#[test]
fn task_state_names_and_serde_round_trip() {
    let cases = [
        (TaskState::InProgress, "en_proceso"),
        (TaskState::Blocked, "bloqueado"),
        (TaskState::Done, "terminado"),
        (TaskState::Failed, "fallido"),
        (TaskState::Validated, "validado"),
    ];
    for (state, text) in cases {
        assert_eq!(state.as_str(), text);
        assert_eq!(state.to_string(), text);
        assert_eq!(String::from(state), text);
        assert_eq!(TaskState::parse(text), Some(state));
        assert_eq!(
            serde_json::to_string(&state).expect("serializar estado"),
            format!("\"{text}\"")
        );
        let back: TaskState =
            serde_json::from_str(&format!("\"{text}\"")).expect("deserializar estado");
        assert_eq!(back, state);
    }
    assert_eq!(TaskState::parse("otro"), None);
    assert!(!TaskState::InProgress.is_final());
    assert!(!TaskState::Blocked.is_final());
    assert!(!TaskState::Done.is_final());
    assert!(TaskState::Failed.is_final());
    assert!(TaskState::Validated.is_final());
}

#[test]
fn record_uses_contract_field_names() {
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::Done);
    record.summary = Some("listo".to_string());
    record.report = Some("informe.md".to_string());
    record.notified = Some(TaskState::Done);
    record
        .history
        .push(change(TaskState::Done, 2, "dev1-opencode", "fin"));

    let json = serde_json::to_value(&record).expect("serializar registro");
    for key in [
        "name",
        "terminal",
        "parent",
        "agent",
        "label",
        "model",
        "effort",
        "role",
        "cwd",
        "brief_head",
        "state",
        "since_ms",
        "history",
        "summary",
        "report",
        "validation",
        "notified",
    ] {
        assert!(json.get(key).is_some(), "falta el campo {key}");
    }
    assert_eq!(json["state"], "terminado");
    assert_eq!(json["brief_head"], "Encargo de prueba");
    let back: Record = serde_json::from_value(json).expect("deserializar registro");
    assert_eq!(back, record);
}

#[test]
fn one_line_flattens_control_chars_and_truncates() {
    assert_eq!(one_line("  hola\n  mundo\t!", 300), "hola mundo !");
    assert_eq!(one_line("a\0b", 300), "a b");
    assert_eq!(one_line("", 300), "");
    assert_eq!(one_line("abc", 2), "a…");
    assert_eq!(one_line("abc", 0), "");
    let long = "x".repeat(400);
    let out = one_line(&long, 300);
    assert_eq!(out.chars().count(), 300);
    assert!(out.ends_with('…'));
}

#[test]
fn notice_text_for_done() {
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::Done);
    record.summary = Some("Terminé la tarea sin problemas.".to_string());
    record.report = Some(r"C:\tmp\informe.md".to_string());
    assert_eq!(
        notice_text(&record),
        "[EzAgent · aviso] dev1-opencode (OpenCode · deepseek-v4.1-flash) terminó. \
         Resumen del subagente: «Terminé la tarea sin problemas.». \
         Informe: C:\\tmp\\informe.md. Para validarlo: ez validate dev1-opencode \
         --pass|--fail --file <evidencia>"
    );

    record.model = None;
    assert!(notice_text(&record).contains("(OpenCode) terminó."));
}

#[test]
fn notice_text_for_failed_uses_last_detail() {
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::Failed);
    record.history.push(change(
        TaskState::Failed,
        5,
        "t6h",
        "la terminal terminó sin informe (código 1)",
    ));
    assert_eq!(
        notice_text(&record),
        "[EzAgent · aviso] dev1-opencode (OpenCode · deepseek-v4.1-flash) falló: \
         la terminal terminó sin informe (código 1). Revisa su pantalla: hc read t12"
    );
}

#[test]
fn notice_text_for_validated_both_verdicts() {
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::Validated);
    record.validation = Some(Validation {
        by: "t7".to_string(),
        verdict: true,
        summary: "Revisé los cambios, todo bien.".to_string(),
        file: None,
        at_ms: 6,
    });
    assert_eq!(
        notice_text(&record),
        "[EzAgent · aviso] dev1-opencode quedó validado por t7: «Revisé los cambios, todo bien.»."
    );

    if let Some(validation) = record.validation.as_mut() {
        validation.verdict = false;
    }
    assert_eq!(
        notice_text(&record),
        "[EzAgent · aviso] dev1-opencode no pasó la validación de t7: «Revisé los cambios, todo bien.»."
    );
}

#[test]
fn pending_notices_excludes_parent_changes_and_already_notified() {
    let mut mine = rec("dev1-opencode", "t6h", "t11", TaskState::Done);
    mine.history
        .push(change(TaskState::Done, 1, "dev1-opencode", "fin"));

    let mut notified = rec("qa1-opencode", "t6h", "t12", TaskState::Done);
    notified.notified = Some(TaskState::Done);
    notified
        .history
        .push(change(TaskState::Done, 2, "qa1-opencode", "fin"));

    let mut by_parent = rec("valqa1-opencode", "t6h", "t13", TaskState::Done);
    by_parent
        .history
        .push(change(TaskState::Done, 3, "t6h", "cambió el padre"));

    let mut changed_again = rec("plan1-opencode", "t6h", "t14", TaskState::Done);
    changed_again.notified = Some(TaskState::InProgress);
    changed_again
        .history
        .push(change(TaskState::Done, 4, "plan1-opencode", "fin"));

    let records = [mine, notified, by_parent, changed_again];
    let pending = pending_notices(&records);
    let names: Vec<&str> = pending.iter().map(|record| record.name.as_str()).collect();
    assert_eq!(names, ["dev1-opencode", "plan1-opencode"]);
}

#[test]
fn notice_text_for_blocked_and_for_resuming() {
    let mut blocked = rec("dev1-opencode", "t6h", "t12", TaskState::Blocked);
    blocked
        .history
        .push(change(TaskState::InProgress, 1, "t6h", "encargo entregado"));
    blocked.history.push(change(
        TaskState::Blocked,
        2,
        "vigilante",
        "límite de uso del plan gratuito (se reinicia a las 19:00)",
    ));
    assert_eq!(
        notice_text(&blocked),
        "[EzAgent · aviso] dev1-opencode (OpenCode · deepseek-v4.1-flash) está bloqueado: \
         límite de uso del plan gratuito (se reinicia a las 19:00). Opciones: \
         ez retry dev1-opencode --auto | ez retry dev1-opencode --agent … | esperar."
    );

    blocked.state = TaskState::InProgress;
    blocked.history.push(change(
        TaskState::InProgress,
        3,
        "vigilante",
        "siguió trabajando",
    ));
    assert_eq!(
        notice_text(&blocked),
        "[EzAgent · aviso] dev1-opencode (OpenCode · deepseek-v4.1-flash) siguió trabajando."
    );
}

mod registry;
