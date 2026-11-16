//! El hilo `ezagent-autonomo`: sondea, aprueba, verifica y aplica las
//! transiciones puras de `ez_agent::autonomous` (10.6, 10.7, 10.10 y 10.13).
//!
//! El hilo es dueño de [`Session`]. Los verbos de las herramientas solo dejan
//! una [`super::Request`]; aquí se aplican. Ningún cerrojo se retiene mientras
//! se llama a Herdr, al disco o al disco de la bitácora.

use std::sync::Arc;

use ez_agent::autonomous::{Action, Incident, TaskState, VerdictSpec};
use ez_agent::tools_herdr::port::PortError;
use ez_herdr::herdr::HerdrError;

use super::approve::{approve_blocked, Approval};
use super::host::start::launch_executor;
use super::verify;
use super::{AutonomousLoop, Request, Session, POLL_S};

impl AutonomousLoop {
    /// Cuerpo del hilo: atiende verbos y sondea según el estado.
    pub(super) fn run(self: Arc<Self>, mut session: Session, epoch: u64) {
        loop {
            let polling = matches!(
                session.state.state,
                TaskState::Preparing | TaskState::Running | TaskState::Verifying
            );
            let taken = if polling {
                self.take_requests_now(epoch)
            } else {
                self.wait_requests(epoch)
            };
            let Some(requests) = taken else {
                return;
            };
            for request in requests {
                self.handle_request(&mut session, request);
            }
            if matches!(
                session.state.state,
                TaskState::Preparing | TaskState::Running | TaskState::Verifying
            ) {
                match session.state.state {
                    TaskState::Preparing | TaskState::Running => {
                        self.tick_running(&mut session);
                    }
                    TaskState::Verifying => {
                        self.tick_verifying(&mut session);
                    }
                    _ => {}
                }
                self.clock.sleep(POLL_S);
            }
            self.commit(epoch, &session);
        }
    }

    /// «Para» y «sigue» (10.13).
    fn handle_request(&self, session: &mut Session, request: Request) {
        match request {
            Request::Stop { reason, accept } => {
                let verifiers = verifier_addresses(session);
                let action = session.state.stop(&reason, accept);
                self.apply(session, action);
                close_verifiers(self, session, verifiers);
            }
            Request::Resume { rounds, minutes } => self.resume(session, rounds, minutes),
        }
    }

    fn resume(&self, session: &mut Session, mut rounds: i64, mut minutes: i64) {
        if session.state.state != TaskState::Paused {
            return;
        }
        let reason = session.state.pause_reason.clone().unwrap_or_default();
        if rounds == 0 && minutes == 0 && is_cap(&reason) {
            // Decisión 1: prórroga por omisión al reanudar un tope.
            rounds = 3;
            minutes = 60;
            session.state.max_approvals += 100;
        }
        let action = session.state.resume(self.now(), rounds, minutes);
        session.idle_since = None;
        self.apply(session, action);
    }

    /// Un tic en ejecución: topes, ejecutor y demás terminales de `scope`.
    fn tick_running(&self, session: &mut Session) {
        let now = self.now();
        let action = session.state.on_deadline(now);
        if matches!(action, Action::Pause(_)) {
            self.apply(session, action);
            return;
        }
        let Some(executor) = session.state.executor.clone() else {
            return;
        };
        self.poll_executor(session, &executor.address, &executor.agent, now);
        if session.state.state != TaskState::Running {
            return;
        }
        let others: Vec<String> = session
            .state
            .scope
            .iter()
            .filter(|entry| {
                entry.address != executor.address
                    && entry.role != ez_agent::autonomous::ScopeRole::Executor
            })
            .map(|entry| entry.address.clone())
            .collect();
        for address in others {
            self.poll_other(session, &address);
        }
    }

    fn poll_executor(&self, session: &mut Session, address: &str, agent: &str, now: f64) {
        let row = match self.port.get_agent(address) {
            Ok(row) => row,
            Err(PortError::Herdr(HerdrError::NotFound(_))) => {
                self.pause(session, ez_agent::autonomous::texts::REASON_LOST);
                return;
            }
            Err(error) => {
                tracing::debug!("No pude leer {address}: {error}");
                return;
            }
        };
        match state_of(&row).as_str() {
            "blocked" => self.blocked(session, address),
            "working" => {
                session.idle_since = None;
                session.state.executor_errors = 0;
            }
            "idle" => self.executor_idle(session, agent, now),
            "done" => {
                session.idle_since = None;
                let action = session.state.on_executor_finished(now);
                self.apply(session, action);
            }
            "error" => self.executor_failed(session, address, now),
            _ => {}
        }
    }

    fn executor_idle(&self, session: &mut Session, agent: &str, now: f64) {
        let since = *session.idle_since.get_or_insert(now);
        let fin = session
            .dir
            .fin_written_after(session.state.round, session.sent_at);
        let action = session.state.on_executor_idle(now, fin, now - since, agent);
        if !matches!(action, Action::Wait) {
            session.idle_since = None;
            self.apply(session, action);
        }
    }

    fn executor_failed(&self, session: &mut Session, address: &str, now: f64) {
        if let Some(reason) = self.screen_incident(address) {
            self.pause(session, &reason);
            return;
        }
        let action = session.state.on_incident(&Incident::ExecutorError, now);
        self.apply(session, action);
    }

