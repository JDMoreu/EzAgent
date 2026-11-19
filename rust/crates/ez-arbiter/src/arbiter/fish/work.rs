//! R6 y R7: correcciones e informes con ruta exacta (diseño §14.3).

use std::path::PathBuf;

use ez_team::Role;

use crate::git;
use crate::paths;
use crate::state::{FishRecord, ReportRecord, Status, WorkItem, WorkKind, WorkState};
use crate::texts;
use crate::types::{FishKey, Outcome, Rejection, Verdict};

use super::{attempt_suffix, ArbiterShared, ReportInput};
use crate::arbiter::emit::Effects;

impl ArbiterShared {
    /// R6: corrección a planificador o desarrollador vivo, antes del QA.
    pub(crate) fn fish_send(
        &self,
        space: &str,
        fish: &str,
        file: PathBuf,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R6", texts::NO_ROUND));
        }
        let record = task
            .fish(fish)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_FISH))?;
        if !matches!(record.role, Role::Planner | Role::Developer) {
            return Err(Rejection::new("R6", texts::cannot_correct(fish)));
        }
        if !self.launcher.alive(&record.terminal) {
            return Err(Rejection::new("R6", texts::terminal_dead(fish)));
        }
        if task
            .fish_in_round(record.round)
            .any(|other| other.role == Role::Qa)
        {
            return Err(Rejection::new("R6", texts::QA_STARTED));
        }
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let source = paths::inside(&task.space.folder, &file)
            .map_err(|_| Rejection::new("R6", texts::outside_folder(&file)))?;
        if !source.is_file() {
            return Err(Rejection::new("R6", texts::correction_missing(&file)));
        }
        self.apply_correction(&mut task, &record, fish, &source)
    }

    /// Copia la corrección, la envía y renueva el trabajo pendiente.
    fn apply_correction(
        &self,
        task: &mut crate::state::Task,
        record: &FishRecord,
        fish: &str,
        source: &std::path::Path,
    ) -> Result<Outcome, Rejection> {
        let number = record.corrections + 1;
        let suffix = attempt_suffix(record.key.attempt);
        let folder = paths::round_dir(&task.space.folder, &task.id, record.round);
        let destination = folder.join(format!("{fish}{suffix}.correccion-{number}.md"));
        let report_path = folder.join(format!("{fish}{suffix}.informe-c{number}.md"));
        paths::copy_file(source, &destination).map_err(|error| {
            Rejection::new("R6", format!("No se pudo copiar la corrección: {error}."))
        })?;
        let line = texts::correction_line(record.round, number, &destination, &report_path);
        self.launcher
            .send(&record.terminal, &line)
            .map_err(|detail| {
                Rejection::new(
                    "R6",
                    format!("No se pudo enviar la corrección a {fish}: {detail}."),
                )
            })?;
        let start =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R6", message))?;
        if let Some(record) = task.fish_by_key_mut(&record.key) {
            record.corrections = number;
            record.start = Some(start);
        }
        if let Some(item) = task.work.iter_mut().find(|item| item.key == record.key) {
            item.kind = WorkKind::Correction(number);
            item.report_path = report_path.clone();
            item.state = WorkState::Pending;
            item.started_ts = self.now();
        } else {
            task.work.push(WorkItem {
                key: record.key.clone(),
                round: record.round,
                kind: WorkKind::Correction(number),
                report_path: report_path.clone(),
                state: WorkState::Pending,
                started_ts: self.now(),
            });
        }
        let mut effects = Effects::default();
        effects.event(self.event(
            task,
            "fish_corrected",
            serde_json::json!({ "fish": fish, "file": destination, "report": report_path }),
        ));
        self.commit(task, effects);
        Ok(Outcome::new(texts::correction_sent(fish)))
    }

    /// R7: recibe el informe en la ruta exacta, lo copia y avisa al elefante.
    pub(crate) fn report(&self, key: &FishKey, input: ReportInput) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_fish_key(key)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R7", texts::NO_ROUND));
        }
        self.report_allowed(&task, key)?;
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let work = task
            .pending_work(key)
            .ok_or_else(|| Rejection::new("R7", texts::no_pending_work(&key.fish)))?;
        let expected = self.canonical_report_path(&task, &work.report_path)?;
        let report_file = paths::inside(&task.space.folder, &input.file)
            .map_err(|_| Rejection::new("R7", texts::outside_folder(&input.file)))?;
        if report_file != expected {
            return Err(Rejection::new("R7", texts::wrong_report_path(&expected)));
        }
        if !report_file.is_file() {
            return Err(Rejection::new("R7", texts::report_missing(&input.file)));
        }
        self.check_report_verdict(&task, key, &input)?;
        let kind_tag = match work.kind {
            WorkKind::Launch => String::new(),
            WorkKind::Correction(number) => format!("-c{number}"),
        };
        self.accept_report(&mut task, key, input, report_file, kind_tag)
    }

    /// Canonicaliza la ruta esperada para compararla con la del informe.
    fn canonical_report_path(
        &self,
        task: &crate::state::Task,
        path: &std::path::Path,
    ) -> Result<PathBuf, Rejection> {
        paths::inside(&task.space.folder, path)
            .map_err(|_| Rejection::new("R7", texts::wrong_report_path(path)))
    }

    /// Valida mutaciones del validador y `pass` sin hallazgos.
    fn check_report_verdict(
        &self,
        task: &crate::state::Task,
        key: &FishKey,
        input: &ReportInput,
    ) -> Result<(), Rejection> {
        let record = task
            .fish_by_key(key)
            .ok_or_else(|| Rejection::new("R1", texts::FISH_NOT_ACTIVE))?;
        if record.role == Role::QaValidator
            && (input.mutations_tried.is_none() || input.mutations_caught.is_none())
        {
            return Err(Rejection::new("R7", texts::MUTATIONS_REQUIRED));
        }
        if input.verdict == Verdict::Pass && (input.blocking > 0 || input.major > 0) {
            return Err(Rejection::new("R7", texts::PASS_WITH_FINDINGS));
        }
        Ok(())
    }

    /// Copia el informe al almacén privado, atribuye cambios y emite eventos.
    fn accept_report(
        &self,
        task: &mut crate::state::Task,
        key: &FishKey,
        input: ReportInput,
        report_file: PathBuf,
        kind_tag: String,
    ) -> Result<Outcome, Rejection> {
        let private = crate::paths::data_task_dir(&self.data_dir, &task.id).join("informes");
        let copy = private.join(format!(
            "{}-a{}{}.informe.md",
            key.fish, key.attempt, kind_tag
        ));
        paths::copy_file(&report_file, &copy).map_err(|error| {
            Rejection::new("R7", format!("No se pudo guardar el informe: {error}."))
        })?;
        let sha = paths::sha256_file(&copy).map_err(|message| Rejection::new("R7", message))?;
        let after =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R7", message))?;
        let record = task
            .fish_by_key(key)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::FISH_NOT_ACTIVE))?;
        let changed = record
            .start
            .as_ref()
            .map(|start| git::changed(start, &after))
            .unwrap_or_default();
        let report = ReportRecord {
            verdict: input.verdict,
            file: report_file.clone(),
            sha,
            blocking: input.blocking,
            major: input.major,
            minor: input.minor,
            mutations_tried: input.mutations_tried,
            mutations_caught: input.mutations_caught,
        };
        if let Some(round) = task.open_round_mut() {
            round.attribute(record.role, &changed);
        }
        if let Some(item) = task.work.iter_mut().find(|item| item.key == *key) {
            item.state = WorkState::Reported;
        }
        if let Some(record) = task.fish_by_key_mut(key) {
            record.report = Some(report);
        }
        if record.role.is_verifier() {
            self.revoke_token(key);
        }
        let message = texts::report_received(&key.fish, input.verdict);
        let data = serde_json::json!({
            "fish": key.fish, "attempt": key.attempt,
            "verdict": input.verdict.id(), "file": report_file,
        });
        let mut effects = Effects::default();
        effects.both(
            self.event(task, "fish_reported", data.clone()),
            self.elephant_event(task, "fish_reported", message.clone(), data),
        );
        self.commit(task, effects);
        Ok(Outcome::new(message))
    }
}
