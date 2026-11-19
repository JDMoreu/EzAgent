//! R3, R4, R8, R9 y R11: rondas, verificación, cierre y paradas.

use ez_team::builtin;

use crate::criteria::{self, Criteria};
use crate::git;
use crate::paths;
use crate::state::{RoundRecord, Status, Task};
use crate::texts;
use crate::types::{Outcome, Rejection, Verdict};
use crate::verify::{self, CheckRecord};

use super::emit::Effects;
use super::ArbiterShared;

impl ArbiterShared {
    /// R4: empieza una ronda y toma la foto inicial de git.
    pub(crate) fn round_start(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status == Status::Paused {
            return Err(Rejection::new("R4", texts::paused(&self.pause_text(&task))));
        }
        if task.status == Status::InRound {
            return Err(Rejection::new("R4", texts::ROUND_OPEN));
        }
        if !matches!(task.status, Status::Approved | Status::BetweenRounds) {
            return Err(Rejection::new("R4", texts::NOT_APPROVED));
        }
        if task.team_status != crate::state::TeamStatus::Approved {
            return Err(Rejection::new("R4", texts::TEAM_NOT_APPROVED));
        }
        if let Err(rejection) = self.check_frozen(&task) {
            return Err(self.pause_task(task, "R3", "criterios_modificados", rejection.message));
        }
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let round = task.round + 1;
        let start =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R4", message))?;
        task.last_fp = Some(start.clone());
        task.round = round;
        task.rounds.push(RoundRecord {
            number: round,
            opened: self.now(),
            closed: None,
            clean: false,
            start: start.clone(),
            close: None,
            verification: Vec::new(),
            reasons: Vec::new(),
            attribution: Default::default(),
        });
        task.status = Status::InRound;
        let message = texts::round_started(round);
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "round_started",
            serde_json::json!({ "round": round }),
        ));
        self.commit(&task, effects);
        Ok(Outcome {
            message,
            data: serde_json::json!({ "round": round }),
        })
    }

    /// R8: ejecuta los criterios y guarda `verificacion.json` en la ronda.
    pub(crate) fn verify(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R8", texts::NO_ROUND));
        }
        if let Err(rejection) = self.check_frozen(&task) {
            return Err(self.pause_task(task, "R3", "criterios_modificados", rejection.message));
        }
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let criteria = self.load_criteria(&task)?;
        let checks = verify::run_all(&task.space.folder, &criteria);
        self.write_verification(&task, &checks)?;
        let failed = checks.iter().filter(|check| !check.ok).count();
        if let Some(record) = task.open_round_mut() {
            record.verification = checks.clone();
        }
        let message = if failed == 0 {
            texts::VERIFY_OK.to_string()
        } else {
            texts::verify_failed(failed, checks.len())
        };
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "verification",
            serde_json::json!({ "ok": failed == 0, "failed": failed, "total": checks.len() }),
        ));
        self.commit(&task, effects);
        Ok(Outcome {
            message,
            data: serde_json::json!({ "ok": failed == 0, "criterios": checks }),
        })
    }

    /// R9: cierra la ronda, exige informes y aplica las paradas.
    pub(crate) fn round_close(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R9", texts::NO_ROUND));
        }
        if let Err(rejection) = self.check_frozen(&task) {
            return Err(self.pause_task(task, "R3", "criterios_modificados", rejection.message));
        }
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let round = task.round;
        let pending: Vec<String> = task
            .pending_in_round(round)
            .iter()
            .map(|item| item.key.fish.clone())
            .collect();
        if !pending.is_empty() {
            return Err(Rejection::new("R9", texts::pending_work(&pending)));
        }
        self.require_reports(&task, round)?;
        let criteria = self.load_criteria(&task)?;
        let checks = verify::run_all(&task.space.folder, &criteria);
        self.write_verification(&task, &checks)?;
        let (clean, reasons) = evaluate(&task, round, &criteria, &checks);
        let close =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R9", message))?;
        self.record_close(&mut task, clean, &reasons, &checks, close);
        let mut message = if clean {
            texts::round_clean(round)
        } else {
            texts::round_dirty(round, &reasons)
        };
        // DECISION: la ronda ya cerró; las paradas posteriores se informan en
        // el mensaje con Ok, no se devuelven como rechazo del comando.
        let paused = self.apply_stop_conditions(&mut task, &criteria, &checks);
        if let Some((_, text)) = &paused {
            message.push(' ');
            message.push_str(text);
        }
        let effects = self.close_effects(&task, round, clean, &reasons, &checks, paused);
        self.commit(&task, effects);
        Ok(Outcome {
            message,
            data: serde_json::json!({ "round": round, "clean": clean }),
        })
    }

    /// El cierre exige los informes del QA y del validador de QA.
    fn require_reports(&self, task: &Task, round: u32) -> Result<(), Rejection> {
        let mut missing = Vec::new();
        if !task
            .fish_in_round(round)
            .any(|fish| fish.role.as_str() == builtin::QA && fish.report.is_some())
        {
            missing.push(format!("qa-r{round}"));
        }
        if !task
            .fish_in_round(round)
            .any(|fish| fish.role.as_str() == builtin::QA_VALIDATOR && fish.report.is_some())
        {
            missing.push(format!("valqa-r{round}"));
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Rejection::new("R9", texts::close_needs(&missing)))
        }
    }

    /// Cierra el registro de la ronda y actualiza la racha y el estado.
    fn record_close(
        &self,
        task: &mut Task,
        clean: bool,
        reasons: &[String],
        checks: &[CheckRecord],
        close: git::Fingerprint,
    ) {
        if let Some(record) = task.open_round_mut() {
            record.closed = Some(self.now());
            record.clean = clean;
            record.verification = checks.to_vec();
            record.reasons = reasons.to_vec();
            record.close = Some(close.clone());
        }
        task.last_close = Some(close.clone());
        task.last_fp = Some(close.clone());
        if clean {
            task.last_clean_close = Some(close);
        }
        task.clean_streak = if clean {
            task.clean_streak.saturating_add(1)
        } else {
            0
        };
        task.status = Status::BetweenRounds;
    }

    /// Eventos del cierre: verificación, ronda y pausa si corresponde.
    fn close_effects(
        &self,
        task: &Task,
        round: u32,
        clean: bool,
        reasons: &[String],
        checks: &[CheckRecord],
        paused: Option<(&'static str, String)>,
    ) -> Effects {
        let mut effects = Effects::default();
        effects.event(self.event(
            task,
            "verification",
            serde_json::json!({ "ok": checks.iter().all(|check| check.ok), "criterios": checks }),
        ));
        effects.event(self.event(
            task,
            "round_closed",
            serde_json::json!({ "round": round, "clean": clean, "reasons": reasons }),
        ));
        if let Some((code, text)) = &paused {
            effects.both(
                self.event(
                    task,
                    "paused",
                    serde_json::json!({ "reason": code, "message": text }),
                ),
                self.elephant_event(
                    task,
                    "paused",
                    text.clone(),
                    serde_json::json!({ "reason": code, "message": text }),
                ),
            );
        }
        effects
    }

    /// Lee los criterios congelados que se están usando.
    pub(crate) fn load_criteria(&self, task: &Task) -> Result<Criteria, Rejection> {
        let path = paths::task_dir(&task.space.folder, &task.id).join("criterios.toml");
        criteria::load(&path, &task.space.folder).map_err(|message| Rejection::new("R3", message))
    }

    fn write_verification(&self, task: &Task, checks: &[CheckRecord]) -> Result<(), Rejection> {
        let path =
            paths::round_dir(&task.space.folder, &task.id, task.round).join("verificacion.json");
        let json = serde_json::json!({
            "ok": checks.iter().all(|check| check.ok),
            "criterios": checks,
        });
        let bytes = serde_json::to_vec_pretty(&json).map_err(|error| {
            Rejection::new(
                "R8",
                format!("No se pudo serializar la verificación: {error}."),
            )
        })?;
        paths::write_atomic(&path, &bytes).map_err(|error| {
            Rejection::new(
                "R8",
                format!("No se pudo guardar la verificación: {error}."),
            )
        })
    }

    /// Aplica el orden de paradas del diseño y devuelve `(código, texto)`.
    fn apply_stop_conditions(
        &self,
        task: &mut Task,
        criteria: &Criteria,
        checks: &[CheckRecord],
    ) -> Option<(&'static str, String)> {
        let max_rounds = criteria.validacion.rondas_maximas + task.extra_rounds;
        let max_hours = criteria.validacion.horas_maximas + task.extra_hours;
        let stop = if task.round >= max_rounds {
            Some(("tope_rondas", texts::max_rounds(max_rounds)))
        } else if same_error(task, checks) {
            Some(("mismo_error", texts::SAME_ERROR.to_string()))
        } else {
            let now = self.now();
            let since = task.approved_at.unwrap_or(now);
            let paused_now = task
                .paused_at
                .map(|started| (now - started).max(0.0))
                .unwrap_or(0.0);
            let active = (now - since - task.paused_total - paused_now).max(0.0);
            if active > max_hours * 3600.0 {
                Some(("tope_tiempo", texts::max_hours(max_hours)))
            } else {
                None
            }
        };
        if let Some((code, message)) = &stop {
            task.pause(code, message.clone(), self.now());
        }
        stop
    }
}

