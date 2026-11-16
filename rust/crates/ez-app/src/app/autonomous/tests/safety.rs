//! Seguridad de la aprobación: lista de denegación, `stop` a media
//! transacción, diálogo que cambia y claves fuera de auditoría y bitácora.

use ez_agent::autonomous::TaskState;

use super::{Harness, CLAUDE_BASH, CODEX_EXEC};

const DENIED: &str = " Bash command\n\n   git push --force origin main\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n";

#[test]
fn a_denied_command_pauses_without_sending_keys() {
    let h = Harness::new();
    let _id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "blocked", DENIED);
    assert!(h.wait(|| h.paused_reason().is_some()), "pausa");
    let reason = h.paused_reason().expect("motivo");
    assert!(reason.contains("--force"), "{reason}");
    assert!(h.port.keys_sent().is_empty(), "no se aprueba lo denegado");
}

#[test]
fn stop_between_the_first_read_and_the_keys_sends_nothing() {
    let h = Harness::new();
    let _id = h.start();
    assert!(h.wait_launch(1));
    let loop_ = h.loop_.clone();
    h.clock.after_next_sleep(move || {
        let _ = loop_.stop_task("prueba", false);
    });
    h.port.set("t1", "blocked", CLAUDE_BASH);
    assert!(h.wait_state(TaskState::Stopped), "se detuvo");
    assert!(h.port.keys_sent().is_empty(), "la generación lo impidió");
}

#[test]
fn a_dialog_that_changes_between_the_two_reads_is_not_approved() {
    let h = Harness::new();
    let _id = h.start();
    assert!(h.wait_launch(1));
    h.port.set("t1", "blocked", CLAUDE_BASH);
    h.port.script("t1", &[CLAUDE_BASH, CODEX_EXEC]);
    assert!(h.wait(|| h.paused_reason().is_some()), "pausa");
    assert_eq!(
        h.paused_reason().as_deref(),
        Some("el diálogo cambió mientras lo leía")
    );
    assert!(h.port.keys_sent().is_empty());
}

#[test]
fn a_fake_key_on_screen_never_reaches_audit_or_the_log() {
    let h = Harness::new();
    let id = h.start();
    assert!(h.wait_launch(1));
    let screen = format!("{CLAUDE_BASH}\nAPI_KEY=sk-live-abc123def456\n");
    h.port.set("t1", "blocked", &screen);
    assert!(h.wait_keys(1), "se aprobó");
    assert!(
        h.wait(|| h.audit_text().contains("autonomous.approve")),
        "se registró la aprobación"
    );
    assert!(h.wait_state(TaskState::Running));
    let audit = h.audit_text();
    let log = h.bitacora(&id);
    assert!(audit.contains("autonomous.approve"), "{audit}");
    assert!(!audit.contains("sk-live-abc123def456"), "{audit}");
    assert!(!log.contains("sk-live-abc123def456"), "{log}");
    assert!(!audit.contains("API_KEY"), "{audit}");
    assert!(!log.contains("API_KEY"), "{log}");
}
