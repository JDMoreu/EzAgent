//! Transiciones del bucle (9.5) como funciones puras.

use std::path::Path;

use crate::autonomous::state::{Action, AutonomousState, Incident, Mode, TaskState};
use crate::autonomous::texts;
use crate::autonomous::verdict::{Finding, Severity, Verdict, VerdictError};

fn task(max_rounds: i64, max_minutes: i64) -> AutonomousState {
    AutonomousState::new(
        "20261116-120000-abcd",
        Mode::Single,
        1_000.0,
        max_rounds,
        max_minutes,
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

fn folder() -> &'static Path {
    Path::new("C:/tarea")
}

fn correction(round: i64) -> Action {
    Action::SendCorrection(
        folder()
            .join(format!("correccion-{round}.md"))
            .to_string_lossy()
            .to_string(),
    )
}

#[test]
fn two_clean_rounds_finish() {
    let mut state = task(5, 120);
    assert_eq!(state.on_executor_finished(1_010.0), Action::LaunchVerifier);
    assert_eq!(
        state.on_verdict(Ok(clean()), folder(), 1_020.0),
        Action::LaunchVerifier
    );
    assert_eq!(state.clean_streak, 1);
    assert_eq!(state.state, TaskState::Verifying);
    assert_eq!(
        state.on_verdict(Ok(clean()), folder(), 1_030.0),
        Action::Done
    );
    assert_eq!(state.state, TaskState::Done);
    assert!(!state.unfinished());
    assert_eq!(
        state.on_verdict(Ok(clean()), folder(), 1_040.0),
        Action::Wait
    );
}

#[test]
fn dirty_round_sends_correction_and_next_round() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    let action = state.on_verdict(Ok(dirty("falta la prueba")), folder(), 1_020.0);
    assert_eq!(action, correction(1));
    assert_eq!(state.round, 2);
    assert_eq!(state.state, TaskState::Running);
    assert_eq!(state.clean_streak, 0);
    assert_eq!(state.last_findings, vec!["falta la prueba".to_string()]);
}

#[test]
fn clean_after_correction_needs_two_more_rounds() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("falta la prueba")), folder(), 1_020.0);
    state.on_executor_finished(1_030.0);
    assert_eq!(
        state.on_verdict(Ok(clean()), folder(), 1_040.0),
        Action::LaunchVerifier
    );
    assert_eq!(
        state.on_verdict(Ok(clean()), folder(), 1_050.0),
        Action::Done
    );
}

#[test]
fn churn_pauses_with_the_same_problem() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(
        Ok(dirty("la prueba del criterio 2 falla")),
        folder(),
        1_020.0,
    );
    state.on_executor_finished(1_030.0);
    let action = state.on_verdict(
        Ok(dirty("La prueba del criterio 2 falla.")),
        folder(),
        1_040.0,
    );
    assert_eq!(action, Action::Pause(texts::REASON_CHURN.to_string()));
    assert_eq!(state.state, TaskState::Paused);
}

#[test]
fn a_clean_round_clears_the_previous_findings() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("la prueba falla")), folder(), 1_020.0);
    state.on_executor_finished(1_030.0);
    state.on_verdict(Ok(clean()), folder(), 1_040.0);
    assert!(state.last_findings.is_empty());
    state.on_executor_finished(1_050.0);
    let action = state.on_verdict(Ok(dirty("la prueba falla")), folder(), 1_060.0);
    assert_eq!(action, correction(2));
}

#[test]
fn round_cap_pauses() {
    let mut state = task(1, 120);
    state.on_executor_finished(1_010.0);
    let action = state.on_verdict(Ok(dirty("cualquiera")), folder(), 1_020.0);
    assert_eq!(action, Action::Pause(texts::REASON_ROUNDS.to_string()));
    assert_eq!(state.state, TaskState::Paused);
    assert_eq!(state.round, 2);
}