/// ¿Repiten los mismos criterios fallidos y salidas que la ronda anterior?
fn same_error(task: &Task, checks: &[CheckRecord]) -> bool {
    let previous = task
        .rounds
        .iter()
        .find(|record| record.number + 1 == task.round && record.closed.is_some());
    let Some(previous) = previous else {
        return false;
    };
    let failing = |list: &[CheckRecord]| -> Vec<(String, String)> {
        list.iter()
            .filter(|check| !check.ok)
            .map(|check| (check.id.clone(), check.salida.clone()))
            .collect()
    };
    let now = failing(checks);
    !now.is_empty() && now == failing(&previous.verification)
}

/// Ronda limpia: criterios, QA, validador y archivos protegidos.
fn evaluate(
    task: &Task,
    round: u32,
    criteria: &Criteria,
    checks: &[CheckRecord],
) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();
    for check in checks.iter().filter(|check| !check.ok) {
        let reason = check.motivo.clone().unwrap_or_default();
        reasons.push(texts::criterion_failed(&check.id, &reason));
    }
    let verifiers: Vec<&ez_team::RoleDef> =
        task.roles.iter().filter(|def| def.is_verifier()).collect();
    let any_reported = task
        .fish_in_round(round)
        .any(|fish| verifiers.iter().any(|def| def.id == fish.role) && fish.report.is_some());
    if !any_reported {
        reasons.push(texts::no_verifier_reported());
    }
    verifier_reasons(task, round, criteria, verifiers, &mut reasons);
    let protected = task.protected_files(round);
    let touched: Vec<String> = task
        .rounds
        .iter()
        .find(|record| record.number == round)
        .map(|record| {
            task.roles
                .iter()
                .filter(|def| !def.is_verifier())
                .flat_map(|def| record.files_of(&def.id).into_iter())
                .collect::<std::collections::BTreeSet<String>>()
        })
        .unwrap_or_default()
        .intersection(&protected)
        .cloned()
        .collect();
    if !touched.is_empty() {
        reasons.push(texts::protected_touched(&touched));
    }
    other_violations(task, round, &mut reasons);
    (reasons.is_empty(), reasons)
}

