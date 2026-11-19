//! Tareas, aprobaciones de la persona y estado.

use std::path::{Path, PathBuf};

use crate::criteria;
use crate::git;
use crate::paths;
use crate::state::{Status, Task, TeamStatus};
use crate::texts;
use crate::types::{Caller, Outcome, Rejection};

use super::emit::Effects;
use super::Arbiter;

impl Arbiter {
    /// R2: crea la tarea, copia los artefactos y emite la tarjeta.
    pub(crate) fn task_new(
        &self,
        space_id: &str,
        title: String,
        brief: PathBuf,
        criteria_path: PathBuf,
    ) -> Result<Outcome, Rejection> {
        let space = self.space(space_id)?;
        {
            let inner = self.inner.lock();
            if inner
                .tasks
                .values()
                .any(|task| task.is_active() && task.space.id == space_id)
            {
                return Err(Rejection::new("R2", texts::ACTIVE_TASK));
            }
        }
        let (task, card) = self.build_task(&space, title, brief, criteria_path)?;
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "task_proposed",
            serde_json::json!({ "title": task.title, "card": card }),
        ));
        let id = task.id.clone();
        self.commit(&task, effects);
        Ok(Outcome {
            message: card,
            data: serde_json::json!({ "task": id, "status": "pending_approval" }),
        })
    }

    /// Comprueba encargo, criterios y git antes de tocar nada.
    fn validate_inputs(
        &self,
        space: &crate::types::Space,
        brief: &Path,
        criteria_path: &Path,
    ) -> Result<(PathBuf, PathBuf, criteria::Criteria), Rejection> {
        let brief_abs = paths::resolve_inside(&space.folder, brief)
            .ok_or_else(|| Rejection::new("R2", texts::outside_folder(brief)))?;
        if !brief_abs.is_file() {
            return Err(Rejection::new("R2", texts::brief_missing(brief)));
        }
        let criteria_abs = paths::resolve_inside(&space.folder, criteria_path)
            .ok_or_else(|| Rejection::new("R2", texts::outside_folder(criteria_path)))?;
        if !criteria_abs.is_file() {
            return Err(Rejection::new("R2", texts::criteria_missing(criteria_path)));
        }
        let parsed = criteria::load(&criteria_abs, &space.folder)
            .map_err(|message| Rejection::new("R2", message))?;
        if !git::is_repo(&space.folder) {
            return Err(Rejection::new("R2", texts::GIT_NOT_REPO));
        }
        Ok((brief_abs, criteria_abs, parsed))
    }

    /// Copia los artefactos y construye la tarea en espera de aprobación.
    fn build_task(
        &self,
        space: &crate::types::Space,
        title: String,
        brief: PathBuf,
        criteria_path: PathBuf,
    ) -> Result<(Task, String), Rejection> {
        let (brief_abs, criteria_abs, parsed) =
            self.validate_inputs(space, &brief, &criteria_path)?;
        // DECISION: el título vacío recibe uno fijo para que la tarjeta lega.
        let title = if title.trim().is_empty() {
            "Tarea".to_string()
        } else {
            title.trim().to_string()
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = paths::task_dir(&space.folder, &id);
        paths::copy_file(&brief_abs, &dir.join("encargo.md")).map_err(|error| {
            Rejection::new("R2", format!("No se pudo copiar el encargo: {error}."))
        })?;
        paths::copy_file(&criteria_abs, &dir.join("criterios.toml")).map_err(|error| {
            Rejection::new("R2", format!("No se pudo copiar los criterios: {error}."))
        })?;
        let frozen = dir.join("criterios.congelados.toml");
        paths::copy_file(&criteria_abs, &frozen).map_err(|error| {
            Rejection::new("R2", format!("No se pudo congelar los criterios: {error}."))
        })?;
        let sha = paths::sha256_file(&frozen).map_err(|message| Rejection::new("R2", message))?;
        let brief_text = paths::read_text(&brief_abs).unwrap_or_default();
        let card = texts::task_card(&title, &brief_text, &parsed.card_lines());
        let task = Task {
            id: id.clone(),
            title: title.clone(),
            space: space.clone(),
            status: Status::PendingApproval,
            paused_from: None,
            pause_reason: None,
            pause_message: None,
            created: self.now(),
            approved_at: None,
            team: Vec::new(),
            team_status: TeamStatus::None,
            team_card: String::new(),
            round: 0,
            clean_streak: 0,
            criteria_sha: sha,
            last_close: None,
            rounds: Vec::new(),
            fish: Vec::new(),
        };
        Ok((task, card))
    }

    /// Estado resumido para el elefante o para un pez.
    pub(crate) fn status(&self, caller: &Caller) -> Result<Outcome, Rejection> {
        match caller {
            Caller::Elephant { space } => {
                let space_info = self.space(space)?;
                let task = self.task_for_elephant(space).ok();
                let Some(task) = task else {
                    return Ok(Outcome::new(texts::STATUS_NO_TASK));
                };
                let message = texts::status_elephant(
                    &space_info.name,
                    &task.id,
                    &task.title,
                    texts::status_label(task.status.id()),
                    task.round,
                    task.clean_streak,
                    task.team_status.id(),
                );
                Ok(Outcome {
                    message,
                    data: task.snapshot(),
                })
            }
            Caller::Fish { fish } => {
                let Some(task) = self.task_for_fish(fish).ok() else {
                    return Ok(Outcome::new(texts::STATUS_UNKNOWN_FISH));
                };
                let Some(record) = task.fish(fish) else {
                    return Ok(Outcome::new(texts::STATUS_UNKNOWN_FISH));
                };
                let message = texts::status_fish(
                    fish,
                    record.role.label(),
                    &task.id,
                    record.round,
                    record.pending,
                );
                Ok(Outcome {
                    message,
                    data: task.snapshot(),
                })
            }
        }
    }

    /// La persona aprueba: se congelan los criterios.
    pub(crate) fn approve_task_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::PendingApproval {
            return Err(Rejection::new("R2", texts::NOT_PENDING));
        }
        let live = paths::task_dir(&task.space.folder, &task.id).join("criterios.toml");
        if !live.is_file() {
            return Err(Rejection::new("R2", texts::criteria_missing(&live)));
        }
        let frozen = self.task_path(&task, "criterios.congelados.toml");
        paths::copy_file(&live, &frozen).map_err(|error| {
            Rejection::new("R2", format!("No se pudo congelar los criterios: {error}."))
        })?;
        task.criteria_sha =
            paths::sha256_file(&frozen).map_err(|message| Rejection::new("R2", message))?;
        task.status = Status::Approved;
        task.approved_at = Some(self.now());
        let mut effects = Effects::default();
        effects.both(
            self.event(&task, "task_approved", serde_json::json!({})),
            self.elephant_event(
                &task,
                "task_approved",
                texts::TASK_APPROVED.to_string(),
                serde_json::json!({ "task": task.id }),
            ),
        );
        self.commit(&task, effects);
        Ok(Outcome::new(texts::TASK_APPROVED))
    }

    /// La persona rechaza la tarea.
    pub(crate) fn reject_task_inner(
        &self,
        space: &str,
        comment: &str,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::PendingApproval {
            return Err(Rejection::new("R2", texts::NOT_PENDING));
        }
        task.status = Status::Rejected;
        let message = texts::task_rejected(comment);
        let mut effects = Effects::default();
        effects.both(
            self.event(
                &task,
                "task_rejected",
                serde_json::json!({ "comment": comment }),
            ),
            self.elephant_event(
                &task,
                "task_rejected",
                message.clone(),
                serde_json::json!({ "comment": comment }),
            ),
        );
        self.commit(&task, effects);
        Ok(Outcome::new(message))
    }

    /// R10: cierra la tarea con dos rondas limpias seguidas.
    pub(crate) fn task_close(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status == Status::InRound {
            return Err(Rejection::new("R10", texts::CLOSE_ROUND_FIRST));
        }
        if task.status == Status::Paused {
            return Err(Rejection::new(
                "R10",
                texts::paused(&self.pause_text(&task)),
            ));
        }
        if task.clean_streak < 2 {
            return Err(Rejection::new(
                "R10",
                texts::two_clean_rounds(task.clean_streak),
            ));
        }
        task.status = Status::Done;
        let message = texts::task_closed(task.clean_streak);
        let mut effects = Effects::default();
        effects.event(self.event(&task, "task_closed", serde_json::json!({})));
        self.commit(&task, effects);
        Ok(Outcome::new(message))
    }

    /// `ez pause`: pausa manual de la persona o del elefante.
    pub(crate) fn pause_command(&self, space: &str, reason: String) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status.is_terminal() {
            return Err(Rejection::new("R4", texts::TASK_FINISHED));
        }
        if task.status == Status::Paused {
            return Err(Rejection::new("R4", texts::ALREADY_PAUSED));
        }
        let reason = if reason.trim().is_empty() {
            texts::PAUSED_MANUAL.to_string()
        } else {
            reason.trim().to_string()
        };
        task.pause("manual", reason.clone());
        let mut effects = Effects::default();
        effects.both(
            self.event(
                &task,
                "paused",
                serde_json::json!({ "reason": "manual", "message": reason }),
            ),
            self.elephant_event(
                &task,
                "paused",
                format!("La tarea quedó en pausa: {reason}"),
                serde_json::json!({ "reason": "manual", "message": reason }),
            ),
        );
        self.commit(&task, effects);
        Ok(Outcome::new(texts::paused(&reason)))
    }

    /// La persona reanuda la tarea.
    pub(crate) fn resume_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::Paused {
            return Err(Rejection::new("R4", texts::NOT_PAUSED));
        }
        task.status = task.paused_from.unwrap_or(Status::Approved);
        task.paused_from = None;
        task.pause_reason = None;
        task.pause_message = None;
        let mut effects = Effects::default();
        effects.both(
            self.event(&task, "resumed", serde_json::json!({})),
            self.elephant_event(
                &task,
                "resumed",
                texts::RESUMED.to_string(),
                serde_json::json!({ "task": task.id }),
            ),
        );
        self.commit(&task, effects);
        Ok(Outcome::new(texts::RESUMED))
    }

    /// La persona detiene la tarea.
    pub(crate) fn stop_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        task.status = Status::Stopped;
        let mut effects = Effects::default();
        effects.event(self.event(&task, "stopped", serde_json::json!({})));
        self.commit(&task, effects);
        Ok(Outcome::new(texts::STOPPED))
    }

    /// Texto de la pausa vigente.
    pub(crate) fn pause_text(&self, task: &Task) -> String {
        task.pause_message
            .clone()
            .unwrap_or_else(|| texts::PAUSED_NO_REASON.to_string())
    }

    /// Verificación de R3: los criterios no cambiaron desde la aprobación.
    pub(crate) fn check_frozen(&self, task: &Task) -> Result<(), Rejection> {
        let live = paths::task_dir(&task.space.folder, &task.id).join("criterios.toml");
        let sha = paths::sha256_file(&live).map_err(|message| Rejection::new("R3", message))?;
        if sha == task.criteria_sha {
            Ok(())
        } else {
            Err(Rejection::new("R3", texts::CRITERIA_CHANGED))
        }
    }

    /// Deja la tarea en pausa por un motivo y devuelve el rechazo.
    pub(crate) fn pause_task(
        &self,
        mut task: Task,
        rule: &'static str,
        code: &str,
        message: String,
    ) -> Rejection {
        task.pause(code, message.clone());
        let mut effects = Effects::default();
        effects.both(
            self.event(
                &task,
                "paused",
                serde_json::json!({ "reason": code, "message": message }),
            ),
            self.elephant_event(
                &task,
                "paused",
                message.clone(),
                serde_json::json!({ "reason": code, "message": message }),
            ),
        );
        self.commit(&task, effects);
        Rejection::new(rule, message)
    }

    /// Verifica que el pez tenga algo que informar (R7).
    pub(crate) fn report_allowed(&self, task: &Task, fish: &str) -> Result<(), Rejection> {
        let record = task
            .fish(fish)
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_FISH))?;
        if record.reported && record.role.is_verifier() {
            return Err(Rejection::new("R7", texts::already_reported(fish)));
        }
        if !record.pending {
            return Err(Rejection::new("R7", texts::no_pending_work(fish)));
        }
        Ok(())
    }
}
