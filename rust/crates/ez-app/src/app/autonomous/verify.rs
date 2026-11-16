//! Verificación, corrección y final con evidencia (9.5, 10.10, 10.12 y 10.16).

use ez_agent::autonomous::artifacts::{CriterionEvidence, Event, SummaryData};
use ez_agent::autonomous::texts;
use ez_agent::autonomous::{Verdict, VerdictError};
use serde_json::json;

use super::host;
use super::host::start;
use super::host::{address_of, error_of, ok_of};
use super::{approve, AutonomousLoop, PendingVerification, Session};

/// Lanza un verificador nuevo con su carpeta y su nonce (10.10).
pub fn launch_verifier(
    loop_: &AutonomousLoop,
    session: &mut Session,
    announce: bool,
) -> Result<(), String> {
    let k = session.state.next_verification();
    let verification = session.dir.prepare_verification(k)?;
    let dir = std::fs::canonicalize(&verification.dir).unwrap_or(verification.dir);
    let verdict_path = dir.join("veredicto.json").to_string_lossy().to_string();
    let encargo = session.encargo_path().to_string_lossy().to_string();
    let criterios = session.criteria_path().to_string_lossy().to_string();
    let prompt = texts::verifier_prompt(&encargo, &criterios, &verdict_path, &verification.token);
    let agent = verifier_agent(session);
    let result = start::run(loop_.port.as_ref(), session, &agent, &prompt)?;
    let Some(address) = address_of(&result) else {
        return Err(texts::launch_failed(
            "el verificador",
            &format!("Herdr no devolvió la terminal: {}", error_of(&result)),
        ));
    };
    let edit_dirs = vec![dir.to_string_lossy().to_string()];
    session.state.verifier_started(&address, &edit_dirs);
    if !ok_of(&result) {
        let trusted = approve::approve_trust(loop_, session, &address);
        let sent = trusted && start::send(loop_.port.as_ref(), &address, &prompt)?;
        if !sent {
            return Err(texts::launch_failed("el verificador", &error_of(&result)));
        }
    }
    session.verification = Some(PendingVerification {
        k,
        address: address.clone(),
        token: verification.token,
        started_at: loop_.now(),
    });
    loop_.log(
        session,
        Event::VerifyStart,
        json!({"verification": k, "address": address, "round": session.state.round}),
    );
    if announce {
        loop_.say(&texts::round(session.state.round));
        session.announced_round = session.state.round;
    }
    Ok(())
}

fn verifier_agent(session: &Session) -> String {
    if session.verifier_agent.is_empty() {
        session.executor_agent.clone()
    } else {
        session.verifier_agent.clone()
    }
}

/// Cierra al verificador, guarda copia del veredicto y pasa la transición.
pub fn finish_verification(
    loop_: &AutonomousLoop,
    session: &mut Session,
    result: Result<Verdict, VerdictError>,
) {
    let Some(pending) = session.verification.take() else {
        return;
    };
    if let Err(error) = loop_.port.kill_agent(&pending.address, true) {
        tracing::debug!("No pude cerrar al verificador {}: {error}", pending.address);
    }
    session.state.forget(&pending.address);
    if session.dir.verdict_path(pending.k).is_file() {
        if let Err(error) = session.dir.store_verdict_copy(pending.k) {
            tracing::warn!("No pude guardar copia del veredicto: {error}");
        }
    }
    let clean = result.as_ref().is_ok_and(|verdict| verdict.clean());
    if let Ok(verdict) = &result {
        session.last_verdict = Some(verdict.clone());
        if verdict.clean() {
            session.last_clean = Some(verdict.clone());
        }
    }
    loop_.log(
        session,
        Event::Verdict,
        json!({"verification": pending.k, "clean": clean, "round": session.state.round}),
    );
    let action = session
        .state
        .on_verdict(result, &session.dir.shared(), loop_.now());
    loop_.apply(session, action);
}

/// 10.10 y 10.12: escribe `correccion-N.md` y manda su ruta al ejecutor.
pub fn send_correction(
    loop_: &AutonomousLoop,
    session: &mut Session,
    path: &str,
) -> Result<(), String> {
    let failed = (session.state.round - 1).max(1);
    if let Some(verdict) = session.last_verdict.take() {
        session.dir.write_correction(failed, &verdict)?;
    }
    let fin = session.dir.fin_path(session.state.round);
    let text = texts::correction(path, &fin.to_string_lossy());
    let Some(executor) = session.state.executor.as_ref() else {
        return Err(texts::REASON_LOST.to_string());
    };
    let address = executor.address.clone();
    if !start::send(loop_.port.as_ref(), &address, &text)? {
        return Err(texts::send_failed(
            "la corrección",
            "Herdr no confirmó el envío",
        ));
    }
    session.sent_at = loop_.now();
    session.idle_since = None;
    loop_.log(
        session,
        Event::Correction,
        json!({"round": failed, "path": path}),
    );
    Ok(())
}

/// `privado/resumen.md` con la evidencia del último veredicto limpio (10.16).
pub fn finish_task(loop_: &AutonomousLoop, session: &mut Session) {
    let base = session.state.base_commit.clone().unwrap_or_default();
    let (diff, log) = if base.is_empty() {
        (String::new(), String::new())
    } else {
        (
            host::diff_stat(&session.state.workdir, &base),
            host::log_oneline(&session.state.workdir, &base),
        )
    };
    let data = summary_data(session, &diff, &log);
    match session.dir.write_summary(&data) {
        Ok(path) => {
            loop_.say(&texts::done_with_summary(
                &session.task,
                &path.to_string_lossy(),
            ));
        }
        Err(error) => {
            tracing::warn!("No pude escribir el resumen final: {error}");
            loop_.say(&texts::done(&session.task, session.state.approvals));
        }
    }
    loop_.log(
        session,
        Event::Done,
        json!({"approvals": session.state.approvals, "round": session.state.round}),
    );
}

fn summary_data(session: &Session, diff: &str, log: &str) -> SummaryData {
    match &session.last_clean {
        Some(verdict) => SummaryData::from_verdict(
            &session.task,
            &session.state.workdir,
            session.state.approvals,
            &session.criteria,
            verdict,
            diff,
            log,
        ),
        None => SummaryData {
            task: session.task.clone(),
            workdir: session.state.workdir.clone(),
            approvals: session.state.approvals,
            criteria: session
                .criteria
                .iter()
                .map(|criterion| CriterionEvidence {
                    text: criterion.text.clone(),
                    evidence: String::new(),
                })
                .collect(),
            diff_stat: diff.to_string(),
            git_log: log.to_string(),
        },
    }
}

/// El verificador no escribió a tiempo (10.10).
pub fn timeout_error() -> VerdictError {
    VerdictError("el verificador no escribió su veredicto a tiempo".to_string())
}

/// El verificador falló sin dejar veredicto utilizable.
pub fn failure_error() -> VerdictError {
    VerdictError(texts::REASON_VERIFIER.to_string())
}
