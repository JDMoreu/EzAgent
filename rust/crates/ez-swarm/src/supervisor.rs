//! Port of `src/ezagent/swarm/supervisor.py` (F4-13a).
//!
//! Los agentes delegados no pueden dejar una conexión abierta esperando
//! mensajes, así que el supervisor hace de canal: vigila el tablero y, cuando
//! un miembro tiene algo pendiente y su agente está libre, le escribe una línea
//! en su terminal. Para el agente es una suscripción: se entera solo de lo suyo
//! y en el momento en que puede atenderlo, sin consultar ni gastar tokens
//! mientras espera.
//!
//! Corre en su propio hilo (`ezagent-swarm-sup`); nunca en el de UI ni en los
//! de audio.

mod host;
mod launch;
mod members;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ez_core::metrics::monotonic;
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{py_strip, python_str, text_or_empty, truthy};
use serde_json::Value;
use tracing::{debug, error};

use crate::db::{Clock, DbResult};
use crate::progress::Progress;
use crate::protocol;
use crate::recovery::{Recovery, RecoveryOptions, ResetLaunches};
use crate::service::SwarmService;

pub use host::{HerdrHost, LaunchRequest, SwarmHost};

/// Pasada con trabajo pendiente.
pub const TICK_SECS: f64 = 3.0;
/// Pasada sin nada que hacer.
pub const IDLE_TICK_SECS: f64 = 20.0;
/// Espera antes de repetir el mismo aviso.
pub const RENUDGE_SECS: f64 = 120.0;
/// Avisos del mismo tipo antes de callarse.
pub const MAX_NUDGES: u32 = 3;
/// Tiempo con una tarea abierta antes de recordar cerrarla.
pub const OPEN_TASK_SECS: f64 = 45.0;
/// Reintento del pegado inicial hasta que el agente use `ezs`.
pub const BRIEF_RETRY_SECS: f64 = 20.0;
/// Margen tras abrir la terminal antes de escribirle.
pub const LAUNCH_SETTLE_SECS: f64 = 4.0;
/// Pasadas seguidas sin ver una terminal antes de darla por perdida.
pub const MISSING_TICKS: u32 = 3;
/// Reaperturas automáticas antes de preguntar.
pub const MAX_LAUNCHES: u32 = 2;

const WORKING: &str = "working";
const IDLE: &str = "idle";
const BLOCKED: &str = "blocked";
const ERROR: &str = "error";
const MISSING: &str = "missing";
const UNKNOWN: &str = "unknown";
const EXITED: &str = "exited";
const SHELLS: [&str; 9] = [
    "",
    "powershell",
    "pwsh",
    "c\u{6d}d",
    "bash",
    "zsh",
    "sh",
    "fish",
    "nu",
];

/// A quién se avisa (la app conecta esto con la voz).
pub type Announce = Arc<dyn Fn(&str) + Send + Sync>;

type Key = (String, String);

/// Opciones del supervisor (los argumentos con nombre del Python).
pub struct SupervisorConfig {
    /// Carpeta donde se guarda el token de cada asiento.
    pub seat_dir: PathBuf,
    /// Aviso hablado.
    pub announce: Announce,
    /// Pregunta al usuario; sin ella, se usa `announce`.
    pub ask: Option<Announce>,
    /// Carpeta del ejecutable `ezs` que se antepone al `PATH`.
    pub command_dir: String,
    /// Reloj monotónico para esperas.
    pub clock: Clock,
    /// Reloj de pared para fechas.
    pub wall: Clock,
}

impl SupervisorConfig {
    /// Opciones con los valores por defecto del Python.
    pub fn new(seat_dir: impl Into<PathBuf>, announce: Announce) -> Self {
        Self {
            seat_dir: seat_dir.into(),
            announce,
            ask: None,
            command_dir: String::new(),
            clock: Arc::new(monotonic),
            wall: Arc::new(timestamp),
        }
    }
}

/// Firma de lo que ya se avisó, para no repetirlo.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Signature {
    Brief,
    Pending { last_id: i64, ready: Vec<i64> },
    Open(Vec<i64>),
}

#[derive(Debug, Clone)]
struct Nudge {
    signature: Signature,
    count: u32,
    at: f64,
}

