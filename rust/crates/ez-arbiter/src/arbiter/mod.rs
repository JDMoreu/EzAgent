//! El árbitro: estado de las tareas, reglas R1–R12 y eventos.
//!
//! Regla de concurrencia: el `Mutex` interno solo se toma para leer o cambiar
//! el estado en memoria. Git, los comandos de verificación, el lanzamiento de
//! peces y el disco ocurren fuera del candado.

mod emit;
mod fish;
mod load;
mod round;
mod task_cmds;
mod team;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use ez_team::Role;

use crate::events::{ArbiterEvent, ElephantEvent};
use crate::state::Task;
use crate::texts;
use crate::types::{
    Caller, Clock, Command, FishLauncher, InstructionWriter, Outcome, Rejection, Space,
};

/// Cola de novedades de un espacio para `ez wait`.
#[derive(Clone)]
pub(crate) struct ElephantQueue {
    pub tx: Sender<ElephantEvent>,
    pub rx: Receiver<ElephantEvent>,
}

/// Estado compartido protegido por un candado.
pub(crate) struct Inner {
    pub spaces: HashMap<String, Space>,
    pub tasks: HashMap<String, Task>,
    pub subscribers: Vec<Sender<ArbiterEvent>>,
    pub elephants: HashMap<String, ElephantQueue>,
}

/// Cola nueva y vacía.
fn elephant_queue() -> ElephantQueue {
    let (tx, rx) = crossbeam_channel::unbounded();
    ElephantQueue { tx, rx }
}

/// El árbitro de un proceso: una tarea activa por espacio.
pub struct Arbiter {
    pub(crate) data_dir: PathBuf,
    pub(crate) launcher: Arc<dyn FishLauncher>,
    pub(crate) writer: Arc<dyn InstructionWriter>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) inner: Mutex<Inner>,
}

