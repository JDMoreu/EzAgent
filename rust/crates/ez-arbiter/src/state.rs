//! Estado persistente de una tarea (`tareas/<tarea>/estado.json`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ez_team::{RoleDef, RoleId, Seat};
use serde::{Deserialize, Serialize};

use crate::git::Fingerprint;
use crate::types::{FishKey, Space, Verdict};
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
    /// sha256 de la copia privada del informe.
    #[serde(default)]
    pub sha: String,
    pub blocking: u32,
    pub major: u32,
    pub minor: u32,
    pub mutations_tried: Option<u32>,
    pub mutations_caught: Option<u32>,
}

/// Unidad de trabajo entregada a un pez (diseño §14.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkItem {
    pub key: FishKey,
    pub round: u32,
    pub kind: WorkKind,
    /// Ruta exacta del informe que debe enviar el pez.
    pub report_path: PathBuf,
    pub state: WorkState,
    /// Segundos desde 1970 en que se entregó el trabajo.
    pub started_ts: f64,
}

/// Origen del trabajo de un pez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Launch,
    Correction(u32),
}

/// Estado de una unidad de trabajo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Pending,
    Reported,
    Failed,
}

/// Sesión de un pez lanzado en una ronda (una por intento).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FishRecord {
    pub key: FishKey,
    pub role: RoleId,
    pub terminal: String,
    pub round: u32,
    /// Huella del proyecto al empezar su trabajo (para atribuir archivos).
    pub start: Option<Fingerprint>,
    /// Último informe aceptado.
    pub report: Option<ReportRecord>,
    pub corrections: u32,
}

impl FishRecord {
    /// Id del pez (`qa-r1`), sin el sufijo de intento.
    pub fn id(&self) -> &str {
        &self.key.fish
    }
}

/// Una ronda con sus verificaciones, motivos y atribución de archivos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundRecord {
    pub number: u32,
    pub opened: f64,
    pub closed: Option<f64>,
    pub clean: bool,
    pub start: Fingerprint,
    pub close: Option<Fingerprint>,
    pub verification: Vec<CheckRecord>,
    pub reasons: Vec<String>,
    /// `role.id()` → rutas que cambiaron mientras ese papel trabajaba.
    pub attribution: BTreeMap<String, BTreeSet<String>>,
}

impl RoundRecord {
    /// Suma las rutas atribuidas a un papel.
    pub fn attribute(&mut self, role: &RoleId, paths: &[String]) {
        self.attribution
            .entry(role.as_str().to_string())
            .or_default()
            .extend(paths.iter().cloned());
    }

    /// Rutas atribuidas a un papel.
    pub fn files_of(&self, role: &RoleId) -> BTreeSet<String> {
        self.attribution
            .get(role.as_str())
            .cloned()
            .unwrap_or_default()
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
    /// Definiciones de los papeles del equipo, congeladas con él.
    #[serde(default)]
    pub roles: Vec<RoleDef>,
    pub team_status: TeamStatus,
    pub team_card: String,
    pub round: u32,
    pub clean_streak: u32,
    /// sha256 de los criterios congelados al aprobar.
    pub criteria_sha: String,
    /// sha256 del encargo congelado al aprobar.
    #[serde(default)]
    pub encargo_sha: String,
    /// sha256 del equipo congelado al aprobarlo.
    #[serde(default)]
    pub team_sha: Option<String>,
    /// Huella al cerrar la última ronda (para R11).
    #[serde(default)]
    pub last_close: Option<Fingerprint>,
    /// Huella al cerrar la última ronda limpia (ronda de verificación y cierre).
    #[serde(default)]
    pub last_clean_close: Option<Fingerprint>,
    /// Última huella registrada (detección de cambios del elefante).
    #[serde(default)]
    pub last_fp: Option<Fingerprint>,
    /// Margen extra de rondas y horas concedido con `more_time` (§14.5).
    #[serde(default)]
    pub extra_rounds: u32,
    #[serde(default)]
    pub extra_hours: f64,
    /// Segundos acumulados en pausas y momento en que empezó la actual.
    #[serde(default)]
    pub paused_total: f64,
    #[serde(default)]
    pub paused_at: Option<f64>,
    pub rounds: Vec<RoundRecord>,
    pub fish: Vec<FishRecord>,
    /// Trabajos pendientes, informados o fallidos de la tarea.
    #[serde(default)]
    pub work: Vec<WorkItem>,
}

impl Task {
    /// Tarea en curso: no está terminada ni rechazada.
    pub fn is_active(&self) -> bool {
        !self.status.is_terminal()
    }

