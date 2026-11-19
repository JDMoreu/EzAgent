//! R5, R6 y R7: lanzar peces, corregirlos y recibir informes.

use std::path::PathBuf;

use ez_team::{InstructionContext, Role};

use crate::git;
use crate::paths;
use crate::state::{ReportRecord, Status};
use crate::texts;
use crate::types::{Outcome, Rejection, Verdict};

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

impl Arbiter {
    /// R5: valida el orden del papel, escribe las instrucciones y lanza el pez.
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
        let brief_abs = paths::resolve_inside(&task.space.folder, &brief)
            .ok_or_else(|| Rejection::new("R5", texts::outside_folder(&brief)))?;
        if !brief_abs.is_file() {
            return Err(Rejection::new("R5", texts::fish_brief_missing(&brief)));
        }
        let index = task
            .fish_in_round(round)
            .filter(|fish| fish.role == Role::Developer)
            .count() as u32
            + 1;
        let fish = fish_id(role, round, index);
        let folder = paths::round_dir(&task.space.folder, &task.id, round);
        let fish_brief = folder.join(format!("{fish}.encargo.md"));
        paths::copy_file(&brief_abs, &fish_brief).map_err(|error| {
            Rejection::new(
                "R5",
                format!("No se pudo copiar el encargo del pez: {error}."),
            )
        })?;
        let start =
            git::snapshot(&task.space.folder).map_err(|message| Rejection::new("R5", message))?;
        let instructions = folder.join(format!("{fish}.instrucciones.md"));
        let qa_report = task
            .fish_in_round(round)
            .find(|record| record.role == Role::Qa)
            .map(|record| folder.join(format!("{}.informe.md", record.id)));
        let context = InstructionContext {
            role,
            fish: fish.clone(),
            round,
            folder: task.space.folder.clone(),
            brief: paths::task_dir(&task.space.folder, &task.id).join("encargo.md"),
            criteria: paths::task_dir(&task.space.folder, &task.id).join("criterios.toml"),
            plan: paths::task_dir(&task.space.folder, &task.id).join("plan.md"),
            fish_brief,
            report: folder.join(format!("{fish}.informe.md")),
            qa_report,
            min_mutations: self.load_criteria(&task)?.validacion.mutaciones_minimas,
        };
        let rendered = self.writer.render(&context).map_err(|detail| {
            Rejection::new(
                "R5",
                format!("No se pudieron escribir las instrucciones de {fish}: {detail}."),
            )
        })?;
        std::fs::write(&instructions, rendered).map_err(|error| {
            Rejection::new(
                "R5",
                format!("No se pudo guardar el archivo de instrucciones: {error}."),
            )
        })?;
        let token = new_token();
        let mut env = std::collections::BTreeMap::new();
        env.insert("EZ_ROLE".to_string(), "fish".to_string());
        env.insert("EZ_FISH".to_string(), fish.clone());
        env.insert("EZ_TOKEN".to_string(), token.clone());
        let mut labels = std::collections::BTreeMap::new();
        labels.insert("ez".to_string(), "1".to_string());
        labels.insert("ez_space".to_string(), task.space.id.clone());
        labels.insert("ez_task".to_string(), task.id.clone());
        labels.insert("ez_role".to_string(), role.id().to_string());
        labels.insert("ez_fish".to_string(), fish.clone());
        let request = crate::types::LaunchRequest {
            fish: fish.clone(),
            hc_name: hc_name(&task.id, &fish),
            role,
            seat: seat.clone(),
            cwd: task.space.folder.clone(),
            env,
            labels,
            first_line: self.writer.first_line(role, &instructions),
        };
        let terminal = self
            .launcher
            .launch(&request)
            .map_err(|detail| Rejection::new("R5", texts::launch_failed(&fish, &detail)))?;
        task.fish.push(crate::state::FishRecord {
            id: fish.clone(),
            role,
            terminal: terminal.clone(),
            round,
            pending: true,
            reported: false,
            start: Some(start),
            report: None,
            corrections: 0,
        });
        let message = texts::fish_launched(&fish, &terminal);
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "fish_launched",
            serde_json::json!({
                "fish": fish, "role": role.id(), "terminal": terminal, "token": token,
            }),
        ));
        self.commit(&task, effects);
        Ok(Outcome {
            message,
            data: serde_json::json!({ "fish": fish, "terminal": terminal }),
        })
    }

    /// Orden de papeles y límite de peces vivos en una ronda (R5).
    fn check_order(
        &self,
        task: &crate::state::Task,
        role: Role,
        round: u32,
        count: u32,
    ) -> Result<(), Rejection> {
        if role.is_verifier() && task.fish_in_round(round).any(|fish| fish.role == role) {
            return Err(Rejection::new("R5", texts::already_in_round(role.id())));
        }
        match role {
            Role::Qa => {
                let waiting: Vec<String> = task
                    .fish_in_round(round)
                    .filter(|fish| fish.role == Role::Developer && fish.pending)
                    .map(|fish| fish.id.clone())
                    .collect();
                if !waiting.is_empty() {
                    return Err(Rejection::new("R5", texts::waiting_for(&waiting)));
                }
            }
            Role::QaValidator => {
                if !task
                    .fish_in_round(round)
                    .any(|fish| fish.role == Role::Qa && fish.report.is_some())
                {
                    return Err(Rejection::new(
                        "R5",
                        texts::waiting_for(&[format!("qa-r{round}")]),
                    ));
                }
            }
            Role::Developer => {
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
            Role::Planner | Role::PlanReviewer => {}
        }
        Ok(())
    }

    /// R6: corrección a un no verificador con la terminal viva.
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
        let source = paths::resolve_inside(&task.space.folder, &file)
            .ok_or_else(|| Rejection::new("R6", texts::outside_folder(&file)))?;
        if !source.is_file() {
            return Err(Rejection::new("R6", texts::correction_missing(&file)));
        }
        let number = record.corrections + 1;
        let destination = paths::round_dir(&task.space.folder, &task.id, record.round)
            .join(format!("{fish}.correccion-{number}.md"));
        paths::copy_file(&source, &destination).map_err(|error| {
            Rejection::new("R6", format!("No se pudo copiar la corrección: {error}."))
        })?;
        let line = texts::correction_line(record.round, &destination);
        self.launcher
            .send(&record.terminal, &line)
            .map_err(|detail| {
                Rejection::new(
                    "R6",
                    format!("No se pudo enviar la corrección a {fish}: {detail}."),
                )
            })?;
        if let Some(record) = task.fish_mut(fish) {
            record.pending = true;
            record.corrections = number;
        }
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "fish_corrected",
            serde_json::json!({ "fish": fish, "file": destination }),
        ));
        self.commit(&task, effects);
        Ok(Outcome::new(texts::correction_sent(fish)))
    }

    /// R7: recibe el informe, atribuye archivos y avisa al elefante.
    pub(crate) fn report(&self, fish: &str, input: ReportInput) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_fish(fish)?;
        if task.status != Status::InRound {
            return Err(Rejection::new("R7", texts::NO_ROUND));
        }
        self.report_allowed(&task, fish)?;
        let record = task
            .fish(fish)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_FISH))?;
        if record.role == Role::QaValidator
            && (input.mutations_tried.is_none() || input.mutations_caught.is_none())
        {
            return Err(Rejection::new("R7", texts::MUTATIONS_REQUIRED));
        }
        let report_file = paths::resolve_inside(&task.space.folder, &input.file)
            .ok_or_else(|| Rejection::new("R7", texts::outside_folder(&input.file)))?;
        if !report_file.is_file() {
            return Err(Rejection::new("R7", texts::report_missing(&input.file)));
        }
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
            blocking: input.blocking,
            major: input.major,
            minor: input.minor,
            mutations_tried: input.mutations_tried,
            mutations_caught: input.mutations_caught,
        };
        if let Some(round) = task.open_round_mut() {
            round.attribute(record.role, &changed);
        }
        if let Some(record) = task.fish_mut(fish) {
            record.pending = false;
            record.reported = true;
            record.report = Some(report);
        }
        let message = texts::report_received(fish, input.verdict);
        let data = serde_json::json!({
            "fish": fish, "verdict": input.verdict.id(), "file": report_file,
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
