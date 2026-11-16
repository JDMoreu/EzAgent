//! Transiciones del bucle (10.1, 10.10, 10.13 y 10.14) como funciones puras.

use std::path::Path;

use crate::autonomous::state::{
    Action, AutonomousState, Incident, Limits, Mode, ScopeRole, TaskState, IDLE_LIMIT_S,
};
use crate::autonomous::texts;
use crate::autonomous::verdict::{Finding, Severity, Verdict, VerdictError};

fn task(max_rounds: i64, max_minutes: i64) -> AutonomousState {
    AutonomousState::new(
        "20261116-120000-abcd",
        Mode::Single,
        "C:/proyecto",
        1_000.0,
        Limits {
            max_rounds,
            max_minutes,
            max_approvals: 200,
        },
    )
}

fn finding(severity: Severity, problem: &str) -> Finding {
    Finding {
        severity,
        criterion: Some(1),
        problem: problem.to_string(),
        evidence: String::new(),
    }
}

fn clean() -> Verdict {
    Verdict {
        cumple: true,
        criteria: Vec::new(),
        findings: Vec::new(),
    }
}

fn dirty(problem: &str) -> Verdict {
    Verdict {
        cumple: false,
        criteria: Vec::new(),
        findings: vec![finding(Severity::Major, problem)],
    }
}

fn invalid() -> Result<Verdict, VerdictError> {
    Err(VerdictError("El veredicto no es JSON válido.".to_string()))
}

fn shared() -> &'static Path {
    Path::new("C:/tarea/compartido")
}

fn correction(round: i64) -> Action {
    Action::SendCorrection(
        shared()
            .join(format!("correccion-{round}.md"))
            .to_string_lossy()
            .to_string(),
    )
}

#[test]
fn two_clean_rounds_finish_and_close_the_scope() {
    let mut state = task(5, 120);
    state.executor_started("addr1", "codex");
    assert_eq!(state.on_executor_finished(1_010.0), Action::LaunchVerifier);
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_020.0),
        Action::LaunchVerifier
    );
    assert_eq!(state.clean_streak, 1);
    assert_eq!(state.state, TaskState::Verifying);
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_030.0),
        Action::Done
    );
    assert_eq!(state.state, TaskState::Done);
    assert!(
        state.scope.is_empty(),
        "la autorización termina con la tarea"
    );
    assert!(!state.unfinished());
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_040.0),
        Action::Wait
    );
}

#[test]
fn dirty_round_sends_correction_and_next_round() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    let action = state.on_verdict(Ok(dirty("falta la prueba")), shared(), 1_020.0);
    assert_eq!(action, correction(1));
    assert_eq!(state.round, 2);
    assert_eq!(state.state, TaskState::Running);
    assert_eq!(state.clean_streak, 0);
    assert_eq!(
        state.corrected_findings,
        vec!["falta la prueba".to_string()]
    );
}

#[test]
fn clean_after_correction_needs_two_more_rounds() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("falta la prueba")), shared(), 1_020.0);
    state.on_executor_finished(1_030.0);
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_040.0),
        Action::LaunchVerifier
    );
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_050.0),
        Action::Done
    );
}

#[test]
fn churn_pauses_with_the_same_problem() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(
        Ok(dirty("la prueba del criterio 2 falla")),
        shared(),
        1_020.0,
    );
    state.on_executor_finished(1_030.0);
    let action = state.on_verdict(
        Ok(dirty("La prueba del criterio 2 falla.")),
        shared(),
        1_040.0,
    );
    assert_eq!(action, Action::Pause(texts::REASON_CHURN.to_string()));
    assert_eq!(state.state, TaskState::Paused);
}

#[test]
fn churn_compares_against_every_corrected_finding() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("la prueba falla")), shared(), 1_020.0);
    state.on_executor_finished(1_030.0);
    // Una ronda limpia no borra el historial de lo ya corregido (10.14).
    state.on_verdict(Ok(clean()), shared(), 1_040.0);
    state.on_verdict(Ok(dirty("La prueba falla.")), shared(), 1_050.0);
    assert_eq!(
        state.corrected_findings,
        vec!["la prueba falla".to_string()]
    );
    assert_eq!(state.state, TaskState::Paused);
}

#[test]
fn round_cap_pauses() {
    let mut state = task(1, 120);
    state.on_executor_finished(1_010.0);
    let action = state.on_verdict(Ok(dirty("cualquiera")), shared(), 1_020.0);
    assert_eq!(action, Action::Pause(texts::REASON_ROUNDS.to_string()));
    assert_eq!(state.state, TaskState::Paused);
    assert_eq!(state.round, 2);
}