/// Motivos de cada verificador: informe limpio y mutaciones (§15.4).
fn verifier_reasons(
    task: &Task,
    round: u32,
    criteria: &Criteria,
    verifiers: Vec<&ez_team::RoleDef>,
    reasons: &mut Vec<String>,
) {
    for def in verifiers {
        let report = task
            .fish_in_round(round)
            .find(|fish| fish.role == def.id)
            .and_then(|fish| fish.report.as_ref());
        let clean = report.is_some_and(|report| {
            report.verdict == Verdict::Pass && report.blocking == 0 && report.major == 0
        });
        if def.writes_tests && !clean {
            reasons.push(texts::qa_not_clean());
        }
        if def.mutations {
            let mutations_ok = report.is_some_and(|report| {
                clean
                    && report.mutations_tried.unwrap_or(0) >= criteria.validacion.mutaciones_minimas
                    && report.mutations_caught == report.mutations_tried
            });
            if !mutations_ok {
                reasons.push(texts::validator_not_clean());
            }
        }
    }
}

/// Informes sucios y papeles que no debían tocar el proyecto (§14.3, §14.4).
fn other_violations(task: &Task, round: u32, reasons: &mut Vec<String>) {
    let bad: Vec<String> = task
        .fish_in_round(round)
        .filter_map(|fish| fish.report.as_ref().map(|report| (fish, report)))
        .filter(|(_, report)| {
            report.verdict != Verdict::Pass || report.blocking > 0 || report.major > 0
        })
        .map(|(fish, _)| fish.id().to_string())
        .collect();
    if !bad.is_empty() {
        reasons.push(texts::reports_not_clean(&bad));
    }
    for def in task
        .roles
        .iter()
        .filter(|def| def.is_verifier() && !def.writes_tests)
    {
        let changed: Vec<String> = task
            .rounds
            .iter()
            .find(|record| record.number == round)
            .map(|record| record.files_of(&def.id).into_iter().collect())
            .unwrap_or_default();
        if !changed.is_empty() {
            reasons.push(texts::role_changed_files(def.id.as_str(), &changed));
        }
    }
}
