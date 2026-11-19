//! API publica del arbitro: comandos, acciones y senales encoladas.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;

use crate::events::{ArbiterEvent, ElephantEvent};
use crate::types::{
    Caller, Clock, Command, FishKey, FishLauncher, InstructionWriter, Outcome, Rejection, Space,
};

use super::{ArbiterShared, Job};

/// El árbitro de un proceso: una tarea activa por espacio.
///
/// Los comandos, las acciones de la persona y las señales de los peces pasan
/// por el ejecutor secuencial del espacio; `wait_for_elephant`, `subscribe` y
/// `snapshot` leen directamente (§14.5).
#[derive(Clone)]
pub struct Arbiter {
    shared: Arc<ArbiterShared>,
}

impl Arbiter {
    /// Abre el árbitro, crea su carpeta de datos y recarga las tareas no finales.
    pub fn new(
        data_dir: PathBuf,
        launcher: Arc<dyn FishLauncher>,
        writer: Arc<dyn InstructionWriter>,
        clock: Arc<dyn Clock>,
    ) -> Result<Arbiter, String> {
        let shared = ArbiterShared::new(data_dir, launcher, writer, clock)?;
        Ok(Arbiter {
            shared: Arc::new(shared),
        })
    }

    /// Registra o actualiza un espacio (lo llama la ventana al abrirlo).
    pub fn register_space(&self, space: Space) {
        self.shared.register_space(space);
    }

    /// Ejecuta un comando del elefante o de un pez, en el ejecutor del espacio.
    pub fn execute(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection> {
        let space = match caller {
            Caller::Elephant { space } => space.clone(),
            Caller::Fish(key) => key.space.clone(),
        };
        let caller = caller.clone();
        self.enqueue(&space, move |arbiter| {
            let result = arbiter.dispatch(&caller, command);
            if let Err(rejection) = &result {
                arbiter.log_rejection(&caller, rejection);
            }
            result
        })
    }

    /// La persona aprueba la tarea propuesta.
    pub fn approve_task(&self, space: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        self.person(space, "approve_task", |arbiter, space| {
            arbiter.approve_task_inner(space)
        })
    }

    /// La persona rechaza la tarea con un comentario para el elefante.
    pub fn reject_task(&self, space: &str, comment: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        let comment = comment.to_string();
        self.person(space, "reject_task", move |arbiter, space| {
            arbiter.reject_task_inner(space, &comment)
        })
    }

    /// La persona aprueba el equipo propuesto.
    pub fn approve_team(&self, space: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        self.person(space, "approve_team", |arbiter, space| {
            arbiter.approve_team_inner(space)
        })
    }

    /// La persona rechaza el equipo con un comentario para el elefante.
    pub fn reject_team(&self, space: &str, comment: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        let comment = comment.to_string();
        self.person(space, "reject_team", move |arbiter, space| {
            arbiter.reject_team_inner(space, &comment)
        })
    }

    /// La persona reanuda una tarea en pausa.
    pub fn resume(&self, space: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        self.person(space, "resume", |arbiter, space| {
            arbiter.resume_inner(space)
        })
    }

    /// La persona detiene la tarea.
    pub fn stop(&self, space: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        self.person(space, "stop", |arbiter, space| arbiter.stop_inner(space))
    }

    /// La persona da más margen (+2 rondas, +1 h) y reanuda (§14.5).
    pub fn more_time(&self, space: &str) -> Result<Outcome, Rejection> {
        let space = space.to_string();
        self.person(space, "more_time", |arbiter, space| {
            arbiter.more_time_impl(space)
        })
    }

    /// Acción de una tarjeta de aprobación o pausa (diseño §14.8).
    pub fn card_action(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Outcome, Rejection> {
        let space = self
            .shared
            .inner
            .lock()
            .cards
            .get(card)
            .map(|card| card.space.clone())
            .ok_or_else(|| Rejection::new("R12", "No hay una tarjeta abierta con ese id."))?;
        let id = card.to_string();
        let action = action.to_string();
        let comment = comment.map(str::to_string);
        self.enqueue(&space, move |arbiter| {
            arbiter.card_action_inner(&id, &action, comment.as_deref())
        })
    }

    /// La ventana vio que un pez quedó bloqueado en hc.
    pub fn fish_blocked(&self, key: &FishKey, kind: Option<&str>) {
        let space = key.space.clone();
        let key = key.clone();
        let kind = kind.map(str::to_string);
        let _ = self.enqueue(&space, move |arbiter| {
            arbiter.fish_blocked(&key, kind.as_deref());
            Ok(Outcome::new(""))
        });
    }

    /// La ventana vio que la terminal de un pez terminó antes de informar.
    pub fn fish_exited(&self, key: &FishKey) {
        self.fish_gone_signal(key, "la terminal del pez terminó", false);
    }

    /// La ventana vio que un pez falló antes de informar (§14.5).
    pub fn fish_failed(&self, key: &FishKey, motive: &str) {
        self.fish_gone_signal(key, motive, true);
    }

    fn fish_gone_signal(&self, key: &FishKey, motive: &str, failed: bool) {
        let space = key.space.clone();
        let key = key.clone();
        let motive = motive.to_string();
        let _ = self.enqueue(&space, move |arbiter| {
            if failed {
                arbiter.fish_failed(&key, &motive);
            } else {
                arbiter.fish_exited(&key);
            }
            Ok(Outcome::new(""))
        });
    }

    /// Espera una novedad del elefante; `None` si vence el tope.
    pub fn wait_for_elephant(&self, space: &str, timeout: Duration) -> Option<ElephantEvent> {
        self.shared.wait_for_elephant(space, timeout)
    }

    /// Suscribe la vista a la traza.
    pub fn subscribe(&self) -> Receiver<ArbiterEvent> {
        self.shared.subscribe()
    }

    /// Estado de la tarea del espacio para la vista.
    pub fn snapshot(&self, space: &str) -> Option<serde_json::Value> {
        self.shared.snapshot(space)
    }

    /// Encola un trabajo en el ejecutor del espacio y espera su resultado.
    fn enqueue<F>(&self, space: &str, run: F) -> Result<Outcome, Rejection>
    where
        F: FnOnce(&ArbiterShared) -> Result<Outcome, Rejection> + Send + 'static,
    {
        let sender = self.shared.queue_sender(space);
        let (done_tx, done_rx) = crossbeam_channel::bounded(1);
        sender
            .send(Job {
                run: Box::new(run),
                done: done_tx,
            })
            .map_err(|_| Rejection::new("R12", "El ejecutor del árbitro se detuvo."))?;
        done_rx
            .recv()
            .unwrap_or_else(|_| Err(Rejection::new("R12", "El ejecutor del árbitro se detuvo.")))
    }

    /// Acción de la persona: se encola y registra el rechazo si corresponde.
    fn person<F>(&self, space: String, _label: &str, action: F) -> Result<Outcome, Rejection>
    where
        F: FnOnce(&ArbiterShared, &str) -> Result<Outcome, Rejection> + Send + 'static,
    {
        let key = space.clone();
        self.enqueue(&key, move |arbiter| {
            let result = action(arbiter, &space);
            if let Err(rejection) = &result {
                arbiter.log_person_rejection(&space, rejection);
            }
            result
        })
    }
}
