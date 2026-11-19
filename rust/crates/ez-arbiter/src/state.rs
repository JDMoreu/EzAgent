//! Estado persistente de una tarea (`tareas/<tarea>/estado.json`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ez_team::{Role, Seat};
use serde::{Deserialize, Serialize};

use crate::git::GitSnapshot;
use crate::types::{Space, Verdict};
use crate::verify::CheckRecord;

/// Estado de la tarea (diseño §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    PendingApproval,
    Approved,
    InRound,
    BetweenRounds,
    Paused,
    Done,
    Stopped,
    Rejected,
}

impl Status {
    /// Id del estado en el protocolo y en `estado.json`.
    pub fn id(self) -> &'static str {
        match self {
            Status::PendingApproval => "pending_approval",
            Status::Approved => "approved",
            Status::InRound => "in_round",
            Status::BetweenRounds => "between_rounds",
            Status::Paused => "paused",
            Status::Done => "done",
            Status::Stopped => "stopped",
            Status::Rejected => "rejected",
        }
    }

    /// Estados finales: no se recargan y no admiten comandos.
    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Done | Status::Stopped | Status::Rejected)
    }
}

/// Aprobación del equipo, independiente de la de la tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    None,
    Proposed,
    Approved,
    Rejected,
}

impl TeamStatus {
    /// Id del estado del equipo.
    pub fn id(self) -> &'static str {
        match self {
            TeamStatus::None => "none",
            TeamStatus::Proposed => "proposed",
            TeamStatus::Approved => "approved",
            TeamStatus::Rejected => "rejected",
        }
    }
}

/// Informe de un pez (R7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportRecord {
    pub verdict: Verdict,
    pub file: PathBuf,
    pub blocking: u32,
    pub major: u32,
    pub minor: u32,
    pub mutations_tried: Option<u32>,
    pub mutations_caught: Option<u32>,
}

/// Pez lanzado en una ronda.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FishRecord {
    pub id: String,
    pub role: Role,
    pub terminal: String,
    pub round: u32,
    /// Tiene trabajo entregado y todavía no informó en esta ronda.
    pub pending: bool,
    pub reported: bool,
    /// Foto de git al empezar su trabajo (para atribuir archivos).
    pub start: Option<GitSnapshot>,
    pub report: Option<ReportRecord>,
    pub corrections: u32,
}

/// Una ronda con sus verificaciones, motivos y atribución de archivos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundRecord {
    pub number: u32,
    pub opened: f64,
    pub closed: Option<f64>,
    pub clean: bool,
    pub start: GitSnapshot,
    pub close: Option<GitSnapshot>,
    pub verification: Vec<CheckRecord>,
    pub reasons: Vec<String>,
    /// `role.id()` → rutas que cambiaron mientras ese papel trabajaba.
    pub attribution: BTreeMap<String, BTreeSet<String>>,
}

impl RoundRecord {
    /// Suma las rutas atribuidas a un papel.
    pub fn attribute(&mut self, role: Role, paths: &[String]) {
        self.attribution
            .entry(role.id().to_string())
            .or_default()
            .extend(paths.iter().cloned());
    }

    /// Rutas atribuidas a un papel.
    pub fn files_of(&self, role: Role) -> BTreeSet<String> {
        self.attribution.get(role.id()).cloned().unwrap_or_default()
    }
}

/// Una tarea del espacio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub space: Space,
    pub status: Status,
    pub paused_from: Option<Status>,
    /// Código del motivo (`tope_rondas`, `criterios_modificados`…).
    pub pause_reason: Option<String>,
    pub pause_message: Option<String>,
    pub created: f64,
    pub approved_at: Option<f64>,
    pub team: Vec<Seat>,
    pub team_status: TeamStatus,
    pub team_card: String,
    pub round: u32,
    pub clean_streak: u32,
    /// sha256 de los criterios congelados al aprobar.
    pub criteria_sha: String,
    /// Foto de git al cerrar la última ronda (para R11).
    pub last_close: Option<GitSnapshot>,
    pub rounds: Vec<RoundRecord>,
    pub fish: Vec<FishRecord>,
}

impl Task {
    /// Tarea en curso: no está terminada ni rechazada.
    pub fn is_active(&self) -> bool {
        !self.status.is_terminal()
    }

    /// Ficha de un pez.
    pub fn fish(&self, id: &str) -> Option<&FishRecord> {
        self.fish.iter().find(|fish| fish.id == id)
    }

    /// Ficha mutable de un pez.
    pub fn fish_mut(&mut self, id: &str) -> Option<&mut FishRecord> {
        self.fish.iter_mut().find(|fish| fish.id == id)
    }

    /// Puesto del equipo de un papel.
    pub fn seat(&self, role: Role) -> Option<&Seat> {
        self.team.iter().find(|seat| seat.role == role)
    }

    /// Peces lanzados en una ronda.
    pub fn fish_in_round(&self, round: u32) -> impl Iterator<Item = &FishRecord> {
        self.fish.iter().filter(move |fish| fish.round == round)
    }

    /// Ronda abierta, mutable.
    pub fn open_round_mut(&mut self) -> Option<&mut RoundRecord> {
        self.rounds
            .iter_mut()
            .find(|record| record.closed.is_none())
    }

    /// Pone la tarea en pausa recordando de dónde venía.
    pub fn pause(&mut self, code: &str, message: String) {
        if self.status != Status::Paused {
            self.paused_from = Some(self.status);
        }
        self.status = Status::Paused;
        self.pause_reason = Some(code.to_string());
        self.pause_message = Some(message);
    }

    /// Archivos atribuidos al QA en rondas anteriores a `round`.
    pub fn protected_files(&self, round: u32) -> BTreeSet<String> {
        let mut files = BTreeSet::new();
        for record in &self.rounds {
            if record.number < round {
                files.extend(record.files_of(Role::Qa));
            }
        }
        files
    }

    /// Estado para la vista (`snapshot`).
    pub fn snapshot(&self) -> serde_json::Value {
        let fish: Vec<serde_json::Value> = self
            .fish
            .iter()
            .map(|fish| {
                serde_json::json!({
                    "id": fish.id,
                    "role": fish.role.id(),
                    "terminal": fish.terminal,
                    "round": fish.round,
                    "pending": fish.pending,
                    "reported": fish.reported,
                })
            })
            .collect();
        let rounds: Vec<serde_json::Value> = self
            .rounds
            .iter()
            .map(|record| {
                serde_json::json!({
                    "number": record.number,
                    "clean": record.clean,
                    "closed": record.closed,
                    "reasons": record.reasons,
                })
            })
            .collect();
        serde_json::json!({
            "space": self.space.id,
            "space_name": self.space.name,
            "task": self.id,
            "title": self.title,
            "status": self.status.id(),
            "round": self.round,
            "clean_streak": self.clean_streak,
            "paused_from": self.paused_from.map(Status::id),
            "pause_reason": self.pause_reason,
            "pause_message": self.pause_message,
            "team": { "status": self.team_status.id(), "seats": self.team },
            "fish": fish,
            "rounds": rounds,
        })
    }
}