    /// Definición congelada de un papel del equipo.
    pub fn role_def(&self, role: &RoleId) -> Option<&RoleDef> {
        self.roles.iter().find(|def| def.id == *role)
    }

    /// Etapa de un papel: la del puesto o la sugerida por su definición.
    pub fn stage_of(&self, role: &RoleId) -> u32 {
        self.seat(role)
            .map(|seat| seat.stage)
            .or_else(|| self.role_def(role).map(|def| def.default_stage))
            .unwrap_or(u32::MAX)
    }

    /// ¿Es verificador este papel?
    pub fn is_verifier(&self, role: &RoleId) -> bool {
        self.role_def(role).is_some_and(RoleDef::is_verifier)
    }

    /// ¿Escribe pruebas este papel (sus archivos quedan protegidos)?
    pub fn is_writes_tests(&self, role: &RoleId) -> bool {
        self.role_def(role).is_some_and(|def| def.writes_tests)
    }

    /// ¿Pide mutaciones este papel?
    pub fn has_mutations(&self, role: &RoleId) -> bool {
        self.role_def(role).is_some_and(|def| def.mutations)
    }

    /// Ficha de un pez por su id (se queda con el último intento).
    pub fn fish(&self, id: &str) -> Option<&FishRecord> {
        self.fish.iter().rev().find(|fish| fish.id() == id)
    }

    /// Ficha de un pez por su clave completa.
    pub fn fish_by_key(&self, key: &FishKey) -> Option<&FishRecord> {
        self.fish.iter().find(|fish| &fish.key == key)
    }

    /// Ficha mutable de un pez por su clave completa.
    pub fn fish_by_key_mut(&mut self, key: &FishKey) -> Option<&mut FishRecord> {
        self.fish.iter_mut().find(|fish| &fish.key == key)
    }

    /// Trabajo vigente (pendiente o informado) de una clave.
    pub fn work_for_key(&self, key: &FishKey) -> Option<&WorkItem> {
        self.work.iter().find(|item| &item.key == key)
    }

    /// Trabajo pendiente de una clave.
    pub fn pending_work(&self, key: &FishKey) -> Option<&WorkItem> {
        self.work
            .iter()
            .find(|item| &item.key == key && item.state == WorkState::Pending)
    }

    /// Trabajos pendientes de una ronda.
    pub fn pending_in_round(&self, round: u32) -> Vec<&WorkItem> {
        self.work
            .iter()
            .filter(|item| item.round == round && item.state == WorkState::Pending)
            .collect()
    }

    /// Intentos de un papel en una ronda (peces lanzados).
    pub fn attempts(&self, role: &RoleId, round: u32) -> u32 {
        self.fish
            .iter()
            .filter(|fish| fish.role == *role && fish.round == round)
            .count() as u32
    }

    /// Puesto del equipo de un papel.
    pub fn seat(&self, role: &RoleId) -> Option<&Seat> {
        self.team.iter().find(|seat| seat.role == *role)
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

    /// Pone la tarea en pausa recordando de dónde venía y cuándo empezó (§14.5).
    pub fn pause(&mut self, code: &str, message: String, now: f64) {
        if self.status != Status::Paused {
            self.paused_from = Some(self.status);
            self.paused_at = Some(now);
        }
        self.status = Status::Paused;
        self.pause_reason = Some(code.to_string());
        self.pause_message = Some(message);
    }

    /// Reanuda contando solo el tiempo fuera de pausa (§14.5).
    pub fn resume(&mut self, now: f64) {
        if let Some(started) = self.paused_at.take() {
            self.paused_total += (now - started).max(0.0);
        }
        self.status = self.paused_from.take().unwrap_or(Status::Approved);
        self.pause_reason = None;
        self.pause_message = None;
    }

    /// Archivos atribuidos al QA en rondas anteriores a `round`.
    // DECISION: se compara por ruta exacta; el cambio de contenido de un
    // archivo sin seguimiento no aparece en `git status` y no se atribuye.
    pub fn protected_files(&self, round: u32) -> BTreeSet<String> {
        let mut files = BTreeSet::new();
        for record in &self.rounds {
            if record.number < round {
                for role in self.roles.iter().filter(|def| def.writes_tests) {
                    files.extend(record.files_of(&role.id));
                }
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
                    "id": fish.id(),
                    "role": fish.role.as_str(),
                    "terminal": fish.terminal,
                    "round": fish.round,
                    "attempt": fish.key.attempt,
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
