//! Estado del modo autónomo y sus transiciones (9.3 y 9.5).
//!
//! Son funciones puras: no tocan Herdr, ni ventanas, ni el disco. La app (A3)
//! ejecuta la acción devuelta fuera de cualquier cerrojo.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::texts;
use super::verdict::{is_churn, Verdict, VerdictError};

/// `mode` de `estado.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Single,
    Team,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Team => "team",
        }
    }
}

/// `state` de `estado.json` (el bucle de 9.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Preparing,
    Running,
    Verifying,
    Done,
    Paused,
    Stopped,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::Verifying => "verifying",
            Self::Done => "done",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    /// Ni cumplida ni detenida: todavía hay algo que hacer.
    pub fn unfinished(self) -> bool {
        !matches!(self, Self::Done | Self::Stopped)
    }
}

/// `executor` de `estado.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Executor {
    pub address: String,
    pub agent: String,
}

/// Un papel del equipo (9.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamRole {
    pub role: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// `team` de `estado.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub swarm: String,
    pub roles: Vec<TeamRole>,
}

/// Estado completo, tal como se escribe en `estado.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutonomousState {
    pub id: String,
    pub mode: Mode,
    pub state: TaskState,
    pub round: i64,
    pub started_at: f64,
    pub deadline: f64,
    pub max_rounds: i64,
    pub approvals: i64,
    pub scope: Vec<String>,
    pub executor: Option<Executor>,
    pub team: Option<Team>,
    pub clean_streak: i64,
    pub last_findings: Vec<String>,
    pub pause_reason: Option<String>,
    /// Intentos fallidos con el veredicto de la ronda en curso (no va a disco).
    #[serde(skip)]
    pub verify_failures: i64,
    /// Errores seguidos del ejecutor (no va a disco).
    #[serde(skip)]
    pub executor_errors: i64,
}

/// Lo que hay que hacer después de cada transición.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Lanzar el ejecutor con `encargo.md`.
    LaunchExecutor,
    /// Lanzar un verificador nuevo.
    LaunchVerifier,
    /// Mandar al ejecutor la ruta de `correccion-N.md`.
    SendCorrection(String),
    /// Avisar por voz y esperar al usuario.
    Pause(String),
    /// El usuario dijo «sigue».
    Resume,
    /// Dos rondas limpias seguidas.
    Done,
    /// El usuario dijo «para».
    Stopped,
    /// Nada que hacer con este evento.
    Wait,
}

/// Incidente que puede pausar el bucle (9.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incident {
    Quota,
    ContextFull,
    WindowClosed,
    ExecutorError,
    /// Coincidió con la lista de denegación; el texto es el motivo.
    Denied(String),
    /// Diálogo de permiso que no se supo clasificar.
    Dialog(String),
}

impl Incident {
    pub fn reason(&self) -> String {
        match self {
            Self::Quota => texts::REASON_QUOTA.to_string(),
            Self::ContextFull => texts::REASON_CONTEXT.to_string(),
            Self::WindowClosed => texts::REASON_WINDOW.to_string(),
            Self::ExecutorError => texts::REASON_EXECUTOR.to_string(),
            Self::Denied(reason) => reason.clone(),
            Self::Dialog(detail) => {
                format!("no supe qué responder a un diálogo del agente: {detail}")
            }
        }
    }
}

/// Resumen para `autonomous_status` y la vista rápida (9.2 y 9.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: String,
    pub mode: Mode,
    pub state: TaskState,
    pub round: i64,
    pub max_rounds: i64,
    pub approvals: i64,
    pub clean_streak: i64,
    pub pause_reason: Option<String>,
}

impl Summary {
    pub fn of(state: &AutonomousState) -> Self {
        Self {
            id: state.id.clone(),
            mode: state.mode,
            state: state.state,
            round: state.round,
            max_rounds: state.max_rounds,
            approvals: state.approvals,
            clean_streak: state.clean_streak,
            pause_reason: state.pause_reason.clone(),
        }
    }

    /// Fila de la vista rápida: «Autónomo · ronda {n} · {estado}» (9.8).
    pub fn describe(&self) -> String {
        if self.state == TaskState::Paused {
            return match &self.pause_reason {
                Some(reason) => format!("Autónomo · ronda {} · en pausa: {reason}", self.round),
                None => format!("Autónomo · ronda {} · en pausa", self.round),
            };
        }
        format!(
            "Autónomo · ronda {} · {}",
            self.round,
            texts::state_label(self.state)
        )
    }
}

impl AutonomousState {
    /// Tarea recién activada, antes de que el ejecutor exista.
    pub fn new(id: &str, mode: Mode, now: f64, max_rounds: i64, max_minutes: i64) -> Self {
        Self {
            id: id.to_string(),
            mode,
            state: TaskState::Preparing,
            round: 1,
            started_at: now,
            deadline: now + minutes(max_minutes),
            max_rounds,
            approvals: 0,
            scope: Vec::new(),
            executor: None,
            team: None,
            clean_streak: 0,
            last_findings: Vec::new(),
            pause_reason: None,
            verify_failures: 0,
            executor_errors: 0,
        }
    }

    pub fn unfinished(&self) -> bool {
        self.state.unfinished()
    }

