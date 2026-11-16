//! Estado del modo autónomo y sus transiciones (10.1, 10.10 y 10.13).
//!
//! Son funciones puras: no tocan Herdr, ni ventanas, ni el disco. La app (A3)
//! ejecuta la acción devuelta fuera de cualquier cerrojo.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::texts;
use super::verdict::{is_churn, Verdict, VerdictError};

/// Un ejecutor en reposo sin `fin-N.md` pausa la tarea (10.10).
pub const IDLE_LIMIT_S: f64 = 180.0;

/// Como máximo tres verificaciones por ronda (10.13).
pub const MAX_VERIFICATIONS_PER_ROUND: i64 = 3;

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

    /// Ni cumplida ni detenida: todavía hay algo que hacer (10.1).
    pub fn unfinished(self) -> bool {
        !matches!(self, Self::Done | Self::Stopped)
    }
}

/// El papel de una terminal dentro de `scope` (10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeRole {
    Executor,
    Verifier,
    Member,
}

impl ScopeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Executor => "executor",
            Self::Verifier => "verifier",
            Self::Member => "member",
        }
    }
}

/// Una terminal cubierta por la autorización y las carpetas que puede editar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeEntry {
    pub address: String,
    pub role: ScopeRole,
    pub edit_dirs: Vec<String>,
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

/// Topes de la tarea (10.13); los argumentos mandan sobre estos valores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_rounds: i64,
    pub max_minutes: i64,
    pub max_approvals: i64,
}

/// Estado completo, tal como se escribe en `privado/estado.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutonomousState {
    pub id: String,
    pub mode: Mode,
    pub state: TaskState,
    pub round: i64,
    pub started_at: f64,
    pub deadline: f64,
    pub max_rounds: i64,
    pub max_approvals: i64,
    pub approvals: i64,
    #[serde(default)]
    pub workdir: String,
    /// Commit base del proyecto, registrado al empezar (10.16).
    #[serde(default)]
    pub base_commit: Option<String>,
    pub scope: Vec<ScopeEntry>,
    pub executor: Option<Executor>,
    pub team: Option<Team>,
    pub clean_streak: i64,
    /// Todos los hallazgos ya mandados a corregir (10.14).
    #[serde(default)]
    pub corrected_findings: Vec<String>,
    pub pause_reason: Option<String>,
    #[serde(default)]
    pub accepted: bool,
    /// Instante de la pausa; el tiempo en pausa no cuenta para el plazo (10.13).
    #[serde(default)]
    pub paused_at: Option<f64>,
    /// K global de las verificaciones, empieza en 1 (10.10).
    #[serde(default)]
    pub verification_seq: i64,
    /// Intentos fallidos con el veredicto de la ronda en curso (no va a disco).
    #[serde(skip)]
    pub verify_failures: i64,
    /// Verificaciones lanzadas en la ronda en curso (no va a disco).
    #[serde(skip)]
    pub verifications_this_round: i64,
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
    /// Mandar al ejecutor la ruta de `compartido/correccion-N.md`.
    SendCorrection(String),
    /// Avisar por voz y esperar al usuario.
    Pause(String),
    /// El usuario dijo «sigue».
    Resume,
    /// Dos rondas limpias seguidas.
    Done,
    /// El usuario dijo «para».
    Stopped,
    /// El usuario aceptó el resultado de una tarea cumplida.
    Accepted,
    /// Nada que hacer con este evento.
    Wait,
}