#[test]
fn deadline_pauses() {
    let mut state = task(5, 1);
    assert_eq!(
        state.on_deadline(1_061.0),
        Action::Pause(texts::REASON_DEADLINE.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);

    let mut later = task(5, 1);
    later.on_executor_finished(1_010.0);
    assert_eq!(
        later.on_verdict(Ok(clean()), folder(), 1_100.0),
        Action::Pause(texts::REASON_DEADLINE.to_string())
    );
}

#[test]
fn invalid_verdict_retries_once_then_pauses() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    assert_eq!(
        state.on_verdict(invalid(), folder(), 1_020.0),
        Action::LaunchVerifier
    );
    assert_eq!(
        state.on_verdict(invalid(), folder(), 1_030.0),
        Action::Pause(texts::REASON_VERDICT.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
}

#[test]
fn valid_verdict_after_a_bad_one_clears_the_counter() {
    let mut state = task(5, 120);
    state.on_executor_finished(1_010.0);
    state.on_verdict(invalid(), folder(), 1_020.0);
    state.on_verdict(Ok(clean()), folder(), 1_030.0);
    assert_eq!(state.verify_failures, 0);
    assert_eq!(state.clean_streak, 1);
}

#[test]
fn incidents_pause_like_the_design() {
    let mut state = task(5, 120);
    assert_eq!(state.on_incident(&Incident::ExecutorError), Action::Wait);
    assert_eq!(
        state.on_incident(&Incident::ExecutorError),
        Action::Pause(texts::REASON_EXECUTOR.to_string())
    );

    let mut quota = task(5, 120);
    assert_eq!(
        quota.on_incident(&Incident::Quota),
        Action::Pause(texts::REASON_QUOTA.to_string())
    );
    assert_eq!(
        quota.on_incident(&Incident::Denied("toca credenciales".to_string())),
        Action::Wait
    );

    let mut dialog = task(5, 120);
    assert_eq!(
        dialog.on_incident(&Incident::Dialog("¿Permitir?".to_string())),
        Action::Pause("no supe qué responder a un diálogo del agente: ¿Permitir?".to_string())
    );
}

#[test]
fn pause_resume_and_stop() {
    let mut state = task(5, 120);
    assert_eq!(state.pause("prueba"), Action::Pause("prueba".to_string()));
    assert_eq!(state.state, TaskState::Paused);
    assert_eq!(state.pause("otra"), Action::Wait);
    assert_eq!(state.resume(1_100.0), Action::Resume);
    assert_eq!(state.state, TaskState::Running);
    assert_eq!(state.pause_reason, None);
    assert_eq!(state.stop("por voz"), Action::Stopped);
    assert_eq!(state.state, TaskState::Stopped);
    assert_eq!(state.pause("x"), Action::Wait);
    assert_eq!(state.stop("x"), Action::Wait);
    assert_eq!(state.resume(1_100.0), Action::Wait);
}

#[test]
fn resume_gives_one_more_round_and_the_same_window() {
    let mut state = task(1, 1);
    state.on_executor_finished(1_010.0);
    state.on_verdict(Ok(dirty("x")), folder(), 1_020.0);
    assert_eq!(state.round, 2);
    assert_eq!(state.resume(1_030.0), Action::Resume);
    assert_eq!(state.max_rounds, 2);
    assert_eq!(state.deadline, 1_060.0);

    let mut expired = task(5, 1);
    assert_eq!(
        expired.on_deadline(1_100.0),
        Action::Pause(texts::REASON_DEADLINE.to_string())
    );
    expired.resume(1_100.0);
    assert_eq!(expired.deadline, 1_160.0);
}

#[test]
fn restart_never_resumes_alone() {
    let mut state = task(5, 120);
    assert_eq!(
        state.on_restart(),
        Action::Pause(texts::REASON_RESTART.to_string())
    );
    assert_eq!(state.state, TaskState::Paused);
    let mut stopped = task(5, 120);
    stopped.stop("");
    assert_eq!(stopped.on_restart(), Action::Wait);
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
fn scope_tracks_the_terminals_of_the_task() {
    let mut state = task(5, 120);
    assert_eq!(state.executor_started("addr1", "codex"), Action::Wait);
    assert_eq!(state.state, TaskState::Running);
    assert_eq!(state.scope, vec!["addr1".to_string()]);
    state.verifier_started("addr1");
    assert_eq!(state.scope.len(), 1);
    state.verifier_started("addr2");
    assert_eq!(state.scope, vec!["addr1".to_string(), "addr2".to_string()]);
    state.forget("addr2");
    assert_eq!(state.scope, vec!["addr1".to_string()]);
    let executor = state.executor.expect("ejecutor");
    assert_eq!(executor.agent, "codex");
}