/// `threading.Event`: bandera que se limpia a mano y espera con plazo.
#[derive(Default)]
struct Event {
    flag: Mutex<bool>,
    condvar: Condvar,
}

impl Event {
    fn set(&self) {
        *self
            .flag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.condvar.notify_all();
    }

    fn clear(&self) {
        *self
            .flag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }

    fn is_set(&self) -> bool {
        *self
            .flag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wait(&self, secs: f64) {
        let flag = self
            .flag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *flag {
            return;
        }
        let _ = self
            .condvar
            .wait_timeout(flag, Duration::from_secs_f64(secs.max(0.0)));
    }
}

#[derive(Default)]
struct State {
    launching: HashSet<Key>,
    nudged: HashMap<Key, Nudge>,
    idle_since: HashMap<Key, f64>,
    unbriefed: HashSet<Key>,
    missing: HashMap<Key, u32>,
    launches: HashMap<Key, u32>,
    announced: HashMap<String, HashSet<String>>,
    finished: Vec<String>,
}

/// Lo que comparten el hilo del supervisor, la recuperación y el servidor.
#[derive(Default)]
struct Book {
    state: Mutex<State>,
    wake: Event,
}

impl Book {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Llega desde el hilo del servidor: solo se anota y se despierta el ciclo.
    fn on_event(&self, swarm: &str, member: &str, op: &str) {
        {
            let mut state = self.lock();
            state
                .unbriefed
                .remove(&(swarm.to_string(), member.to_string()));
            if op == "finished" {
                state.finished.push(swarm.to_string());
            }
        }
        self.wake.set();
    }

    /// Miembro reabierto o reemplazado: empieza de cero sus contadores.
    fn reset(&self, swarm: &str, member: &str) {
        let key = (swarm.to_string(), member.to_string());
        {
            let mut state = self.lock();
            state.launches.remove(&key);
            state.unbriefed.remove(&key);
            state.nudged.remove(&key);
            state.missing.remove(&key);
            state.idle_since.remove(&key);
        }
        self.wake.set();
    }
}

/// Lanza, despierta y vigila a los miembros de los enjambres activos.
pub struct Supervisor {
    service: Arc<SwarmService>,
    host: Arc<dyn SwarmHost>,
    announce: Announce,
    ask: Announce,
    command_dir: String,
    running: AtomicUsize,
    view: RwLock<Vec<(String, String)>>,
    /// Avisos de avance ya emitidos.
    pub progress: Progress,
    /// Incidentes y decisiones.
    pub recovery: Recovery,
    clock: Clock,
    book: Arc<Book>,
    launch: Arc<launch::LaunchContext>,
    stop: Event,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Supervisor {
    /// Construye el supervisor y se suscribe a los eventos del tablero.
    pub fn new(
        service: Arc<SwarmService>,
        host: Arc<dyn SwarmHost>,
        config: SupervisorConfig,
    ) -> Self {
        let book = Arc::new(Book::default());
        {
            let book = Arc::clone(&book);
            service.subscribe(move |swarm, member, op| book.on_event(swarm, member, op));
        }
        let reset_book = Arc::clone(&book);
        let reset: ResetLaunches = Arc::new(move |swarm, member| reset_book.reset(swarm, member));
        let ask = config
            .ask
            .clone()
            .unwrap_or_else(|| Arc::clone(&config.announce));
        let handoff_dir = config
            .seat_dir
            .parent()
            .map_or_else(|| PathBuf::from("handoff"), |parent| parent.join("handoff"));
        let recovery = Recovery::with_options(
            Arc::clone(&service),
            Arc::clone(&host),
            handoff_dir,
            Arc::clone(&config.announce),
            Arc::clone(&ask),
            RecoveryOptions {
                reset_launches: reset,
                wall: Arc::clone(&config.wall),
                clock: Arc::clone(&config.clock),
            },
        );
        let launch = Arc::new(launch::LaunchContext {
            service: Arc::clone(&service),
            host: Arc::clone(&host),
            seat_dir: config.seat_dir,
            announce: Arc::clone(&config.announce),
            command_dir: config.command_dir.clone(),
            clock: Arc::clone(&config.clock),
            book: Arc::clone(&book),
        });
        Self {
            service,
            host,
            announce: config.announce,
            ask,
            command_dir: config.command_dir,
            running: AtomicUsize::new(0),
            view: RwLock::new(Vec::new()),
            progress: Progress::new(),
            recovery,
            clock: config.clock,
            book,
            launch,
            stop: Event::default(),
            thread: Mutex::new(None),
        }
    }

    // -------------------------------------------------------------- ciclo

    /// Arranca el hilo del supervisor.
    pub fn start(self: &Arc<Self>) {
        let mut thread = self
            .thread
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if thread.is_some() {
            return;
        }
        self.stop.clear();
        let this = Arc::clone(self);
        match thread::Builder::new()
            .name("ezagent-swarm-sup".to_string())
            .spawn(move || this.run())
        {
            Ok(handle) => *thread = Some(handle),
            Err(error) => error!("No pude arrancar el supervisor del enjambre: {error}"),
        }
    }

    /// Detiene el hilo y espera a que termine.
    pub fn stop(&self) {
        self.stop.set();
        self.book.wake.set();
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(thread) = thread {
            // DECISION: sin plazo (Python usa `join(timeout=5)`): `stop`
            // despierta el ciclo y solo se espera a la pasada en curso, acotada
            // por los timeouts propios de las llamadas a Herdr.
            let _ = thread.join();
        }
    }

    /// Adelanta la próxima pasada (hay novedades en el tablero).
    pub fn poke(&self) {
        self.book.wake.set();
    }

    fn run(&self) {
        while !self.stop.is_set() {
            let busy = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick()))
            {
                Ok(Ok(busy)) => busy,
                Ok(Err(error)) => {
                    error!("Fallo en una pasada del supervisor del enjambre: {error}");
                    false
                }
                Err(_) => {
                    error!("Fallo en una pasada del supervisor del enjambre");
                    false
                }
            };
            self.book
                .wake
                .wait(if busy { TICK_SECS } else { IDLE_TICK_SECS });
            self.book.wake.clear();
        }
    }