impl Arbiter {
    /// Abre el árbitro, crea su carpeta de datos y recarga las tareas no finales.
    pub fn new(
        data_dir: PathBuf,
        launcher: Arc<dyn FishLauncher>,
        writer: Arc<dyn InstructionWriter>,
        clock: Arc<dyn Clock>,
    ) -> Result<Arbiter, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| format!("No se pudo crear {}: {error}.", data_dir.display()))?;
        let loaded = load::load_tasks(&data_dir);
        let mut spaces = HashMap::new();
        let mut elephants = HashMap::new();
        let mut tasks = HashMap::new();
        for task in loaded {
            spaces
                .entry(task.space.id.clone())
                .or_insert_with(|| task.space.clone());
            elephants
                .entry(task.space.id.clone())
                .or_insert_with(elephant_queue);
            tasks.insert(task.id.clone(), task);
        }
        Ok(Arbiter {
            data_dir,
            launcher,
            writer,
            clock,
            inner: Mutex::new(Inner {
                spaces,
                tasks,
                subscribers: Vec::new(),
                elephants,
            }),
        })
    }

    /// Registra o actualiza un espacio (lo llama la ventana al abrirlo).
    pub fn register_space(&self, space: Space) {
        let mut inner = self.inner.lock();
        inner
            .elephants
            .entry(space.id.clone())
            .or_insert_with(elephant_queue);
        inner.spaces.insert(space.id.clone(), space);
    }

    /// Ejecuta un comando del elefante o de un pez.
    pub fn execute(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection> {
        let result = self.dispatch(caller, command);
        if let Err(rejection) = &result {
            self.log_rejection(caller, rejection);
        }
        result
    }

    fn dispatch(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection> {
        match command {
            Command::Status => self.status(caller),
            Command::TaskNew {
                title,
                brief,
                criteria,
            } => {
                let space = self.elephant_space(caller)?;
                self.task_new(&space, title, brief, criteria)
            }
            Command::TeamPropose { seats, card } => {
                let space = self.elephant_space(caller)?;
                self.team_propose(&space, seats, card)
            }
            Command::RoundStart => {
                let space = self.elephant_space(caller)?;
                self.round_start(&space)
            }
            Command::FishNew { role, brief } => {
                let space = self.elephant_space(caller)?;
                self.fish_new(&space, role, brief)
            }
            Command::FishSend { fish, file } => {
                let space = self.elephant_space(caller)?;
                self.fish_send(&space, &fish, file)
            }
            Command::Verify => {
                let space = self.elephant_space(caller)?;
                self.verify(&space)
            }
            Command::RoundClose => {
                let space = self.elephant_space(caller)?;
                self.round_close(&space)
            }
            Command::TaskClose => {
                let space = self.elephant_space(caller)?;
                self.task_close(&space)
            }
            Command::Pause { reason } => {
                let space = self.elephant_space(caller)?;
                self.pause_command(&space, reason)
            }
            Command::Report {
                verdict,
                file,
                blocking,
                major,
                minor,
                mutations_tried,
                mutations_caught,
            } => {
                let fish = self.fish_caller(caller)?;
                self.report(
                    &fish,
                    fish::ReportInput {
                        verdict,
                        file,
                        blocking,
                        major,
                        minor,
                        mutations_tried,
                        mutations_caught,
                    },
                )
            }
        }
    }

    // -------------------------------------------------------------- personas

    /// La persona aprueba la tarea propuesta.
    pub fn approve_task(&self, space: &str) -> Result<Outcome, Rejection> {
        let result = self.approve_task_inner(space);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    /// La persona rechaza la tarea con un comentario para el elefante.
    pub fn reject_task(&self, space: &str, comment: &str) -> Result<Outcome, Rejection> {
        let result = self.reject_task_inner(space, comment);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    /// La persona aprueba el equipo propuesto.
    pub fn approve_team(&self, space: &str) -> Result<Outcome, Rejection> {
        let result = self.approve_team_inner(space);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    /// La persona rechaza el equipo con un comentario para el elefante.
    pub fn reject_team(&self, space: &str, comment: &str) -> Result<Outcome, Rejection> {
        let result = self.reject_team_inner(space, comment);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    /// La persona reanuda una tarea en pausa.
    pub fn resume(&self, space: &str) -> Result<Outcome, Rejection> {
        let result = self.resume_inner(space);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    /// La persona detiene la tarea.
    pub fn stop(&self, space: &str) -> Result<Outcome, Rejection> {
        let result = self.stop_inner(space);
        if let Err(rejection) = &result {
            self.log_person_rejection(space, rejection);
        }
        result
    }

    // --------------------------------------------------------------- señales

    /// La ventana vio que un pez quedó bloqueado en hc.
    pub fn fish_blocked(&self, fish: &str, kind: Option<&str>) {
        let Some(task) = self.task_for_fish(fish).ok() else {
            return;
        };
        let message = texts::elephant_blocked(fish, kind);
        let data = serde_json::json!({ "fish": fish, "kind": kind });
        let event = self.event(&task, "fish_blocked", data.clone());
        let elephant = self.elephant_event(&task, "fish_blocked", message, data);
        self.emit_only(&task.space.id, Some(&task.id), vec![event], vec![elephant]);
    }

    /// La ventana vio que la terminal de un pez terminó.
    pub fn fish_exited(&self, fish: &str) {
        let Some(task) = self.task_for_fish(fish).ok() else {
            return;
        };
        let message = texts::elephant_exited(fish);
        let data = serde_json::json!({ "fish": fish });
        let event = self.event(&task, "fish_exited", data.clone());
        let elephant = self.elephant_event(&task, "fish_exited", message, data);
        self.emit_only(&task.space.id, Some(&task.id), vec![event], vec![elephant]);
    }

    /// Espera una novedad del elefante; `None` si vence el tope.
    pub fn wait_for_elephant(&self, space: &str, timeout: Duration) -> Option<ElephantEvent> {
        let receiver = {
            let inner = self.inner.lock();
            inner.elephants.get(space).map(|queue| queue.rx.clone())
        }?;
        receiver.recv_timeout(timeout).ok()
    }

    /// Suscribe la vista a la traza.
    pub fn subscribe(&self) -> Receiver<ArbiterEvent> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        self.inner.lock().subscribers.push(sender);
        receiver
    }

    /// Estado de la tarea del espacio para la vista.
    pub fn snapshot(&self, space: &str) -> Option<serde_json::Value> {
        let inner = self.inner.lock();
        inner
            .tasks
            .values()
            .find(|task| task.is_active() && task.space.id == space)
            .map(Task::snapshot)
    }

    // --------------------------------------------------------------- ayudas

    /// Reloj del árbitro.
    pub(crate) fn now(&self) -> f64 {
        self.clock.now()
    }

    /// Espacio registrado a partir de quien llama.
    pub(crate) fn elephant_space(&self, caller: &Caller) -> Result<String, Rejection> {
        let Caller::Elephant { space } = caller else {
            return Err(Rejection::new("R1", texts::ELEPHANT_ONLY));
        };
        if self.inner.lock().spaces.contains_key(space) {
            Ok(space.clone())
        } else {
            Err(Rejection::new("R1", texts::UNKNOWN_SPACE))
        }
    }

    /// Pez que llama.
    pub(crate) fn fish_caller(&self, caller: &Caller) -> Result<String, Rejection> {
        match caller {
            Caller::Fish { fish } => Ok(fish.clone()),
            Caller::Elephant { .. } => Err(Rejection::new("R1", texts::FISH_ONLY)),
        }
    }

    /// Espacio registrado, o error de R1.
    pub(crate) fn space(&self, space: &str) -> Result<Space, Rejection> {
        self.inner
            .lock()
            .spaces
            .get(space)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_SPACE))
    }

    /// Tarea activa del espacio.
    pub(crate) fn task_for_elephant(&self, space: &str) -> Result<Task, Rejection> {
        self.space(space)?;
        self.inner
            .lock()
            .tasks
            .values()
            .find(|task| task.is_active() && task.space.id == space)
            .cloned()
            .ok_or_else(|| Rejection::new("R2", texts::NO_ACTIVE_TASK))
    }

    /// Tarea activa que contiene al pez.
    pub(crate) fn task_for_fish(&self, fish: &str) -> Result<Task, Rejection> {
        self.inner
            .lock()
            .tasks
            .values()
            .find(|task| task.is_active() && task.fish(fish).is_some())
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_FISH))
    }

    /// Tarea de quien llama, si se puede resolver.
    pub(crate) fn task_for_caller(&self, caller: &Caller) -> Option<Task> {
        match caller {
            Caller::Elephant { space } => self.task_for_elephant(space).ok(),
            Caller::Fish { fish } => self.task_for_fish(fish).ok(),
        }
    }

    /// Deja constancia de un rechazo de una acción de la persona.
    fn log_person_rejection(&self, space: &str, rejection: &Rejection) {
        let task = self.task_for_elephant(space).ok();
        let event = ArbiterEvent {
            kind: "rejected".to_string(),
            space: space.to_string(),
            task: task.as_ref().map(|task| task.id.clone()),
            ts: self.now(),
            data: serde_json::json!({ "rule": rejection.rule, "message": rejection.message }),
        };
        let (subscribers, sender) = {
            let inner = self.inner.lock();
            (
                inner.subscribers.clone(),
                inner.elephants.get(space).cloned(),
            )
        };
        if task.is_some() {
            self.append_bitacora(&event);
        }
        if let Some(sender) = sender {
            let mut message = ElephantEvent::new(
                "rejected",
                space,
                event.ts,
                rejection.message.clone(),
                event.data.clone(),
            );
            message.task = task.as_ref().map(|task| task.id.clone());
            let _ = sender.tx.send(message);
        }
        for subscriber in subscribers {
            let _ = subscriber.send(event.clone());
        }
    }
}

/// Nombre hc de un pez: `ez-{4 últimos de la tarea}-{pez}`.
pub(crate) fn hc_name(task: &str, fish: &str) -> String {
    let tail: String = task
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("ez-{tail}-{fish}")
}

/// Token nuevo de un pez (uuid v4 simple).
pub(crate) fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Id de pez de un papel en una ronda (`dev-r1-2`, `qa-r1`…).
pub(crate) fn fish_id(role: Role, round: u32, index: u32) -> String {
    match role {
        Role::Developer => format!("dev-r{round}-{index}"),
        other => format!("{}-r{round}", other.short()),
    }
}