#[test]
fn deadline_pauses_and_paused_time_does_not_count() {
    let mut state = task(5, 1);
    assert_eq!(
        state.on_deadline(1_061.0),
        Action::Pause(texts::REASON_DEADLINE.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
    assert_eq!(state.paused_at, Some(1_061.0));

    // Diez minutos en pausa corren el plazo lo mismo (10.13).
    assert_eq!(state.resume(1_661.0, 0, 0), Action::Resume);
    assert_eq!(state.deadline, 1_660.0);
    assert_eq!(state.on_deadline(1_659.0), Action::Wait);
    assert_eq!(
        state.on_deadline(1_661.0),
        Action::Pause(texts::REASON_DEADLINE.to_string())
    );
}

#[test]
fn invalid_verdict_retries_once_then_pauses() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    assert_eq!(
        state.on_verdict(invalid(), shared(), 1_020.0),
        Action::LaunchVerifier
    );
    assert_eq!(
        state.on_verdict(invalid(), shared(), 1_030.0),
        Action::Pause(texts::REASON_VERDICT.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
}

#[test]
fn valid_verdict_after_a_bad_one_clears_the_counter() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(invalid(), shared(), 1_020.0);
    state.on_verdict(Ok(clean()), shared(), 1_030.0);
    assert_eq!(state.verify_failures, 0);
    assert_eq!(state.clean_streak, 1);
}

#[test]
fn three_verifications_per_round_is_the_cap() {
    let mut state = task(5, 120);
    assert_eq!(state.on_executor_finished(1_010.0), Action::LaunchVerifier);
    assert_eq!(
        state.on_verdict(invalid(), shared(), 1_020.0),
        Action::LaunchVerifier
    );
    assert_eq!(
        state.on_verdict(Ok(clean()), shared(), 1_030.0),
        Action::LaunchVerifier
    );
    assert_eq!(state.verifications_this_round, 3);
    assert_eq!(
        state.on_verdict(invalid(), shared(), 1_040.0),
        Action::Pause(texts::REASON_VERIFICATIONS.to_string())
    );
}

#[test]
fn incidents_pause_like_the_design() {
    let mut state = task(5, 120);
    assert_eq!(
        state.on_incident(&Incident::ExecutorError, 1_010.0),
        Action::Wait
    );
    assert_eq!(
        state.on_incident(&Incident::ExecutorError, 1_020.0),
        Action::Pause(texts::REASON_EXECUTOR.to_string())
    );

    let mut quota = task(5, 120);
    assert_eq!(
        quota.on_incident(&Incident::Quota, 1_010.0),
        Action::Pause(texts::REASON_QUOTA.to_string())
    );
    assert_eq!(
        quota.on_incident(&Incident::Denied("toca credenciales".to_string()), 1_020.0),
        Action::Wait
    );

    let mut lost = task(5, 120);
    assert_eq!(
        lost.on_incident(&Incident::Lost, 1_010.0),
        Action::Pause(texts::REASON_LOST.to_string())
    );

    let mut dialog = task(5, 120);
    assert_eq!(
        dialog.on_incident(&Incident::Dialog("¿Permitir?".to_string()), 1_010.0),
        Action::Pause("no supe qué responder a un diálogo del agente: ¿Permitir?".to_string())
    );
}

#[test]
fn pause_resume_and_stop() {
    let mut state = task(5, 120);
    assert_eq!(
        state.pause("prueba", 1_010.0),
        Action::Pause("prueba".to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
    assert_eq!(state.pause("otra", 1_020.0), Action::Wait);
    assert_eq!(state.resume(1_100.0, 0, 0), Action::Resume);
    assert_eq!(state.state, TaskState::Running);
    assert_eq!(state.pause_reason, None);
    assert_eq!(state.stop("por voz", false), Action::Stopped);
    assert_eq!(state.state, TaskState::Stopped);
    assert_eq!(state.pause("x", 1_200.0), Action::Wait);
    assert_eq!(state.stop("x", false), Action::Wait);
    assert_eq!(state.resume(1_200.0, 0, 0), Action::Wait);
}

#[test]
fn resume_extends_rounds_and_minutes() {
    let mut state = task(1, 1);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("x")), shared(), 1_020.0);
    assert_eq!(state.round, 2);
    assert_eq!(state.resume(1_030.0, 2, 30), Action::Resume);
    assert_eq!(state.max_rounds, 3);
    assert_eq!(state.deadline, 1_060.0 + 10.0 + 30.0 * 60.0);
}

#[test]
fn a_done_task_can_be_accepted_but_stopping_never_accepts() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(clean()), shared(), 1_020.0);
    state.on_verdict(Ok(clean()), shared(), 1_030.0);
    assert_eq!(state.stop("", true), Action::Accepted);
    assert!(state.accepted);
    assert_eq!(state.stop("", true), Action::Wait);

    let mut running = task(5, 120);
    running.executor_started("addr1", "codex");
    assert_eq!(running.stop("para", true), Action::Stopped);
    assert!(!running.accepted, "parar en curso no acepta el resultado");
    assert!(running.scope.is_empty());
}