    /// Una pasada completa. Devuelve `true` si hay enjambres activos.
    pub fn tick(&self) -> DbResult<bool> {
        let finished = std::mem::take(&mut self.book.lock().finished);
        for swarm in finished {
            self.book.lock().announced.remove(&swarm);
            self.progress.forget(&swarm);
            let summary = self.service.summary(&swarm)?;
            (self.ask)(&format!("{summary} {}", protocol::FINAL_OFFER));
        }
        let active = self.service.store.active()?;
        self.running.store(active.len(), Ordering::SeqCst);
        *self
            .view
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = active
            .iter()
            .map(|info| (info.id.clone(), info.goal.clone()))
            .collect();
        if active.is_empty() {
            return Ok(false);
        }
        let states = self.herdr_states();
        for info in &active {
            self.tick_swarm(info, states.as_ref())?;
        }
        Ok(true)
    }

    /// Estado de cada terminal según Herdr, o `None` si no se pudo consultar.
    fn herdr_states(&self) -> Option<HashMap<String, String>> {
        match self.host.list_agents() {
            Ok(agents) => Some(
                agents
                    .iter()
                    .map(|agent| {
                        (
                            text_or_empty(agent.get("address")),
                            agent_state(agent.get("state")).to_string(),
                        )
                    })
                    .collect(),
            ),
            Err(error) => {
                debug!("El supervisor no pudo consultar Herdr: {error}");
                None
            }
        }
    }

    /// Enjambres activos en la última pasada (lo lee la UI).
    pub fn running(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    /// `(id, objetivo)` de los enjambres activos (lo lee la UI).
    pub fn active_view(&self) -> Vec<(String, String)> {
        self.view
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Carpeta del ejecutable `ezs`, si se encontró.
    pub fn command_dir(&self) -> &str {
        &self.command_dir
    }
}

/// `str(raw or "").strip().lower()` convertido a uno de los estados internos.
fn agent_state(raw: Option<&Value>) -> &'static str {
    let text = match raw {
        Some(value) if truthy(value) => python_str(value),
        _ => String::new(),
    };
    match py_strip(&text).to_lowercase().as_str() {
        WORKING => WORKING,
        "done" | IDLE => IDLE,
        BLOCKED => BLOCKED,
        ERROR => ERROR,
        _ => UNKNOWN,
    }
}