    /// Un tic verificando: el verificador, su plazo y su veredicto (10.10).
    fn tick_verifying(&self, session: &mut Session) {
        let Some((k, address, token, started)) = session.verification.as_ref().map(|pending| {
            (
                pending.k,
                pending.address.clone(),
                pending.token.clone(),
                pending.started_at,
            )
        }) else {
            return;
        };
        if self.now() - started > super::VERIFY_TIMEOUT_S {
            verify::finish_verification(self, session, Err(verify::timeout_error()));
            return;
        }
        let row = match self.port.get_agent(&address) {
            Ok(row) => row,
            Err(PortError::Herdr(HerdrError::NotFound(_))) => {
                self.pause(session, ez_agent::autonomous::texts::REASON_LOST);
                return;
            }
            Err(error) => {
                tracing::debug!("No pude leer al verificador {address}: {error}");
                return;
            }
        };
        let verifier_state = state_of(&row);
        match verifier_state.as_str() {
            "blocked" => self.blocked(session, &address),
            "done" | "idle" => {
                if session.dir.verdict_path(k).is_file() {
                    let result = session.dir.read_verdict(
                        k,
                        &VerdictSpec {
                            token: &token,
                            criteria: session.criteria.len(),
                        },
                    );
                    verify::finish_verification(self, session, result);
                } else if verifier_state == "done" {
                    // El proceso salió sin escribir el veredicto (10.10).
                    verify::finish_verification(self, session, Err(verify::failure_error()));
                }
            }
            "error" => {
                if let Some(reason) = self.screen_incident(&address) {
                    self.pause(session, &reason);
                } else {
                    verify::finish_verification(self, session, Err(verify::failure_error()));
                }
            }
            _ => {}
        }
    }

    fn poll_other(&self, session: &mut Session, address: &str) {
        let row = match self.port.get_agent(address) {
            Ok(row) => row,
            Err(_) => return,
        };
        match state_of(&row).as_str() {
            "blocked" => self.blocked(session, address),
            "error" => {
                if let Some(reason) = self.screen_incident(address) {
                    self.pause(session, &reason);
                }
            }
            _ => {}
        }
    }

    fn blocked(&self, session: &mut Session, address: &str) {
        match approve_blocked(self, session, address) {
            Approval::Approved | Approval::Ignored | Approval::Aborted => {}
            Approval::Paused(reason) => self.pause(session, &reason),
        }
    }

    /// Cuota, contexto lleno o sesión perdida en la pantalla (9.5).
    fn screen_incident(&self, address: &str) -> Option<String> {
        let screen = self.port.read_agent(address, Some(60)).ok()?;
        ez_agent::autonomous::screen_incident_reason(&screen).map(str::to_string)
    }

    /// Aplica una `Action` pura; el estado ya puede haber cambiado.
    pub(super) fn apply(&self, session: &mut Session, action: Action) {
        match action {
            Action::Wait => {}
            Action::Pause(reason) => {
                self.revoke();
                let verifiers = verifier_addresses(session);
                let text = if reason == ez_agent::autonomous::texts::REASON_APPROVALS {
                    ez_agent::autonomous::texts::paused_approvals(&reason)
                } else {
                    ez_agent::autonomous::texts::paused(&reason)
                };
                self.say(&text);
                self.log(
                    session,
                    ez_agent::autonomous::artifacts::Event::Pause,
                    serde_json::json!({"reason": reason}),
                );
                close_verifiers(self, session, verifiers);
            }
            Action::Resume => {
                self.say(ez_agent::autonomous::texts::RESUMED);
                self.log(
                    session,
                    ez_agent::autonomous::artifacts::Event::Resume,
                    serde_json::json!({}),
                );
            }
            Action::Done => verify::finish_task(self, session),
            Action::Stopped => {
                self.say(ez_agent::autonomous::texts::STOPPED);
                self.log(
                    session,
                    ez_agent::autonomous::artifacts::Event::Stop,
                    serde_json::json!({}),
                );
            }
            Action::Accepted => {
                self.say(ez_agent::autonomous::texts::ACCEPTED);
                self.log(
                    session,
                    ez_agent::autonomous::artifacts::Event::Done,
                    serde_json::json!({"accepted": true}),
                );
            }
            Action::LaunchVerifier => {
                let announce = session.announced_round != session.state.round;
                if let Err(reason) = verify::launch_verifier(self, session, announce) {
                    self.pause(session, &reason);
                }
            }
            Action::SendCorrection(path) => {
                if let Err(reason) = verify::send_correction(self, session, &path) {
                    self.pause(session, &reason);
                }
            }
            Action::LaunchExecutor => {
                if let Err(error) = launch_executor(self, session) {
                    self.pause(session, &error);
                }
            }
        }
    }
}

/// Los topes que, al reanudar, se prorrogan por omisión (decisión 1).
fn is_cap(reason: &str) -> bool {
    reason == ez_agent::autonomous::texts::REASON_ROUNDS
        || reason == ez_agent::autonomous::texts::REASON_DEADLINE
        || reason == ez_agent::autonomous::texts::REASON_APPROVALS
}

fn verifier_addresses(session: &Session) -> Vec<String> {
    session
        .state
        .scope
        .iter()
        .filter(|entry| entry.role == ez_agent::autonomous::ScopeRole::Verifier)
        .map(|entry| entry.address.clone())
        .collect()
}

fn close_verifiers(loop_: &AutonomousLoop, session: &mut Session, addresses: Vec<String>) {
    for address in addresses {
        if let Err(error) = loop_.port.kill_agent(&address, true) {
            tracing::debug!("No pude cerrar al verificador {address}: {error}");
        }
        session.state.forget(&address);
    }
    session.verification = None;
}

fn state_of(row: &serde_json::Map<String, serde_json::Value>) -> String {
    ez_core::pyvalue::py_strip(&ez_core::pyvalue::text_or_empty(row.get("state"))).to_lowercase()
}