    /// El ejecutor ya está abierto: preparando → ejecutando y a `scope`.
    pub fn executor_started(&mut self, address: &str, agent: &str) -> Action {
        if !matches!(self.state, TaskState::Preparing | TaskState::Running) {
            return Action::Wait;
        }
        self.cover(address);
        self.executor = Some(Executor {
            address: address.to_string(),
            agent: agent.to_string(),
        });
        self.state = TaskState::Running;
        Action::Wait
    }

    /// El verificador abrió: su dirección también entra en `scope`.
    pub fn verifier_started(&mut self, address: &str) {
        self.cover(address);
    }

    /// Suma una dirección a las cubiertas por la autorización, sin repetir.
    pub fn cover(&mut self, address: &str) {
        if !address.is_empty() && !self.scope.iter().any(|known| known == address) {
            self.scope.push(address.to_string());
        }
    }

    /// La terminó: se saca de `scope` (9.5).
    pub fn forget(&mut self, address: &str) {
        self.scope.retain(|known| known != address);
    }

    /// El ejecutor terminó (`done` o `idle` tras `working`): toca verificar.
    pub fn on_executor_finished(&mut self, now: f64) -> Action {
        if !matches!(self.state, TaskState::Preparing | TaskState::Running) {
            return Action::Wait;
        }
        self.executor_errors = 0;
        if now > self.deadline {
            return self.pause(texts::REASON_DEADLINE);
        }
        self.state = TaskState::Verifying;
        self.verify_failures = 0;
        Action::LaunchVerifier
    }

    /// Llegó (o falló) el veredicto del verificador de la ronda.
    pub fn on_verdict(
        &mut self,
        verdict: Result<Verdict, VerdictError>,
        folder: &Path,
        now: f64,
    ) -> Action {
        if self.state != TaskState::Verifying {
            return Action::Wait;
        }
        if now > self.deadline {
            return self.pause(texts::REASON_DEADLINE);
        }
        let verdict = match verdict {
            Ok(verdict) => {
                self.verify_failures = 0;
                verdict
            }
            Err(_) => {
                self.verify_failures += 1;
                if self.verify_failures >= 2 {
                    return self.pause(texts::REASON_VERDICT);
                }
                return Action::LaunchVerifier;
            }
        };
        if verdict.clean() {
            self.clean_streak += 1;
            // El churn se mide contra lo corregido en la ronda anterior: una
            // ronda limpia la deja sin hallazgos pendientes.
            self.last_findings.clear();
            if self.clean_streak >= 2 {
                self.state = TaskState::Done;
                return Action::Done;
            }
            return Action::LaunchVerifier;
        }
        self.clean_streak = 0;
        let problems = verdict.problems();
        if is_churn(&problems, &self.last_findings) {
            return self.pause(texts::REASON_CHURN);
        }
        let failed_round = self.round;
        self.last_findings = problems;
        self.round += 1;
        if self.round > self.max_rounds {
            return self.pause(texts::REASON_ROUNDS);
        }
        self.state = TaskState::Running;
        let path = folder.join(format!("correccion-{failed_round}.md"));
        Action::SendCorrection(path.to_string_lossy().to_string())
    }

    /// Incidente de la terminal del ejecutor o del verificador (9.5).
    pub fn on_incident(&mut self, incident: &Incident) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        if *incident == Incident::ExecutorError {
            self.executor_errors += 1;
            if self.executor_errors < 2 {
                return Action::Wait;
            }
        }
        self.pause(&incident.reason())
    }

    /// Comprobación periódica de los topes (9.5).
    pub fn on_deadline(&mut self, now: f64) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        if now > self.deadline {
            return self.pause(texts::REASON_DEADLINE);
        }
        if self.round > self.max_rounds {
            return self.pause(texts::REASON_ROUNDS);
        }
        Action::Wait
    }

    /// Al arrancar la app: `running` o `verifying` pasan a `paused` (9.3).
    pub fn on_restart(&mut self) -> Action {
        if matches!(
            self.state,
            TaskState::Preparing | TaskState::Running | TaskState::Verifying
        ) {
            return self.pause(texts::REASON_RESTART);
        }
        Action::Wait
    }

    /// Pausa y pregunta al usuario; nunca se reanuda sola.
    pub fn pause(&mut self, reason: &str) -> Action {
        if !self.unfinished() || self.state == TaskState::Paused {
            return Action::Wait;
        }
        self.state = TaskState::Paused;
        self.pause_reason = Some(reason.to_string());
        Action::Pause(reason.to_string())
    }

    /// «Sigue»: vuelve a ejecutar y, si el tope era la causa, da otra ronda.
    pub fn resume(&mut self, now: f64) -> Action {
        if self.state != TaskState::Paused {
            return Action::Wait;
        }
        self.state = TaskState::Running;
        self.pause_reason = None;
        self.verify_failures = 0;
        self.executor_errors = 0;
        // DECISION: el tope es un punto de control; «sigue» concede una ronda
        // más y, si el plazo ya venció, la misma ventana otra vez.
        if self.round > self.max_rounds {
            self.max_rounds = self.round;
        }
        if now > self.deadline {
            let window = (self.deadline - self.started_at).max(0.0);
            self.deadline = now + window;
        }
        Action::Resume
    }

    /// «Para»: se retira la autorización al instante (9.5).
    pub fn stop(&mut self, _reason: &str) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        self.state = TaskState::Stopped;
        self.pause_reason = None;
        Action::Stopped
    }
}

fn minutes(minutes: i64) -> f64 {
    (minutes.max(0) as f64) * 60.0
}
