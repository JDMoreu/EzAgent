//! El árbitro: estado de las tareas, reglas R1–R12 y eventos.
//!
//! Regla de concurrencia: el `Mutex` interno solo se toma para leer o cambiar
//! el estado en memoria. Git, los comandos de verificación, el lanzamiento de
//! peces, los tokens y el disco ocurren fuera del candado.

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
use crate::state::{Task, WorkState};
use crate::texts;
use crate::types::{
    Caller, Clock, Command, FishKey, FishLauncher, InstructionWriter, Outcome, Rejection, Space,
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
    /// Tokens vivos por clave de pez. Nunca se serializan (§14.1).
    pub tokens: HashMap<String, String>,
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
                tokens: HashMap::new(),
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
            Command::Report {
                verdict,
                file,
                blocking,
                major,
                minor,
                mutations_tried,
                mutations_caught,
            } => {
                let key = self.fish_caller(caller)?;
                self.report(
                    &key,
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
            other => self.dispatch_elephant(caller, other),
        }
    }

    /// Los comandos restantes son del elefante del espacio (R1).
    fn dispatch_elephant(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection> {
        let space = self.elephant_space(caller)?;
        match command {
            Command::TaskNew {
                title,
                brief,
                criteria,
            } => self.task_new(&space, title, brief, criteria),
            Command::TeamPropose { seats, card } => self.team_propose(&space, seats, card),
            Command::RoundStart => self.round_start(&space),
            Command::FishNew { role, brief } => self.fish_new(&space, role, brief),
            Command::FishSend { fish, file } => self.fish_send(&space, &fish, file),
            Command::Verify => self.verify(&space),
            Command::RoundClose => self.round_close(&space),
            Command::TaskClose => self.task_close(&space),
            Command::Pause { reason } => self.pause_command(&space, reason),
            Command::Status | Command::Report { .. } => {
                Err(Rejection::new("R1", texts::ELEPHANT_ONLY))
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
    pub fn fish_blocked(&self, key: &FishKey, kind: Option<&str>) {
        let Some(task) = self.task_for_fish_key(key).ok() else {
            return;
        };
        let message = texts::elephant_blocked(&key.fish, kind);
        let data = serde_json::json!({
            "fish": key.fish, "attempt": key.attempt, "kind": kind,
        });
        let event = self.event(&task, "fish_blocked", data.clone());
        let elephant = self.elephant_event(&task, "fish_blocked", message, data);
        self.emit_only(&task.space.id, Some(&task.id), vec![event], vec![elephant]);
    }

    /// La ventana vio que la terminal de un pez terminó antes de informar.
    pub fn fish_exited(&self, key: &FishKey) {
        self.fish_gone(key, "la terminal del pez terminó");
    }

    /// La ventana vio que un pez falló antes de informar (§14.5).
    pub fn fish_failed(&self, key: &FishKey, motive: &str) {
        self.fish_gone(key, motive);
    }

    /// Marca el trabajo como fallido, revoca el token y avisa al elefante.
    fn fish_gone(&self, key: &FishKey, motive: &str) {
        let Some(mut task) = self.task_for_fish_key(key).ok() else {
            return;
        };
        let pending = task
            .pending_work(key)
            .map(|item| item.started_ts)
            .is_some();
        if !pending {
            let message = texts::elephant_exited(&key.fish);
            let data = serde_json::json!({ "fish": key.fish, "attempt": key.attempt });
            let event = self.event(&task, "fish_exited", data.clone());
            let elephant = self.elephant_event(&task, "fish_exited", message, data);
            self.emit_only(&task.space.id, Some(&task.id), vec![event], vec![elephant]);
            return;
        }
        for item in &mut task.work {
            if &item.key == key && item.state == WorkState::Pending {
                item.state = WorkState::Failed;
            }
        }
        self.revoke_token(key);
        let message = texts::fish_failed(&key.fish, motive);
        let data = serde_json::json!({
            "fish": key.fish, "attempt": key.attempt, "reason": motive,
        });
        let mut effects = emit::Effects::default();
        effects.both(
            self.event(&task, "fish_failed", data.clone()),
            self.elephant_event(&task, "fish_failed", message, data),
        );
        self.commit(&task, effects);
    }

    /// Revoca el token vivo de una clave, sin retener el candado.
    pub(crate) fn revoke_token(&self, key: &FishKey) {
        let token = self.inner.lock().tokens.remove(&key.storage_key());
        if let Some(token) = token {
            self.launcher.revoke(&token);
        }
    }

    /// Revoca todos los tokens de una tarea, sin retener el candado.
    pub(crate) fn revoke_task_tokens(&self, task: &Task) {
        for fish in &task.fish {
            self.revoke_token(&fish.key);
        }
    }

    /// Registra el token de un pez recién lanzado.
    pub(crate) fn remember_token(&self, key: &FishKey, token: &str) {
        self.inner
            .lock()
            .tokens
            .insert(key.storage_key(), token.to_string());
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

    /// Pez que llama (R1).
    pub(crate) fn fish_caller(&self, caller: &Caller) -> Result<FishKey, Rejection> {
        match caller {
            Caller::Fish(key) => Ok(key.clone()),
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

    /// Tarea activa que contiene un trabajo de la clave dada (R1).
    pub(crate) fn task_for_fish_key(&self, key: &FishKey) -> Result<Task, Rejection> {
        self.inner
            .lock()
            .tasks
            .values()
            .find(|task| {
                task.is_active()
                    && task
                        .work_for_key(key)
                        .is_some_and(|item| item.state != WorkState::Failed)
            })
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::FISH_NOT_ACTIVE))
    }

    /// Tarea de quien llama, si se puede resolver.
    pub(crate) fn task_for_caller(&self, caller: &Caller) -> Option<Task> {
        match caller {
            Caller::Elephant { space } => self.task_for_elephant(space).ok(),
            Caller::Fish(key) => self.task_for_fish_key(key).ok(),
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

/// Id de tarea: `t` + 12 hex de un uuid v4 (§14.1).
pub(crate) fn new_task_id() -> String {
    let simple = uuid::Uuid::new_v4().simple().to_string();
    format!("t{}", &simple[..12])
}

/// Token nuevo de un pez (uuid v4 simple).
pub(crate) fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Nombre hc: `ez-{8 hex de la tarea}-{pez}`, más `-a{n}` si `attempt > 1`.
pub(crate) fn hc_name(task: &str, fish: &str, attempt: u32) -> String {
    let hex: String = task
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .take(8)
        .collect();
    let suffix = if attempt > 1 {
        format!("-a{attempt}")
    } else {
        String::new()
    };
    format!("ez-{hex}-{fish}{suffix}")
}

/// Id de pez de un papel en una ronda (`dev-r1-2`, `qa-r1`…).
pub(crate) fn fish_id(role: Role, round: u32, index: u32) -> String {
    match role {
        Role::Developer => format!("dev-r{round}-{index}"),
        other => format!("{}-r{round}", other.short()),
    }
}
