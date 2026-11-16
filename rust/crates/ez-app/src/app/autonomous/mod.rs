//! Modo autónomo en la app (AUTO-A3): el bucle que vigila, aprueba como
//! transacción, verifica, corrige, aplica topes y sobrevive a un reinicio.
//!
//! Implementa 10.6, 10.7, 10.9, 10.10, 10.13 y 10.16 del diseño aprobado.
//! El estado y las transiciones son puros y viven en `ez_agent::autonomous`;
//! aquí solo se ejecutan sus `Action`.
//!
//! Regla de hilos de 9.1: **ningún cerrojo se retiene mientras se llama a
//! Herdr, al disco, al runtime o a la barra**. El hilo `ezagent-autonomo` es
//! dueño del estado de su tarea; los verbos de las herramientas solo publican
//! una petición y despiertan al hilo. Lo único compartido es un resumen corto
//! (para `autonomous_status` y la barra), las direcciones cubiertas y el
//! contador atómico de generación.

mod approve;
mod driver;
mod host;
mod time;
mod verify;
mod wiring;

#[cfg(test)]
mod tests;

pub use time::{Clock, RealClock, TestClock};
pub(crate) use wiring::{peek_row, AUTONOMOUS_KEY};

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use ez_agent::audit::AuditLog;
use ez_agent::autonomous::artifacts::{Event, TaskDir};
use ez_agent::autonomous::{AutonomousState, Criterion, StartSpec, Summary, TaskState, Verdict};
use ez_agent::tools_herdr::port::HerdrPort;
use parking_lot::{Condvar, Mutex};
use serde_json::Value;

use crate::app::core::{AppEvent, Queue};

/// Sondeo de las direcciones de `scope` (10.6).
pub const POLL_S: f64 = 1.0;
/// Margen para leer la pantalla después de aprobar (10.7).
pub const GRANT_READ_S: f64 = 2.0;
/// Plazo para que el verificador termine y escriba su veredicto (10.10).
pub const VERIFY_TIMEOUT_S: f64 = 1200.0;
/// Huellas de diálogos ya aprobados que se recuerdan (10.7).
pub const FINGERPRINTS: usize = 20;

/// Petición de un verbo del usuario al hilo del bucle.
#[derive(Debug, Clone)]
enum Request {
    /// «Para»; con `accept`, da por aceptado un resultado cumplido.
    Stop { reason: String, accept: bool },
    /// «Sigue», con la prórroga que pida el usuario (0 = sin prórroga).
    Resume { rounds: i64, minutes: i64 },
}

/// Lo compartido: un resumen corto, quién ocupa el hueco y las peticiones.
#[derive(Default)]
struct Shared {
    summary: Option<Summary>,
    coverage: Vec<String>,
    /// Direcciones de verificadores abiertos, para cerrarlos al apagar.
    verifiers: Vec<String>,
    /// Hay una tarea en `preparing`, `running`, `verifying` o `paused` (10.1).
    busy: bool,
    /// Se está construyendo una tarea nueva antes de publicarla.
    starting: bool,
    /// Cambia con cada tarea: un hilo viejo no publica sobre la nueva.
    epoch: u64,
    requests: Vec<Request>,
}

/// Una verificación en curso.
pub struct PendingVerification {
    pub k: i64,
    pub address: String,
    pub token: String,
    pub started_at: f64,
}

/// Todo lo que el hilo dueña de la tarea necesita; no se comparte.
pub struct Session {
    pub state: ez_agent::autonomous::AutonomousState,
    pub dir: TaskDir,
    pub task: String,
    pub criteria: Vec<Criterion>,
    pub space: String,
    pub executor_agent: String,
    pub verifier_agent: String,
    /// Instante del último envío al ejecutor (ancla de `fin-N.md`).
    pub sent_at: f64,
    /// Primer instante en reposo del ejecutor, para el tope de 3 min.
    pub idle_since: Option<f64>,
    pub verification: Option<PendingVerification>,
    pub last_verdict: Option<Verdict>,
    pub last_clean: Option<Verdict>,
    /// Huellas de los últimos diálogos aprobados (antirrebote, 10.7).
    pub fingerprints: Vec<String>,
    /// Ronda cuyo aviso «Ronda {n}: verificando.» ya se dijo.
    pub announced_round: i64,
}

