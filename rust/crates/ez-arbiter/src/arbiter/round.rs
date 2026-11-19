//! R3, R4, R8, R9 y R11: rondas, verificación, cierre y paradas.

use ez_team::Role;

use crate::criteria::{self, Criteria};
use crate::git;
use crate::paths;
use crate::state::{RoundRecord, Status, Task};
use crate::texts;
use crate::types::{Outcome, Rejection, Verdict};
use crate::verify::{self, CheckRecord};

use super::emit::Effects;
use super::Arbiter;

impl Arbiter {
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
        // R11: el elefante no cambió archivos del proyecto fuera de ronda.
        if let Some(previous) = &task.last_close {
            let current = git::snapshot(&task.space.folder)
                .map_err(|message| Rejection::new("R11", message))?;
            let changed = git::changed(&task.space.folder, previous, &current);
            if !changed.is_empty() {
                let message = texts::elephant_changed_outside_round(&changed);
                return Err(self.pause_task(task, "R11", "cambios_fuera_de_ronda", message));
            }
        }
        let round = task.round + 1;
        let start =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R4", message))?;
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
            .any(|fish| fish.role == Role::Qa && fish.report.is_some())
        {
            missing.push(format!("qa-r{round}"));
        }
        if !task
            .fish_in_round(round)
            .any(|fish| fish.role == Role::QaValidator && fish.report.is_some())
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
        close: git::GitSnapshot,
    ) {
        if let Some(record) = task.open_round_mut() {
            record.closed = Some(self.now());
            record.clean = clean;
            record.verification = checks.to_vec();
            record.reasons = reasons.to_vec();
            record.close = Some(close.clone());
        }
        task.last_close = Some(close);
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
        let stop = if task.round >= criteria.validacion.rondas_maximas {
            Some((
                "tope_rondas",
                texts::max_rounds(criteria.validacion.rondas_maximas),
            ))
        } else if same_error(task, checks) {
            Some(("mismo_error", texts::SAME_ERROR.to_string()))
        } else {
            let since = task.approved_at.unwrap_or_else(|| self.now());
            if self.now() - since > criteria.validacion.horas_maximas * 3600.0 {
                Some((
                    "tope_tiempo",
                    texts::max_hours(criteria.validacion.horas_maximas),
                ))
            } else {
                None
            }
        };
        if let Some((code, message)) = &stop {
            task.pause(code, message.clone());
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
    let qa_ok = task
        .fish_in_round(round)
        .find(|fish| fish.role == Role::Qa)
        .and_then(|fish| fish.report.as_ref())
        .is_some_and(|report| {
            report.verdict == Verdict::Pass && report.blocking == 0 && report.major == 0
        });
    if !qa_ok {
        reasons.push(texts::qa_not_clean());
    }
    let validator_ok = task
        .fish_in_round(round)
        .find(|fish| fish.role == Role::QaValidator)
        .and_then(|fish| fish.report.as_ref())
        .is_some_and(|report| {
            report.verdict == Verdict::Pass
                && report.mutations_tried.unwrap_or(0) >= criteria.validacion.mutaciones_minimas
                && report.mutations_caught == report.mutations_tried
        });
    if !validator_ok {
        reasons.push(texts::validator_not_clean());
    }
    let protected = task.protected_files(round);
    let touched: Vec<String> = task
        .rounds
        .iter()
        .find(|record| record.number == round)
        .map(|record| record.files_of(Role::Developer))
        .unwrap_or_default()
        .intersection(&protected)
        .cloned()
        .collect();
    if !touched.is_empty() {
        reasons.push(texts::protected_touched(&touched));
    }
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
    for role in [Role::Planner, Role::PlanReviewer, Role::QaValidator] {
        let changed: Vec<String> = task
            .rounds
            .iter()
            .find(|record| record.number == round)
            .map(|record| record.files_of(role).into_iter().collect())
            .unwrap_or_default();
        if !changed.is_empty() {
            reasons.push(texts::role_changed_files(role.id(), &changed));
        }
    }
    (reasons.is_empty(), reasons)
}
