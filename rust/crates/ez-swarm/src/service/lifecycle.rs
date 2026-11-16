//! Creación de enjambres y tareas del modo autónomo (F3-12 y 10.18).

use tracing::info;

use crate::db::DbResult;
use crate::models::{SwarmError, SYSTEM};

use super::{stages_for, MemberSpec, Seat, SwarmService, MAX_TASKS};

impl SwarmService {
    /// Creates a swarm. `tasks` are `(title, role)` and chain in order.
    pub fn start(
        &self,
        goal: &str,
        members: &[MemberSpec],
        cwd: &str,
        tasks: Option<&[(String, String)]>,
    ) -> DbResult<(String, Vec<Seat>)> {
        self.create(goal, members, cwd, tasks, &|_| {})
    }

    /// Creates a swarm for the autonomous mode (10.18).
    ///
    /// `before_active` runs with the id before the swarm is visible as active,
    /// so the first launch of its members already knows it is autonomous.
    pub fn start_autonomous(
        &self,
        goal: &str,
        members: &[MemberSpec],
        cwd: &str,
        before_active: &dyn Fn(&str),
    ) -> DbResult<String> {
        let (swarm, _) = self.create(goal, members, cwd, None, before_active)?;
        Ok(swarm)
    }

    /// Creates the swarm in `setup`, fills the board and activates it.
    fn create(
        &self,
        goal: &str,
        members: &[MemberSpec],
        cwd: &str,
        tasks: Option<&[(String, String)]>,
        before_active: &dyn Fn(&str),
    ) -> DbResult<(String, Vec<Seat>)> {
        if members.is_empty() {
            return Err(SwarmError::new("Un enjambre necesita al menos un miembro.").into());
        }
        // In «setup» the supervisor neither opens agents nor issues tokens until
        // the swarm is complete.
        let swarm = self.store.create(goal, cwd, "setup")?;
        let seats = match self.populate(&swarm, members, tasks) {
            Ok(seats) => seats,
            Err(error) => {
                let _ = self.store.set_status(&swarm, "stopped");
                return Err(error);
            }
        };
        before_active(&swarm);
        self.store.set_status(&swarm, "active")?;
        info!("Enjambre {} creado con {} miembros.", swarm, seats.len());
        Ok((swarm, seats))
    }

    /// One `fix` task of the autonomous mode (10.18); `note` carries the
    /// correction file so the member that takes it knows where to look.
    pub fn fix_task(&self, swarm: &str, title: &str, note: &str, role: &str) -> DbResult<i64> {
        if self.store.tasks(swarm)?.len() >= MAX_TASKS {
            return Err(SwarmError::new(format!(
                "El enjambre ya tiene {MAX_TASKS} tareas; agrupa el trabajo."
            ))
            .into());
        }
        // El tablero se cierra solo al terminar; una corrección lo reabre.
        if self
            .store
            .get(swarm)?
            .is_some_and(|info| info.status == "done")
        {
            self.store.set_status(swarm, "active")?;
        }
        let task = self.store.add_task(swarm, title, role, "", &[], 0)?;
        self.store.set_task_note(swarm, task.id, note)?;
        self.changed(swarm, SYSTEM, "add");
        Ok(task.id)
    }

    fn populate(
        &self,
        swarm: &str,
        specs: &[MemberSpec],
        tasks: Option<&[(String, String)]>,
    ) -> DbResult<Vec<Seat>> {
        for spec in specs {
            self.store.join(
                swarm,
                &spec.name,
                &spec.role,
                &spec.agent,
                "",
                &spec.model,
                &spec.effort,
            )?;
        }
        let stages: Vec<Vec<(String, String, String)>> = match tasks {
            Some(tasks) => tasks
                .iter()
                .map(|(title, role)| vec![(title.clone(), role.clone(), String::new())])
                .collect(),
            None => stages_for(specs),
        };
        let mut previous: Vec<i64> = Vec::new();
        for stage in stages {
            let mut ids = Vec::new();
            for (title, role, owner) in stage {
                let task = self
                    .store
                    .add_task(swarm, &title, &role, &owner, &previous, 0)?;
                ids.push(task.id);
            }
            previous = ids;
        }
        let mut seats = Vec::new();
        for member in self.store.members(swarm)? {
            seats.push(Seat {
                name: member.name.clone(),
                token: self.issue_token(swarm, &member.name)?,
                briefing: self.briefing(swarm, &member.name)?,
            });
        }
        Ok(seats)
    }
}