/// Incidente que puede pausar el bucle (9.5 y 10.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incident {
    Quota,
    ContextFull,
    WindowClosed,
    ExecutorError,
    /// La terminal del agente desapareció.
    Lost,
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
            Self::Lost => texts::REASON_LOST.to_string(),
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
    pub workdir: String,
    pub accepted: bool,
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
            workdir: state.workdir.clone(),
            accepted: state.accepted,
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
    pub fn new(id: &str, mode: Mode, workdir: &str, now: f64, limits: Limits) -> Self {
        Self {
            id: id.to_string(),
            mode,
            state: TaskState::Preparing,
            round: 1,
            started_at: now,
            deadline: now + minutes(limits.max_minutes),
            max_rounds: limits.max_rounds,
            max_approvals: limits.max_approvals,
            approvals: 0,
            workdir: workdir.to_string(),
            base_commit: None,
            scope: Vec::new(),
            executor: None,
            team: None,
            clean_streak: 0,
            corrected_findings: Vec::new(),
            pause_reason: None,
            accepted: false,
            paused_at: None,
            verification_seq: 0,
            verify_failures: 0,
            verifications_this_round: 0,
            executor_errors: 0,
        }
    }

    pub fn unfinished(&self) -> bool {
        self.state.unfinished()
    }

    /// El ejecutor ya está abierto: preparando → ejecutando y a `scope`.
    ///
    /// Solo puede editar dentro de `workdir` (10.1).
    pub fn executor_started(&mut self, address: &str, agent: &str) -> Action {
        if !matches!(self.state, TaskState::Preparing | TaskState::Running) {
            return Action::Wait;
        }
        let workdir = self.workdir.clone();
        self.cover(address, ScopeRole::Executor, std::slice::from_ref(&workdir));
        self.executor = Some(Executor {
            address: address.to_string(),
            agent: agent.to_string(),
        });
        self.state = TaskState::Running;
        Action::Wait
    }

    /// El verificador abrió: solo puede editar dentro de su carpeta (10.1).
    pub fn verifier_started(&mut self, address: &str, edit_dirs: &[String]) {
        self.cover(address, ScopeRole::Verifier, edit_dirs);
    }

    /// Un miembro del equipo abrió: puede editar dentro de `workdir` (10.1).
    pub fn member_started(&mut self, address: &str) {
        let workdir = self.workdir.clone();
        self.cover(address, ScopeRole::Member, std::slice::from_ref(&workdir));
    }

    /// Suma una dirección a las cubiertas por la autorización.
    pub fn cover(&mut self, address: &str, role: ScopeRole, edit_dirs: &[String]) {
        if address.is_empty() {
            return;
        }
        let entry = ScopeEntry {
            address: address.to_string(),
            role,
            edit_dirs: edit_dirs.to_vec(),
        };
        match self.scope.iter_mut().find(|known| known.address == address) {
            Some(known) => *known = entry,
            None => self.scope.push(entry),
        }
    }

    /// La terminal se cerró o terminó: se saca de `scope` (9.5).
    pub fn forget(&mut self, address: &str) {
        self.scope.retain(|known| known.address != address);
    }

    /// Lo que se puede aprobar en una dirección, o `None` si nada (10.1).
    ///
    /// En pausa no se aprueba nada; la autorización sigue viva para reanudar.
    pub fn approval_scope(&self, address: &str) -> Option<&ScopeEntry> {
        if !matches!(self.state, TaskState::Running | TaskState::Verifying) {
            return None;
        }
        self.scope.iter().find(|entry| entry.address == address)
    }

    /// El ejecutor terminó (`done`): toca verificar.
    pub fn on_executor_finished(&mut self, now: f64) -> Action {
        if !matches!(self.state, TaskState::Preparing | TaskState::Running) {
            return Action::Wait;
        }
        self.executor_errors = 0;
        if self.expired(now) {
            return self.pause(texts::REASON_DEADLINE, now);
        }
        self.state = TaskState::Verifying;
        self.verify_failures = 0;
        self.verifications_this_round = 0;
        self.launch_verifier(now)
    }

    /// El ejecutor está en reposo: solo termina si escribió `fin-N.md` (10.10).
    pub fn on_executor_idle(
        &mut self,
        now: f64,
        fin_after_send: bool,
        idle_seconds: f64,
        agent: &str,
    ) -> Action {
        if !matches!(self.state, TaskState::Preparing | TaskState::Running) {
            return Action::Wait;
        }
        if fin_after_send {
            return self.on_executor_finished(now);
        }
        if idle_seconds >= IDLE_LIMIT_S {
            return self.pause(&texts::idle(agent), now);
        }
        Action::Wait
    }

    /// Llegó (o falló) el veredicto del verificador de la ronda.
    ///
    /// `shared` es la carpeta `compartido/`, donde vive `correccion-N.md`.
    pub fn on_verdict(
        &mut self,
        verdict: Result<Verdict, VerdictError>,
        shared: &Path,
        now: f64,
    ) -> Action {
        if self.state != TaskState::Verifying {
            return Action::Wait;
        }
        if self.expired(now) {
            return self.pause(texts::REASON_DEADLINE, now);
        }
        let verdict = match verdict {
            Ok(verdict) => {
                self.verify_failures = 0;
                verdict
            }
            Err(_) => {
                self.verify_failures += 1;
                if self.verify_failures >= 2 {
                    return self.pause(texts::REASON_VERDICT, now);
                }
                return self.launch_verifier(now);
            }
        };
        if verdict.clean() {
            self.clean_streak += 1;
            if self.clean_streak >= 2 {
                self.state = TaskState::Done;
                self.scope.clear();
                self.paused_at = None;
                return Action::Done;
            }
            return self.launch_verifier(now);
        }
        self.clean_streak = 0;
        let problems = verdict.problems();
        if problems.is_empty() {
            // Un veredicto que no cumple y no dice qué corregir es dudoso (10.17).
            return self.pause(texts::REASON_DOUBTFUL, now);
        }
        if is_churn(&problems, &self.corrected_findings) {
            return self.pause(texts::REASON_CHURN, now);
        }
        let failed_round = self.round;
        self.corrected_findings.extend(problems);
        self.round += 1;
        if self.round > self.max_rounds {
            return self.pause(texts::REASON_ROUNDS, now);
        }
        self.state = TaskState::Running;
        self.verifications_this_round = 0;
        let path = shared.join(format!("correccion-{failed_round}.md"));
        Action::SendCorrection(path.to_string_lossy().to_string())
    }

    /// Incidente de la terminal del ejecutor o del verificador (9.5 y 10.10).
    pub fn on_incident(&mut self, incident: &Incident, now: f64) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        if *incident == Incident::ExecutorError {
            self.executor_errors += 1;
            if self.executor_errors < 2 {
                return Action::Wait;
            }
        }
        self.pause(&incident.reason(), now)
    }

    /// Comprobación periódica de los topes (9.5).
    pub fn on_deadline(&mut self, now: f64) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        if self.expired(now) {
            return self.pause(texts::REASON_DEADLINE, now);
        }
        if self.round > self.max_rounds {
            return self.pause(texts::REASON_ROUNDS, now);
        }
        Action::Wait
    }

    /// Al arrancar la app: `running` o `verifying` pasan a `paused` (9.3).
    pub fn on_restart(&mut self, now: f64) -> Action {
        if matches!(
            self.state,
            TaskState::Preparing | TaskState::Running | TaskState::Verifying
        ) {
            return self.pause(texts::REASON_RESTART, now);
        }
        Action::Wait
    }

    /// Pausa y pregunta al usuario; nunca se reanuda sola.
    pub fn pause(&mut self, reason: &str, now: f64) -> Action {
        if !self.unfinished() || self.state == TaskState::Paused {
            return Action::Wait;
        }
        self.state = TaskState::Paused;
        self.pause_reason = Some(reason.to_string());
        self.paused_at = Some(now);
        Action::Pause(reason.to_string())
    }

    /// «Sigue»: vuelve a ejecutar con la prórroga que pida el usuario (10.13).
    pub fn resume(&mut self, now: f64, extend_rounds: i64, extend_minutes: i64) -> Action {
        if self.state != TaskState::Paused {
            return Action::Wait;
        }
        if let Some(paused_at) = self.paused_at.take() {
            self.deadline += (now - paused_at).max(0.0);
        }
        if extend_rounds > 0 {
            self.max_rounds += extend_rounds;
        }
        if extend_minutes > 0 {
            self.deadline = self.deadline.max(now) + minutes(extend_minutes);
        }
        self.state = TaskState::Running;
        self.pause_reason = None;
        self.verify_failures = 0;
        self.executor_errors = 0;
        Action::Resume
    }

    /// «Para»: se retira la autorización al instante (9.5 y 10.13).
    ///
    /// Con `accept`, una tarea cumplida queda aceptada; parar una tarea en
    /// curso nunca la acepta.
    pub fn stop(&mut self, _reason: &str, accept: bool) -> Action {
        if self.state == TaskState::Done {
            if accept && !self.accepted {
                self.accepted = true;
                return Action::Accepted;
            }
            return Action::Wait;
        }
        if !self.unfinished() {
            return Action::Wait;
        }
        self.state = TaskState::Stopped;
        self.pause_reason = None;
        self.paused_at = None;
        self.accepted = false;
        self.scope.clear();
        Action::Stopped
    }

    /// Una aprobación automática; al llegar al tope se pausa (10.13).
    pub fn record_approval(&mut self, now: f64) -> Action {
        if !self.unfinished() {
            return Action::Wait;
        }
        self.approvals += 1;
        if self.approvals >= self.max_approvals {
            return self.pause(texts::REASON_APPROVALS, now);
        }
        Action::Wait
    }

    /// K global de la próxima verificación (10.10).
    pub fn next_verification(&mut self) -> i64 {
        self.verification_seq += 1;
        self.verification_seq
    }

    fn launch_verifier(&mut self, now: f64) -> Action {
        self.verifications_this_round += 1;
        if self.verifications_this_round > MAX_VERIFICATIONS_PER_ROUND {
            return self.pause(texts::REASON_VERIFICATIONS, now);
        }
        Action::LaunchVerifier
    }

    fn expired(&self, now: f64) -> bool {
        now > self.deadline
    }
}

fn minutes(minutes: i64) -> f64 {
    (minutes.max(0) as f64) * 60.0
}
