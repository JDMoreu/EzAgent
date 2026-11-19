//! Tareas, aprobaciones de la persona, artefactos congelados y estado.

use std::path::{Path, PathBuf};

use crate::criteria;
use crate::events::CardKind;
use crate::git;
use crate::paths;
use crate::state::{Status, Task, TeamStatus, WorkState};
use crate::texts;
use crate::types::{Caller, FishKey, Outcome, Rejection};
use crate::verify;

use super::emit::Effects;
use super::{new_task_id, ArbiterShared};

impl ArbiterShared {
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
            serde_json::json!({ "title": task.title, "card": card.clone() }),
        ));
        let opened = self.open_card(
            space_id,
            Some(&task.id),
            CardKind::Task,
            format!("Tarea propuesta: {}", task.title),
            card.clone(),
        );
        effects.event(self.card_opened_event(&opened));
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
        let brief_abs = paths::inside(&space.folder, brief)
            .map_err(|_| Rejection::new("R2", texts::outside_folder(brief)))?;
        if !brief_abs.is_file() {
            return Err(Rejection::new("R2", texts::brief_missing(brief)));
        }
        let criteria_abs = paths::inside(&space.folder, criteria_path)
            .map_err(|_| Rejection::new("R2", texts::outside_folder(criteria_path)))?;
        if !criteria_abs.is_file() {
            return Err(Rejection::new("R2", texts::criteria_missing(criteria_path)));
        }
        let parsed = criteria::load(&criteria_abs, &space.folder)
            .map_err(|message| Rejection::new("R2", message))?;
        if !crate::git::is_repo(&space.folder) {
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
        // DECISION: el título vacío recibe uno fijo para que la tarjeta sea lega.
        let title = if title.trim().is_empty() {
            "Tarea".to_string()
        } else {
            title.trim().to_string()
        };
        let id = new_task_id();
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
        let task = self.new_task_state(space, title, id, sha);
        Ok((task, card))
    }

    /// Estado inicial de una tarea recién propuesta.
    fn new_task_state(
        &self,
        space: &crate::types::Space,
        title: String,
        id: String,
        sha: String,
    ) -> Task {
        Task {
            id,
            title,
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
            encargo_sha: String::new(),
            team_sha: None,
            last_close: None,
            last_clean_close: None,
            last_fp: None,
            extra_rounds: 0,
            extra_hours: 0.0,
            paused_total: 0.0,
            paused_at: None,
            rounds: Vec::new(),
            fish: Vec::new(),
            work: Vec::new(),
        }
    }

    /// Estado resumido para el elefante o para un pez.
    pub(crate) fn status(&self, caller: &Caller) -> Result<Outcome, Rejection> {
        match caller {
            Caller::Elephant { space } => {
                let space_info = self.space(space)?;
                let Some(task) = self.task_for_elephant(space).ok() else {
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
            Caller::Fish(key) => {
                let Some(task) = self.task_for_fish_key(key).ok() else {
                    return Ok(Outcome::new(texts::STATUS_UNKNOWN_FISH));
                };
                let Some(record) = task.fish_by_key(key) else {
                    return Ok(Outcome::new(texts::STATUS_UNKNOWN_FISH));
                };
                let pending = task.pending_work(key).is_some();
                let message = texts::status_fish(
                    &key.fish,
                    record.role.label(),
                    &task.id,
                    record.round,
                    pending,
                );
                Ok(Outcome {
                    message,
                    data: task.snapshot(),
                })
            }
        }
    }

    /// La persona aprueba: se congelan los criterios y el encargo.
    pub(crate) fn approve_task_impl(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::PendingApproval {
            return Err(Rejection::new("R2", texts::NOT_PENDING));
        }
        let dir = paths::task_dir(&task.space.folder, &task.id);
        for name in ["criterios.toml", "encargo.md"] {
            if !dir.join(name).is_file() {
                return Err(Rejection::new("R2", texts::outside_folder(&dir.join(name))));
            }
        }
        let (criteria_sha, encargo_sha) = self.freeze_artifacts(&task)?;
        task.criteria_sha = criteria_sha;
        task.encargo_sha = encargo_sha;
        task.status = Status::Approved;
        task.approved_at = Some(self.now());
        task.last_fp = Some(
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R2", message))?,
        );
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
        if let Some(card) = self.take_card(space, CardKind::Task) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, effects);
        Ok(Outcome::new(texts::TASK_APPROVED))
    }

    /// La persona rechaza la tarea.
    pub(crate) fn reject_task_impl(
        &self,
        space: &str,
        comment: &str,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::PendingApproval {
            return Err(Rejection::new("R2", texts::NOT_PENDING));
        }
        task.status = Status::Rejected;
        self.revoke_task_tokens(&task);
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
        if let Some(card) = self.take_card(space, CardKind::Task) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
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
        if task.status != Status::BetweenRounds {
            return Err(Rejection::new("R10", texts::close_between_rounds()));
        }
        if let Err(rejection) = self.check_frozen(&task) {
            return Err(self.pause_task(task, "R3", "criterios_modificados", rejection.message));
        }
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let criteria = self.load_criteria(&task)?;
        let checks = verify::run_all(&task.space.folder, &criteria);
        let failed = checks.iter().filter(|check| !check.ok).count();
        if failed > 0 {
            return Err(Rejection::new(
                "R10",
                texts::verify_failed(failed, checks.len()),
            ));
        }
        let current =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R10", message))?;
        if task
            .last_clean_close
            .as_ref()
            .is_some_and(|fingerprint| fingerprint != &current)
        {
            return Err(Rejection::new("R10", texts::project_changed_since_clean()));
        }
        task.last_fp = Some(current);
        if task.clean_streak < 2 {
            return Err(Rejection::new(
                "R10",
                texts::two_clean_rounds(task.clean_streak),
            ));
        }
        task.status = Status::Done;
        self.revoke_task_tokens(&task);
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
        task.pause("manual", reason.clone(), self.now());
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
        let opened = self.open_card(
            space,
            Some(&task.id),
            CardKind::Pause,
            "Tarea en pausa".to_string(),
            reason.clone(),
        );
        effects.event(self.card_opened_event(&opened));
        self.commit(&task, effects);
        Ok(Outcome::new(texts::paused(&reason)))
    }

    /// La persona reanuda la tarea; si la pausa fue de artefactos, los restaura.
    pub(crate) fn resume_impl(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::Paused {
            return Err(Rejection::new("R4", texts::NOT_PAUSED));
        }
        let mut message = texts::RESUMED.to_string();
        if task.pause_reason.as_deref() == Some("artefactos_modificados") {
            self.restore_artifacts(&mut task)?;
            message = texts::ARTIFACTS_RESTORED.to_string();
        }
        if task.pause_reason.as_deref() == Some("cambios_fuera_de_ronda") {
            task.clean_streak = 0;
            task.last_fp = Some(
                git::snapshot(&task.space.folder).map_err(|reason| Rejection::new("R4", reason))?,
            );
        }
        task.resume(self.now());
        let mut effects = Effects::default();
        effects.both(
            self.event(&task, "resumed", serde_json::json!({})),
            self.elephant_event(
                &task,
                "resumed",
                message.clone(),
                serde_json::json!({ "task": task.id }),
            ),
        );
        if let Some(card) = self.take_card(space, CardKind::Pause) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, effects);
        Ok(Outcome::new(message))
    }

    /// La persona da más margen (+2 rondas, +1 h) y reanuda (§14.5).
    pub(crate) fn more_time_impl(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::Paused {
            return Err(Rejection::new("R4", texts::NOT_PAUSED));
        }
        task.extra_rounds = task.extra_rounds.saturating_add(2);
        task.extra_hours += 1.0;
        task.resume(self.now());
        let message = texts::more_time(task.extra_rounds, task.extra_hours);
        let mut effects = Effects::default();
        effects.both(
            self.event(
                &task,
                "resumed",
                serde_json::json!({ "extra_rounds": 2, "extra_hours": 1.0 }),
            ),
            self.elephant_event(&task, "resumed", message.clone(), serde_json::json!({})),
        );
        if let Some(card) = self.take_card(space, CardKind::Pause) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, effects);
        Ok(Outcome::new(message))
    }

    /// La persona detiene la tarea.
    pub(crate) fn stop_impl(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        task.status = Status::Stopped;
        self.revoke_task_tokens(&task);
        let mut effects = Effects::default();
        effects.event(self.event(&task, "stopped", serde_json::json!({})));
        if let Some(card) = self.take_card(space, CardKind::Pause) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, effects);
        Ok(Outcome::new(texts::STOPPED))
    }

    /// Texto de la pausa vigente.
    pub(crate) fn pause_text(&self, task: &Task) -> String {
        task.pause_message
            .clone()
            .unwrap_or_else(|| texts::PAUSED_NO_REASON.to_string())
    }

    /// Detecta cambios del elefante cuando no hay trabajo pendiente (§14.4).
    /// Se llama antes de cada comando del elefante; nunca retiene el candado
    /// mientras calcula la huella.
    pub(crate) fn check_elephant_changes(&self, space: &str) -> Result<(), Rejection> {
        let Some(task) = self.task_for_elephant(space).ok() else {
            return Ok(());
        };
        if task
            .work
            .iter()
            .any(|item| item.state == WorkState::Pending)
        {
            return Ok(());
        }
        let Some(expected) = task.last_fp.clone() else {
            return Ok(());
        };
        let current =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R11", message))?;
        if current == expected {
            return Ok(());
        }
        let changed = git::changed(&expected, &current);
        let message = texts::elephant_changed_outside_round(&changed);
        Err(self.pause_task(task, "R11", "cambios_fuera_de_ronda", message))
    }

    /// Deja la tarea en pausa por un motivo y devuelve el rechazo.
    pub(crate) fn pause_task(
        &self,
        mut task: Task,
        rule: &'static str,
        code: &str,
        message: String,
    ) -> Rejection {
        task.pause(code, message.clone(), self.now());
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
        let opened = self.open_card(
            &task.space.id,
            Some(&task.id),
            CardKind::Pause,
            "Tarea en pausa".to_string(),
            message.clone(),
        );
        effects.event(self.card_opened_event(&opened));
        self.commit(&task, effects);
        Rejection::new(rule, message)
    }

    /// Verifica que la clave tenga trabajo pendiente (R7).
    pub(crate) fn report_allowed(&self, task: &Task, key: &FishKey) -> Result<(), Rejection> {
        if task.pending_work(key).is_some() {
            return Ok(());
        }
        let verifier = task
            .fish_by_key(key)
            .is_some_and(|record| record.role.is_verifier());
        if verifier && task.work_for_key(key).is_some() {
            return Err(Rejection::new("R7", texts::already_reported(&key.fish)));
        }
        Err(Rejection::new("R7", texts::no_pending_work(&key.fish)))
    }
}
