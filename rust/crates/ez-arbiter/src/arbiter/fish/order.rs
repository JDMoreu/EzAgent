//! Orden por etapas de la ronda (diseño §15.4).

use ez_team::{builtin, RoleId};

use crate::git;
use crate::state::WorkState;
use crate::texts;
use crate::types::{Rejection, Verdict};

use super::ArbiterShared;

impl ArbiterShared {
    /// Un trabajo de etapa *k* empieza cuando informaron todos los de etapas
    /// menores en la ronda; además se respetan los límites de papel y puestos.
    pub(crate) fn check_order(
        &self,
        task: &crate::state::Task,
        role: &RoleId,
        round: u32,
        count: u32,
    ) -> Result<(), Rejection> {
        let Some(def) = task.role_def(role) else {
            return Err(Rejection::new("R5", texts::role_not_in_team(role.as_str())));
        };
        if round > 1 && def.first_round_only {
            return Err(Rejection::new("R5", texts::PLAN_ONLY_ROUND1));
        }
        let single = def.is_verifier() || def.first_round_only;
        if single
            && task.fish_in_round(round).any(|fish| {
                fish.role == *role
                    && task
                        .work_for_key(&fish.key)
                        .is_some_and(|item| item.state != WorkState::Failed)
            })
        {
            return Err(Rejection::new("R5", texts::already_in_round(role.as_str())));
        }
        let stage = task.stage_of(role);
        let waiting = self.lower_stage_waiting(task, round, stage);
        if !waiting.is_empty() {
            return Err(Rejection::new("R5", texts::waiting_for(&waiting)));
        }
        let first_verifier_stage = task
            .team
            .iter()
            .filter(|seat| task.is_verifier(&seat.role))
            .filter(|seat| {
                task.role_def(&seat.role)
                    .is_some_and(|def| !def.first_round_only)
            })
            .map(|seat| seat.stage)
            .min()
            .unwrap_or(stage);
        if stage < first_verifier_stage
            && task.fish_in_round(round).any(|fish| {
                task.is_verifier(&fish.role)
                    && task
                        .role_def(&fish.role)
                        .is_some_and(|def| !def.first_round_only)
            })
        {
            return Err(Rejection::new("R5", texts::QA_STARTED));
        }
        if def.is_verifier() {
            self.check_verifier_entry(task, round, stage, first_verifier_stage)?;
        }
        self.check_alive_count(task, role, round, count)
    }

    /// Respeta el número de puestos simultáneos del papel (§15.3).
    fn check_alive_count(
        &self,
        task: &crate::state::Task,
        role: &RoleId,
        round: u32,
        count: u32,
    ) -> Result<(), Rejection> {
        if count <= 1 && role.as_str() != builtin::DEVELOPER {
            return Ok(());
        }
        let alive = task
            .fish_in_round(round)
            .filter(|fish| fish.role == *role && self.launcher.alive(&fish.terminal))
            .count() as u32;
        if alive >= count.max(1) {
            return Err(Rejection::new(
                "R5",
                texts::too_many_fish(alive, count.max(1)),
            ));
        }
        Ok(())
    }

    /// Trabajos de etapas menores que aún no informaron (o que deben pasar en la ronda 1).
    fn lower_stage_waiting(
        &self,
        task: &crate::state::Task,
        round: u32,
        stage: u32,
    ) -> Vec<String> {
        let mut waiting: Vec<String> = task
            .work
            .iter()
            .filter(|item| item.round == round && item.state == WorkState::Pending)
            .filter(|item| {
                task.fish_by_key(&item.key).is_some_and(|fish| {
                    task.stage_of(&fish.role) < stage
                        && (round == 1
                            || task
                                .role_def(&fish.role)
                                .is_some_and(|def| !def.first_round_only))
                })
            })
            .map(|item| item.key.fish.clone())
            .collect();
        for seat in task.team.iter().filter(|seat| {
            seat.stage < stage
                && task.is_verifier(&seat.role)
                && (round == 1
                    || task
                        .role_def(&seat.role)
                        .is_some_and(|def| !def.first_round_only))
        }) {
            let reported = task
                .fish_in_round(round)
                .any(|fish| fish.role == seat.role && fish.report.is_some());
            if !reported {
                waiting.push(format!("{}-r{round}", seat.role.short()));
            }
        }
        if round == 1 {
            for seat in task
                .team
                .iter()
                .filter(|seat| seat.stage < stage)
                .filter(|seat| {
                    task.role_def(&seat.role)
                        .is_some_and(|def| def.first_round_only)
                })
            {
                let passed = task.fish_in_round(round).any(|fish| {
                    fish.role == seat.role
                        && fish
                            .report
                            .as_ref()
                            .is_some_and(|report| report.verdict == Verdict::Pass)
                });
                if !passed && !waiting.contains(&seat.role.as_str().to_string()) {
                    let id = format!("{}-r{round}", seat.role.short());
                    waiting.push(id);
                }
            }
        }
        waiting
    }

    /// El primer verificador espera a los ejecutores; en rondas sin ejecutores
    /// solo se permite una ronda de verificación tras una ronda limpia.
    fn check_verifier_entry(
        &self,
        task: &crate::state::Task,
        round: u32,
        stage: u32,
        first_verifier_stage: u32,
    ) -> Result<(), Rejection> {
        if stage != first_verifier_stage {
            return Ok(());
        }
        let executors: Vec<&ez_team::Seat> = task
            .team
            .iter()
            .filter(|seat| !task.is_verifier(&seat.role))
            .collect();
        let has_executor_work = task
            .fish_in_round(round)
            .any(|fish| !task.is_verifier(&fish.role));
        if round > 1 && !has_executor_work {
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
            return if verification_only {
                Ok(())
            } else {
                Err(Rejection::new("R5", texts::no_verification_round()))
            };
        }
        if round > 1 {
            return Ok(());
        }
        let main_stage = executors.iter().map(|seat| seat.stage).max().unwrap_or(0);
        for seat in executors.iter().filter(|seat| seat.stage == main_stage) {
            let launched = task.fish_in_round(round).any(|fish| fish.role == seat.role);
            if !launched {
                let index = if seat.count > 1 || seat.role.as_str() == builtin::DEVELOPER {
                    "-1"
                } else {
                    ""
                };
                let expected = format!("{}-r{round}{index}", seat.role.short());
                return Err(Rejection::new("R5", texts::waiting_for(&[expected])));
            }
        }
        Ok(())
    }
}