impl Session {
    /// La sesión de una tarea recién creada (10.1 y 10.2).
    fn new(spec: &StartSpec, state: AutonomousState, dir: TaskDir, now: f64) -> Self {
        Self {
            state,
            dir,
            task: spec.task.clone(),
            criteria: spec.done_criteria.clone(),
            space: spec.space.clone(),
            executor_agent: spec.agent.clone(),
            verifier_agent: if spec.verifier_agent.is_empty() {
                spec.agent.clone()
            } else {
                spec.verifier_agent.clone()
            },
            sent_at: now,
            idle_since: None,
            verification: None,
            last_verdict: None,
            last_clean: None,
            fingerprints: Vec::new(),
            announced_round: 0,
        }
    }

    pub fn id(&self) -> &str {
        &self.state.id
    }

    /// `compartido/encargo.md`.
    pub fn encargo_path(&self) -> PathBuf {
        self.dir.shared().join("encargo.md")
    }

    /// `compartido/criterios.md`.
    pub fn criteria_path(&self) -> PathBuf {
        self.dir.shared().join("criterios.md")
    }
}

/// El bucle del modo autónomo: uno por proceso, una tarea a la vez.
pub struct AutonomousLoop {
    port: Arc<dyn HerdrPort>,
    events: Arc<Queue<AppEvent>>,
    clock: Arc<dyn Clock>,
    root: PathBuf,
    audit: Mutex<Option<Arc<AuditLog>>>,
    shared: Mutex<Shared>,
    changed: Condvar,
    /// Autorización de la tarea; `stop` y cada pausa lo incrementan primero (10.7).
    generation: AtomicU64,
    /// El `Arc` que comparten el hilo y las herramientas.
    me: OnceLock<Weak<AutonomousLoop>>,
}

