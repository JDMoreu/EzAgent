//! Fases de la ronda y orden de los papeles (diseño §14.3).

use ez_team::Role;

use crate::git;
use crate::state::WorkState;
use crate::texts;
use crate::types::Rejection;

use super::ArbiterShared;

impl ArbiterShared {
    /// Fases de la ronda y límite de peces vivos (§14.3).
    pub(crate) fn check_order(
        &self,
        task: &crate::state::Task,
        role: Role,
        round: u32,
        count: u32,
    ) -> Result<(), Rejection> {
        if round > 1 && matches!(role, Role::Planner | Role::PlanReviewer) {
            return Err(Rejection::new("R5", texts::PLAN_ONLY_ROUND1));
        }
        if role == Role::Developer
            && task
                .fish_in_round(round)
                .any(|fish| fish.role == Role::Qa && task.work_for_key(&fish.key).is_some())
        {
            return Err(Rejection::new("R5", texts::QA_STARTED));
        }
        let live = (role.is_verifier() || role == Role::Planner)
            && task
                .fish_in_round(round)
                .filter(|fish| fish.role == role)
                .any(|fish| {
                    task.work_for_key(&fish.key)
                        .is_some_and(|item| item.state != WorkState::Failed)
                });
        if live {
            return Err(Rejection::new("R5", texts::already_in_round(role.id())));
        }
        match role {
            Role::PlanReviewer => {
                if !self.role_passed(task, Role::Planner, round) {
                    return Err(Rejection::new(
                        "R5",
                        texts::waiting_for(&[format!("plan-r{round}")]),
                    ));
                }
            }
            Role::Developer => self.check_developer_order(task, round, count)?,
            Role::Qa => self.check_qa_order(task, round)?,
            Role::QaValidator => {
                let reported = task
                    .fish_in_round(round)
                    .filter(|fish| fish.role == Role::Qa)
                    .any(|fish| fish.report.is_some());
                if !reported {
                    return Err(Rejection::new(
                        "R5",
                        texts::waiting_for(&[format!("qa-r{round}")]),
                    ));
                }
            }
            Role::Planner => {}
        }
        Ok(())
    }

    /// El desarrollador necesita el plan aprobado en la ronda 1.
    fn check_developer_order(
        &self,
        task: &crate::state::Task,
        round: u32,
        count: u32,
    ) -> Result<(), Rejection> {
        if round == 1 && !self.role_passed(task, Role::Planner, round) {
            return Err(Rejection::new(
                "R5",
                texts::waiting_for(&[format!("plan-r{round}")]),
            ));
        }
        if round == 1
            && task.seat(Role::PlanReviewer).is_some()
            && !self.role_passed(task, Role::PlanReviewer, round)
        {
            return Err(Rejection::new(
                "R5",
                texts::waiting_for(&[format!("revplan-r{round}")]),
            ));
        }
        let alive = task
            .fish_in_round(round)
            .filter(|fish| fish.role == Role::Developer && self.launcher.alive(&fish.terminal))
            .count() as u32;
        if alive >= count.max(1) {
            return Err(Rejection::new(
                "R5",
                texts::too_many_developers(alive, count.max(1)),
            ));
        }
        Ok(())
    }

    /// ¿Informó `pass` ese papel en esta ronda?
    fn role_passed(&self, task: &crate::state::Task, role: Role, round: u32) -> bool {
        task.fish_in_round(round)
            .filter(|fish| fish.role == role)
            .any(|fish| {
                fish.report
                    .as_ref()
                    .is_some_and(|report| report.verdict == crate::types::Verdict::Pass)
            })
    }

    /// El QA espera a los desarrolladores, salvo en una ronda solo de verificación.
    fn check_qa_order(&self, task: &crate::state::Task, round: u32) -> Result<(), Rejection> {
        let developers: Vec<_> = task
            .fish_in_round(round)
            .filter(|fish| fish.role == Role::Developer)
            .collect();
        if developers.is_empty() && round == 1 {
            return Err(Rejection::new(
                "R5",
                texts::waiting_for(&[format!("dev-r{round}-1")]),
            ));
        }
        if !developers.is_empty() {
            let waiting: Vec<String> = developers
                .iter()
                .filter(|fish| task.pending_work(&fish.key).is_some())
                .map(|fish| fish.id().to_string())
                .collect();
            if !waiting.is_empty() {
                return Err(Rejection::new("R5", texts::waiting_for(&waiting)));
            }
            if !developers.iter().any(|fish| fish.report.is_some()) {
                return Err(Rejection::new(
                    "R5",
                    texts::waiting_for(&[format!("dev-r{round}-1")]),
                ));
            }
            return Ok(());
        }
        // La ronda solo de verificación exige la huella del cierre limpio anterior.
        let previous = task
            .rounds
            .iter()
            .find(|record| record.number + 1 == round && record.closed.is_some());
        let verification_only = previous.is_some_and(|record| {
            record.clean
                && git::snapshot(&task.space.folder)
                    .map(|now| now == record.close.clone().unwrap_or_default())
                    .unwrap_or(false)
        });
        if !verification_only {
            return Err(Rejection::new("R5", texts::no_verification_round()));
        }
        Ok(())
    }
}
