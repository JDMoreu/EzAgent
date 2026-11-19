//! R5, R6 y R7: fases, lanzamiento, correcciones e informes (diseño §14.3).

use std::path::{Path, PathBuf};

use ez_team::{InstructionContext, Role};

use crate::git;
use crate::paths;
use crate::state::{FishRecord, ReportRecord, Status, WorkItem, WorkKind, WorkState};
use crate::texts;
use crate::types::{Delivery, FishKey, Launched, Outcome, Rejection, Verdict};

use super::emit::Effects;
use super::{fish_id, hc_name, new_token, Arbiter};

/// Datos de un informe, agrupados para no pasar ocho argumentos.
#[derive(Debug, Clone)]
pub(crate) struct ReportInput {
    pub verdict: Verdict,
    pub file: PathBuf,
    pub blocking: u32,
    pub major: u32,
    pub minor: u32,
    pub mutations_tried: Option<u32>,
    pub mutations_caught: Option<u32>,
}

/// Sufijo de nombre de archivo según el intento.
fn attempt_suffix(attempt: u32) -> String {
    if attempt > 1 {
        format!("-a{attempt}")
    } else {
        String::new()
    }
}

impl Arbiter {
    /// R5: valida la fase, escribe instrucciones y entrega el encargo al pez.
    pub(crate) fn fish_new(
        &self,
        space: &str,
        role: Role,
        brief: PathBuf,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R5", texts::NO_ROUND));
        }
        let round = task.round;
        let seat = task
            .seat(role)
            .cloned()
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(role.id())))?;
        self.check_order(&task, role, round, seat.count)?;
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        let attempts = task.attempts(role, round);
        if attempts >= 3 {
            return Err(Rejection::new("R5", texts::too_many_attempts(role.id())));
        }
        let attempt = attempts + 1;
        let index = if role == Role::Developer {
            task.fish_in_round(round)
                .filter(|fish| fish.role == Role::Developer)
                .count() as u32
                + 1
        } else {
            1
        };
        let fish = fish_id(role, round, index);
        let suffix = attempt_suffix(attempt);
        let key = FishKey {
            space: task.space.id.clone(),
            task: task.id.clone(),
            fish: fish.clone(),
            attempt,
        };
        let folder = paths::round_dir(&task.space.folder, &task.id, round);
        let fish_brief = folder.join(format!("{fish}{suffix}.encargo.md"));
        let instructions = folder.join(format!("{fish}{suffix}.instrucciones.md"));
        let report_path = folder.join(format!("{fish}{suffix}.informe.md"));
        let start = self.copy_fish_brief(&task, &brief, &fish_brief)?;
        let context =
            self.instruction_context(&task, role, &key, round, fish_brief, &report_path)?;
        self.write_instructions(&context, &instructions, &fish)?;
        let token = new_token();
        let request = self.launch_request(&task, role, &seat, &key, &instructions, &token);
        let launched = self.launcher.launch(&request);
        let (terminal, mut message) = match launched {
            Ok(Launched {
                terminal,
                delivery: Delivery::Delivered,
            }) => (terminal.clone(), texts::fish_launched(&fish, &terminal)),
            Ok(Launched {
                terminal,
                delivery: Delivery::Unconfirmed,
            }) => (
                terminal.clone(),
                format!(
                    "{}{}",
                    texts::fish_launched(&fish, &terminal),
                    texts::delivery_unconfirmed(&fish)
                ),
            ),
            Ok(Launched {
                delivery: Delivery::Failed(detail),
                ..
            }) => {
                return Err(self.fail_launch(task, &key, role, round, &report_path, &detail, &token));
            }
            Err(detail) => {
                return Err(self.fail_launch(task, &key, role, round, &report_path, &detail, &token));
            }
        };
        self.remember_token(&key, &token);
        task.fish.push(FishRecord {
            key: key.clone(),
            role,
            terminal: terminal.clone(),
            round,
            start: Some(start),
            report: None,
            corrections: 0,
        });
        task.work.push(WorkItem {
            key: key.clone(),
            round,
            kind: WorkKind::Launch,
            report_path: report_path.clone(),
            state: WorkState::Pending,
            started_ts: self.now(),
        });
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "fish_launched",
            serde_json::json!({
                "fish": fish, "role": role.id(), "terminal": terminal, "attempt": attempt,
            }),
        ));
        self.commit(&task, effects);
        message.push_str(&format!(" Informa con --file \"{}\".", report_path.display()));
        Ok(Outcome {
            message,
            data: serde_json::json!({ "fish": fish, "terminal": terminal, "attempt": attempt }),
        })
    }

    /// Copia el encargo del pez y toma la foto de git antes de lanzarlo.
    fn copy_fish_brief(
        &self,
        task: &crate::state::Task,
        brief: &Path,
        fish_brief: &Path,
    ) -> Result<git::GitSnapshot, Rejection> {
        let brief_abs = paths::inside(&task.space.folder, brief)
            .map_err(|_| Rejection::new("R5", texts::outside_folder(brief)))?;
        if !brief_abs.is_file() {
            return Err(Rejection::new("R5", texts::fish_brief_missing(brief)));
        }
        paths::copy_file(&brief_abs, fish_brief).map_err(|error| {
            Rejection::new(
                "R5",
                format!("No se pudo copiar el encargo del pez: {error}."),
            )
        })?;
        git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R5", message))
    }

    /// Deja el trabajo fallido, revoca el token y avisa al elefante (§14.2).
    fn fail_launch(
        &self,
        mut task: crate::state::Task,
        key: &FishKey,
        role: Role,
        round: u32,
        report_path: &Path,
        detail: &str,
        token: &str,
    ) -> Rejection {
        self.launcher.revoke(token);
        task.fish.push(FishRecord {
            key: key.clone(),
            role,
            terminal: String::new(),
            round,
            start: None,
            report: None,
            corrections: 0,
        });
        task.work.push(WorkItem {
            key: key.clone(),
            round,
            kind: WorkKind::Launch,
            report_path: report_path.to_path_buf(),
            state: WorkState::Failed,
            started_ts: self.now(),
        });
        let message = texts::delivery_failed(&key.fish, detail);
        let data = serde_json::json!({
            "fish": key.fish, "attempt": key.attempt, "reason": detail,
        });
        let mut effects = Effects::default();
        effects.both(
            self.event(&task, "fish_failed", data.clone()),
            self.elephant_event(&task, "fish_failed", message.clone(), data),
        );
        self.commit(&task, effects);
        Rejection::new("R5", message)
    }

    /// Contexto de las plantillas de `ez-team` para este pez.
    fn instruction_context(
        &self,
        task: &crate::state::Task,
        role: Role,
        key: &FishKey,
        round: u32,
        fish_brief: PathBuf,
        report_path: &Path,
    ) -> Result<InstructionContext, Rejection> {
        let qa_report = task
            .fish_in_round(round)
            .find(|record| record.role == Role::Qa)
            .and_then(|record| {
                task.work
                    .iter()
                    .find(|item| item.key == record.key)
                    .map(|item| item.report_path.clone())
            });
        let task_dir = paths::task_dir(&task.space.folder, &task.id);
        Ok(InstructionContext {
            role,
            fish: key.fish.clone(),
            round,
            folder: task.space.folder.clone(),
            brief: task_dir.join("encargo.md"),
            criteria: task_dir.join("criterios.toml"),
            plan: task_dir.join("plan.md"),
            fish_brief,
            report: report_path.to_path_buf(),
            qa_report,
            min_mutations: self.load_criteria(task)?.validacion.mutaciones_minimas,
        })
    }

    /// Renderiza y guarda `<pez>.instrucciones.md`.
    fn write_instructions(
        &self,
        context: &InstructionContext,
        path: &Path,
        fish: &str,
    ) -> Result<(), Rejection> {
        let rendered = self.writer.render(context).map_err(|detail| {
            Rejection::new(
                "R5",
                format!("No se pudieron escribir las instrucciones de {fish}: {detail}."),
            )
        })?;
        std::fs::write(path, rendered).map_err(|error| {
            Rejection::new(
                "R5",
                format!("No se pudo guardar el archivo de instrucciones: {error}."),
            )
        })
    }

    /// Identidad, etiquetas y primer mensaje del pez.
    fn launch_request(
        &self,
        task: &crate::state::Task,
        role: Role,
        seat: &ez_team::Seat,
        key: &FishKey,
        instructions: &Path,
        token: &str,
    ) -> crate::types::LaunchRequest {
        let mut env = std::collections::BTreeMap::new();
        env.insert("EZ_ROLE".to_string(), "fish".to_string());
        env.insert("EZ_FISH".to_string(), key.fish.clone());
        env.insert("EZ_TOKEN".to_string(), token.to_string());
        let mut labels = std::collections::BTreeMap::new();
        labels.insert("ez".to_string(), "1".to_string());
        labels.insert("ez_space".to_string(), task.space.id.clone());
        labels.insert("ez_task".to_string(), task.id.clone());
        labels.insert("ez_role".to_string(), role.id().to_string());
        labels.insert("ez_fish".to_string(), key.fish.clone());
        labels.insert("ez_attempt".to_string(), key.attempt.to_string());
        crate::types::LaunchRequest {
            fish: key.fish.clone(),
            hc_name: hc_name(&task.id, &key.fish, key.attempt),
            role,
            seat: seat.clone(),
            cwd: task.space.folder.clone(),
            env,
            labels,
            first_line: self.writer.first_line(role, instructions),
            token: token.to_string(),
        }
    }

    /// Fases de la ronda y límite de intentos (§14.3).
    fn check_order(
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
            && task.fish_in_round(round).filter(|fish| fish.role == role).any(|fish| {
                task.work_for_key(&fish.key)
                    .is_some_and(|item| item.state != WorkState::Failed)
            });
        if live {
            return Err(Rejection::new("R5", texts::already_in_round(role.id())));
        }
        match role {
            Role::PlanReviewer => {
                if !self.role_passed(task, Role::Planner, round) {
                    return Err(Rejection::new("R5", texts::waiting_for(&[format!("plan-r{round}")])));
                }
            }
            Role::Developer => {
                if round == 1 && !self.role_passed(task, Role::Planner, round) {
                    return Err(Rejection::new("R5", texts::waiting_for(&[format!("plan-r{round}")])));
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
                    .filter(|fish| {
                        fish.role == Role::Developer && self.launcher.alive(&fish.terminal)
                    })
                    .count() as u32;
                if alive >= count.max(1) {
                    return Err(Rejection::new(
                        "R5",
                        texts::too_many_developers(alive, count.max(1)),
                    ));
                }
            }
            Role::Qa => self.check_qa_order(task, round)?,
            Role::QaValidator => {
                let qa_reported = task
                    .fish_in_round(round)
                    .filter(|fish| fish.role == Role::Qa)
                    .any(|fish| fish.report.is_some());
                if !qa_reported {
                    return Err(Rejection::new("R5", texts::waiting_for(&[format!("qa-r{round}")])));
                }
            }
            Role::Planner => {}
        }
        Ok(())
    }

    /// ¿Informó `pass` ese papel en esta ronda?
    fn role_passed(&self, task: &crate::state::Task, role: Role, round: u32) -> bool {
        task.fish_in_round(round)
            .filter(|fish| fish.role == role)
            .any(|fish| fish.report.as_ref().is_some_and(|report| report.verdict == Verdict::Pass))
    }

    /// El QA espera a los desarrolladores, salvo en una ronda solo de verificación.
    fn check_qa_order(&self, task: &crate::state::Task, round: u32) -> Result<(), Rejection> {
        let developers: Vec<_> = task
            .fish_in_round(round)
            .filter(|fish| fish.role == Role::Developer)
            .collect();
        if developers.is_empty() && round == 1 {
            return Err(Rejection::new("R5", texts::waiting_for(&[format!("dev-r{round}-1")])));
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
        // DECISION: hasta ECO-2c se compara la foto de git; el brief prevé
        // cambiarla por la huella de contenido en esa entrega.
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
        if task.fish_in_round(record.round).any(|other| other.role == Role::Qa) {
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
        let number = record.corrections + 1;
        let suffix = attempt_suffix(record.key.attempt);
        let folder = paths::round_dir(&task.space.folder, &task.id, record.round);
        let destination = folder.join(format!("{fish}{suffix}.correccion-{number}.md"));
        let report_path = folder.join(format!("{fish}{suffix}.informe-c{number}.md"));
        paths::copy_file(&source, &destination).map_err(|error| {
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
            &task,
            "fish_corrected",
            serde_json::json!({ "fish": fish, "file": destination, "report": report_path }),
        ));
        self.commit(&task, effects);
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
        let expected = work.report_path.clone();
        let kind_tag = match work.kind {
            WorkKind::Launch => String::new(),
            WorkKind::Correction(number) => format!("-c{number}"),
        };
        let report_file = paths::inside(&task.space.folder, &input.file)
            .map_err(|_| Rejection::new("R7", texts::outside_folder(&input.file)))?;
        let expected = paths::inside(&task.space.folder, &expected)
            .map_err(|_| Rejection::new("R7", texts::wrong_report_path(&expected)))?;
        if report_file != expected {
            return Err(Rejection::new("R7", texts::wrong_report_path(&expected)));
        }
        if !report_file.is_file() {
            return Err(Rejection::new("R7", texts::report_missing(&input.file)));
        }
        let record = task
            .fish_by_key(key)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::FISH_NOT_ACTIVE))?;
        if record.role == Role::QaValidator
            && (input.mutations_tried.is_none() || input.mutations_caught.is_none())
        {
            return Err(Rejection::new("R7", texts::MUTATIONS_REQUIRED));
        }
        if input.verdict == Verdict::Pass && (input.blocking > 0 || input.major > 0) {
            return Err(Rejection::new("R7", texts::PASS_WITH_FINDINGS));
        }
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
        let changed = record
            .start
            .as_ref()
            .map(|start| git::changed(&task.space.folder, start, &after))
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
            self.event(&task, "fish_reported", data.clone()),
            self.elephant_event(&task, "fish_reported", message.clone(), data),
        );
        self.commit(&task, effects);
        Ok(Outcome::new(message))
    }
}