impl AutonomousLoop {
    pub fn new(
        port: Arc<dyn HerdrPort>,
        events: Arc<Queue<AppEvent>>,
        clock: Arc<dyn Clock>,
        root: PathBuf,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            port,
            events,
            clock,
            root,
            audit: Mutex::new(None),
            shared: Mutex::new(Shared::default()),
            changed: Condvar::new(),
            generation: AtomicU64::new(0),
            me: OnceLock::new(),
        });
        let _ = this.me.set(Arc::downgrade(&this));
        this
    }

    /// Instala la auditoría del runtime, que existe después del bucle.
    pub fn set_audit(&self, audit: Arc<AuditLog>) {
        *self.audit.lock() = Some(audit);
    }

    pub fn status(&self) -> Option<Summary> {
        self.shared.lock().summary.clone()
    }

    /// ¿La dirección está cubierta por la autorización de la tarea? (10.6)
    pub fn covers(&self, address: &str) -> bool {
        self.shared
            .lock()
            .coverage
            .iter()
            .any(|known| known == address)
    }

    /// ¿Hay una tarea en preparación, ejecución, verificación o pausa? (10.1)
    pub fn busy(&self) -> bool {
        let shared = self.shared.lock();
        shared.busy || shared.starting
    }

    /// Contador de autorización que miran las transacciones.
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Retira la autorización al instante (10.7 y 10.13).
    fn revoke(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn say(&self, text: &str) {
        self.events.put(AppEvent::Say(text.to_string()));
    }

    /// Escribe una línea de la bitácora; el detalle se redacta dentro.
    fn log(&self, session: &Session, event: Event, detail: Value) {
        if let Err(error) = session.dir.log(event, &detail) {
            tracing::warn!("No pude escribir la bitácora: {error}");
        }
    }

    fn save(&self, session: &Session) {
        if let Err(error) = session.dir.write_state(&session.state) {
            tracing::warn!("No pude guardar el estado autónomo: {error}");
        }
    }

    /// Publica el resumen y las direcciones cubiertas sin retener nada.
    fn publish(&self, epoch: u64, session: &Session) {
        let mut shared = self.shared.lock();
        if shared.epoch != epoch {
            return;
        }
        shared.summary = Some(Summary::of(&session.state));
        shared.coverage = session
            .state
            .scope
            .iter()
            .map(|entry| entry.address.clone())
            .collect();
        shared.verifiers = session
            .state
            .scope
            .iter()
            .filter(|entry| entry.role == ez_agent::autonomous::ScopeRole::Verifier)
            .map(|entry| entry.address.clone())
            .collect();
        shared.busy = session.state.unfinished();
    }

    /// Toma las peticiones sin esperar; lo usa el sondeo de cada segundo.
    fn take_requests_now(&self, epoch: u64) -> Option<Vec<Request>> {
        let mut shared = self.shared.lock();
        if shared.epoch != epoch {
            return None;
        }
        Some(std::mem::take(&mut shared.requests))
    }

    /// Espera a que llegue una petición (pausa, cumplida o detenida).
    fn wait_requests(&self, epoch: u64) -> Option<Vec<Request>> {
        let mut shared = self.shared.lock();
        if shared.epoch != epoch {
            return None;
        }
        if shared.requests.is_empty() {
            self.changed.wait(&mut shared);
        }
        if shared.epoch != epoch {
            return None;
        }
        Some(std::mem::take(&mut shared.requests))
    }

    /// Las herramientas publican aquí su verbo; nunca esperan al bucle.
    fn request(&self, request: Request) {
        let mut shared = self.shared.lock();
        shared.requests.push(request);
        self.changed.notify_all();
    }

    /// Pide al hilo actual que salga; la próxima tarea sube `epoch`.
    pub fn shutdown(&self) {
        self.revoke();
        let verifiers = {
            let mut shared = self.shared.lock();
            shared.epoch += 1;
            self.changed.notify_all();
            shared.requests.clear();
            if let Some(summary) = shared.summary.take() {
                tracing::debug!("Modo autónomo detenido al apagar: {}", summary.id);
            }
            shared.coverage.clear();
            let verifiers = std::mem::take(&mut shared.verifiers);
            shared.busy = false;
            shared.starting = false;
            verifiers
        };
        for address in verifiers {
            let _ = self.port.kill_agent(&address, true);
        }
    }

    /// Fila de la vista rápida mientras la tarea existe (9.8).
    pub fn describe(&self) -> Option<String> {
        self.status().map(|summary| summary.describe())
    }

    /// Una tarea viva (para la barra y el atajo de cancelar, 10.19).
    pub fn running_or_verifying(&self) -> bool {
        self.status().is_some_and(|summary| {
            matches!(summary.state, TaskState::Running | TaskState::Verifying)
        })
    }

    /// Tarea nueva: reserva el hueco antes de tocar Herdr o el disco.
    fn reserve(&self) -> Result<(), String> {
        let mut shared = self.shared.lock();
        if shared.busy || shared.starting {
            return Err(ez_agent::autonomous::texts::ALREADY_ACTIVE.to_string());
        }
        shared.starting = true;
        Ok(())
    }

    fn unreserve(&self) {
        self.shared.lock().starting = false;
    }

    /// Publica una tarea recién construida y arranca su hilo.
    fn adopt(&self, session: Session) {
        let epoch = {
            let mut shared = self.shared.lock();
            shared.starting = false;
            shared.epoch += 1;
            shared.busy = session.state.unfinished();
            shared.summary = Some(Summary::of(&session.state));
            shared.coverage = session
                .state
                .scope
                .iter()
                .map(|entry| entry.address.clone())
                .collect();
            shared.verifiers = session
                .state
                .scope
                .iter()
                .filter(|entry| entry.role == ez_agent::autonomous::ScopeRole::Verifier)
                .map(|entry| entry.address.clone())
                .collect();
            shared.epoch
        };
        let Some(this) = self.me.get().and_then(Weak::upgrade) else {
            tracing::warn!("El bucle autónomo se perdió antes de arrancar su hilo.");
            return;
        };
        let built = std::thread::Builder::new()
            .name("ezagent-autonomo".to_string())
            .spawn(move || this.run(session, epoch));
        if let Err(error) = built {
            tracing::error!("No pude crear el hilo ezagent-autonomo: {error}");
            self.shared.lock().busy = false;
        }
    }

    /// Escribe el estado y publica el resumen tras una transición.
    fn commit(&self, epoch: u64, session: &Session) {
        self.save(session);
        self.publish(epoch, session);
    }

    fn now(&self) -> f64 {
        self.clock.now()
    }
}