#[test]
fn restart_never_resumes_alone() {
    let mut state = task(5, 120);
    assert_eq!(
        state.on_restart(1_010.0),
        Action::Pause(texts::REASON_RESTART.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
    let mut stopped = task(5, 120);
    stopped.stop("", false);
    assert_eq!(stopped.on_restart(1_010.0), Action::Wait);
}

#[test]
fn only_blocking_or_major_findings_dirty_a_round() {
    let verdict = Verdict {
        cumple: true,
        criteria: Vec::new(),
        findings: vec![finding(Severity::Minor, "estilo")],
    };
    assert!(verdict.clean());
    let unfinished = Verdict {
        cumple: false,
        criteria: Vec::new(),
        findings: Vec::new(),
    };
    assert!(!unfinished.clean());
    let blocked = Verdict {
        cumple: true,
        criteria: Vec::new(),
        findings: vec![finding(Severity::Blocking, "no compila")],
    };
    assert!(!blocked.clean());
}

#[test]
fn a_verdict_that_does_not_say_what_to_fix_pauses() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    let action = state.on_verdict(
        Ok(Verdict {
            cumple: false,
            criteria: Vec::new(),
            findings: Vec::new(),
        }),
        shared(),
        1_020.0,
    );
    assert_eq!(action, Action::Pause(texts::REASON_DOUBTFUL.to_string()));
}

#[test]
fn scope_tracks_roles_and_editable_folders() {
    let mut state = task(5, 120);
    state.executor_started("addr1", "codex");
    let entry = state.approval_scope("addr1").expect("ejecutor");
    assert_eq!(entry.role, ScopeRole::Executor);
    assert_eq!(entry.edit_dirs, ["C:/proyecto"]);

    state.verifier_started("addr2", &["C:/tarea/verificacion-1".to_string()]);
    let verifier = state.approval_scope("addr2").expect("verificador");
    assert_eq!(verifier.role, ScopeRole::Verifier);
    assert_eq!(verifier.edit_dirs, ["C:/tarea/verificacion-1"]);

    state.member_started("addr3");
    assert_eq!(
        state.approval_scope("addr3").map(|e| e.role),
        Some(ScopeRole::Member)
    );
    assert_eq!(
        state.approval_scope("addr3").map(|e| e.edit_dirs.clone()),
        Some(vec!["C:/proyecto".to_string()])
    );

    // Una dirección ya cubierta se actualiza, no se duplica.
    state.verifier_started("addr1", &["C:/tarea/verificacion-2".to_string()]);
    assert_eq!(state.scope.len(), 3);
    assert_eq!(
        state.approval_scope("addr1").map(|e| e.role),
        Some(ScopeRole::Verifier)
    );

    state.forget("addr2");
    assert!(state.approval_scope("addr2").is_none());

    // En pausa no se aprueba nada.
    state.pause("motivo", 1_010.0);
    assert!(state.approval_scope("addr1").is_none());
}

#[test]
fn approvals_pause_at_the_cap() {
    let mut state = AutonomousState::new(
        "20261116-120000-abcd",
        Mode::Single,
        "C:/proyecto",
        1_000.0,
        Limits {
            max_rounds: 5,
            max_minutes: 120,
            max_approvals: 2,
        },
    );
    state.executor_started("addr1", "codex");
    assert_eq!(state.record_approval(1_010.0), Action::Wait);
    assert_eq!(state.approvals, 1);
    assert_eq!(
        state.record_approval(1_020.0),
        Action::Pause(texts::REASON_APPROVALS.to_string())
    );
    assert_eq!(state.approvals, 2);
}

#[test]
fn idle_finishes_with_fin_and_pauses_without_it() {
    let mut state = task(5, 120);
    assert_eq!(
        state.on_executor_idle(1_010.0, false, 10.0, "codex"),
        Action::Wait
    );
    assert_eq!(
        state.on_executor_idle(1_010.0, false, IDLE_LIMIT_S, "codex"),
        Action::Pause(texts::idle("codex"))
    );

    let mut finished = task(5, 120);
    assert_eq!(
        finished.on_executor_idle(1_010.0, true, 1.0, "codex"),
        Action::LaunchVerifier
    );
}

#[test]
fn verification_numbers_are_global() {
    let mut state = task(5, 120);
    assert_eq!(state.next_verification(), 1);
    assert_eq!(state.next_verification(), 2);
    assert_eq!(state.verification_seq, 2);
}
