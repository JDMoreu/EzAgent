//! §15.5: delegación entre subagentes. Un pez con un papel que puede delegar
//! lanza hijos de su `delega_en`, dentro de la `cantidad` del equipo y sin
//! superar `profundidad_maxima`.

use std::path::PathBuf;

use ez_team::RoleId;

use crate::paths;
use crate::state::Status;
use crate::texts;
use crate::types::{FishKey, Outcome, Rejection};

use super::fish::{attempt_suffix, LaunchPlan};
use super::{new_token, ArbiterShared};

impl ArbiterShared {
    /// Un subagente delega un papel permitido en uno de sus propios hijos.
    pub(crate) fn agent_delegate(
        &self,
        parent_key: &FishKey,
        role: RoleId,
        brief: PathBuf,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_fish_key(parent_key)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R5", texts::NO_ROUND));
        }
        let parent = task
            .fish_by_key(parent_key)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::FISH_NOT_ACTIVE))?;
        let parent_def = task
            .role_def(&parent.role)
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(parent.role.as_str())))?;
        if !parent_def.can_delegate || parent_def.is_verifier() {
            return Err(Rejection::new(
                "R5",
                texts::cannot_delegate(parent.role.as_str()),
            ));
        }
        let parent_seat = task
            .seat(&parent.role)
            .cloned()
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(parent.role.as_str())))?;
        if !parent_seat.delegates_to.contains(&role) {
            return Err(Rejection::new(
                "R5",
                texts::role_not_delegable(role.as_str()),
            ));
        }
        task.role_def(&role)
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(role.as_str())))?;
        let depth = parent.depth + 1;
        let max_depth = self.load_criteria(&task)?.validacion.profundidad_maxima;
        if depth > max_depth {
            return Err(Rejection::new("R5", texts::too_deep(depth, max_depth)));
        }
        self.check_child_count(&task, &role)?;
        let plan = self.prepare_child(&task, &role, &parent, parent_key, brief, depth)?;
        self.deliver(&mut task, plan, depth)
    }

    /// El papel delegado respeta la cantidad de puestos del equipo.
    fn check_child_count(&self, task: &crate::state::Task, role: &RoleId) -> Result<(), Rejection> {
        let seat = task
            .seat(role)
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(role.as_str())))?;
        let alive = task
            .fish_in_round(task.round)
            .filter(|fish| fish.role == *role && self.launcher.alive(&fish.terminal))
            .count() as u32;
        if alive >= seat.count.max(1) {
            return Err(Rejection::new(
                "R5",
                texts::too_many_fish(alive, seat.count.max(1)),
            ));
        }
        Ok(())
    }

    /// Identidad, archivos y petición de lanzamiento del hijo.
    fn prepare_child(
        &self,
        task: &crate::state::Task,
        role: &RoleId,
        parent: &crate::state::FishRecord,
        parent_key: &FishKey,
        brief: PathBuf,
        _depth: u32,
    ) -> Result<LaunchPlan, Rejection> {
        let round = task.round;
        let attempt = task.attempts(role, round) + 1;
        // DECISION: el id del hijo lleva el id del padre para que no choque con
        // el pez del mismo papel lanzado por el agente principal en la ronda.
        let fish = format!("{}-{}", parent_key.fish, role.short());
        let key = FishKey {
            space: task.space.id.clone(),
            task: task.id.clone(),
            elephant: task.elephant.elephant.clone(),
            fish: fish.clone(),
            attempt,
            parent: Some(parent_key.fish.clone()),
        };
        let suffix = attempt_suffix(attempt);
        let folder = paths::round_dir(&task.space.folder, &task.id, round);
        let fish_brief = folder.join(format!("{fish}{suffix}.encargo.md"));
        let instructions = folder.join(format!("{fish}{suffix}.instrucciones.md"));
        let report_path = folder.join(format!("{fish}{suffix}.informe.md"));
        let start = self.copy_fish_brief(task, &brief, &fish_brief)?;
        let seat = task
            .seat(role)
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(role.as_str())))?
            .clone();
        let context = self.instruction_context(
            task,
            role,
            &seat,
            &key,
            round,
            Some(parent_key.fish.as_str()),
        )?;
        self.write_instructions(&context, &instructions, &fish)?;
        let token = new_token();
        let request = self.launch_request(task, role, &seat, &key, &instructions, &token);
        // El hijo hereda la etapa del padre: no sigue el orden de etapas (§15.5).
        let mut request = request;
        request.seat.stage = task.stage_of(&parent.role);
        Ok(LaunchPlan {
            key,
            fish,
            role: role.clone(),
            report_path,
            start,
            request,
        })
    }
}
