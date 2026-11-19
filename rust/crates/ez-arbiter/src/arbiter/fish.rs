//! R5: fases, lanzamiento y entrega del encargo (diseño §14.2 y §14.3).

mod order;
mod work;

use std::path::{Path, PathBuf};

use ez_team::{InstructionContext, Role};

use crate::git;
use crate::paths;
use crate::state::{FishRecord, Status, WorkItem, WorkKind, WorkState};
use crate::texts;
use crate::types::{Delivery, FishKey, Launched, Outcome, Rejection, Verdict};

use super::emit::Effects;
use super::{fish_id, hc_name, new_token, ArbiterShared};

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
pub(crate) fn attempt_suffix(attempt: u32) -> String {
    if attempt > 1 {
        format!("-a{attempt}")
    } else {
        String::new()
    }
}

/// Todo lo que se necesita para entregar el encargo a un pez.
struct LaunchPlan {
    key: FishKey,
    fish: String,
    role: Role,
    report_path: PathBuf,
    start: git::Fingerprint,
    request: crate::types::LaunchRequest,
}

impl ArbiterShared {
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
        let seat = task
            .seat(role)
            .cloned()
            .ok_or_else(|| Rejection::new("R5", texts::role_not_in_team(role.id())))?;
        self.check_order(&task, role, task.round, seat.count)?;
        if let Err(rejection) = self.check_artifacts(&task) {
            return Err(self.pause_task(task, "R3", "artefactos_modificados", rejection.message));
        }
        if task.attempts(role, task.round) >= 3 {
            return Err(Rejection::new("R5", texts::too_many_attempts(role.id())));
        }
        let plan = self.prepare_launch(&task, role, &seat, brief)?;
        self.deliver(&mut task, plan)
    }

    /// Identidad, archivos de la ronda y petición de lanzamiento.
    fn prepare_launch(
        &self,
        task: &crate::state::Task,
        role: Role,
        seat: &ez_team::Seat,
        brief: PathBuf,
    ) -> Result<LaunchPlan, Rejection> {
        let round = task.round;
        let attempt = task.attempts(role, round) + 1;
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
        let start = self.copy_fish_brief(task, &brief, &fish_brief)?;
        let context =
            self.instruction_context(task, role, &key, round, fish_brief, &report_path)?;
        self.write_instructions(&context, &instructions, &fish)?;
        let token = new_token();
        Ok(LaunchPlan {
            request: self.launch_request(task, role, seat, &key, &instructions, &token),
            key,
            fish,
            role,
            report_path,
            start,
        })
    }

    /// Ejecuta la entrega, registra el trabajo y emite la traza (§14.2).
    fn deliver(
        &self,
        task: &mut crate::state::Task,
        plan: LaunchPlan,
    ) -> Result<Outcome, Rejection> {
        let token = plan.request.token.clone();
        let launched = self.launcher.launch(&plan.request);
        let (terminal, mut message) = match launched {
            Ok(Launched {
                terminal,
                delivery: Delivery::Delivered,
            }) => (
                terminal.clone(),
                texts::fish_launched(&plan.fish, &terminal),
            ),
            Ok(Launched {
                terminal,
                delivery: Delivery::Unconfirmed,
            }) => (
                terminal.clone(),
                format!(
                    "{}{}",
                    texts::fish_launched(&plan.fish, &terminal),
                    texts::delivery_unconfirmed(&plan.fish)
                ),
            ),
            Ok(Launched {
                delivery: Delivery::Failed(detail),
                ..
            }) => return Err(self.fail_launch(task, &plan, &detail)),
            Err(detail) => return Err(self.fail_launch(task, &plan, &detail)),
        };
        self.remember_token(&plan.key, &token);
        task.fish.push(FishRecord {
            key: plan.key.clone(),
            role: plan.role,
            terminal: terminal.clone(),
            round: task.round,
            start: Some(plan.start),
            report: None,
            corrections: 0,
        });
        task.work.push(WorkItem {
            key: plan.key.clone(),
            round: task.round,
            kind: WorkKind::Launch,
            report_path: plan.report_path.clone(),
            state: WorkState::Pending,
            started_ts: self.now(),
        });
        let mut effects = Effects::default();
        effects.event(self.event(
            task,
            "fish_launched",
            serde_json::json!({
                "fish": plan.fish, "role": plan.role.id(), "terminal": terminal,
                "attempt": plan.key.attempt,
            }),
        ));
        self.commit(task, effects);
        message.push_str(&format!(
            " Informa con --file \"{}\".",
            plan.report_path.display()
        ));
        Ok(Outcome {
            message,
            data: serde_json::json!({
                "fish": plan.fish, "terminal": terminal, "attempt": plan.key.attempt,
            }),
        })
    }

    /// Copia el encargo del pez y toma la foto de git antes de lanzarlo.
    fn copy_fish_brief(
        &self,
        task: &crate::state::Task,
        brief: &Path,
        fish_brief: &Path,
    ) -> Result<git::Fingerprint, Rejection> {
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
        task: &mut crate::state::Task,
        plan: &LaunchPlan,
        detail: &str,
    ) -> Rejection {
        self.launcher.revoke(&plan.request.token);
        task.fish.push(FishRecord {
            key: plan.key.clone(),
            role: plan.request.role,
            terminal: String::new(),
            round: task.round,
            start: None,
            report: None,
            corrections: 0,
        });
        task.work.push(WorkItem {
            key: plan.key.clone(),
            round: task.round,
            kind: WorkKind::Launch,
            report_path: plan.report_path.clone(),
            state: WorkState::Failed,
            started_ts: self.now(),
        });
        let message = texts::delivery_failed(&plan.fish, detail);
        let data = serde_json::json!({
            "fish": plan.fish, "attempt": plan.key.attempt, "reason": detail,
        });
        let mut effects = Effects::default();
        effects.both(
            self.event(task, "fish_failed", data.clone()),
            self.elephant_event(task, "fish_failed", message.clone(), data),
        );
        self.commit(task, effects);
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
}
