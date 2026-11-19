//! El árbitro: estado de las tareas, reglas R1–R12 y eventos.
//!
//! Regla de concurrencia: el `Mutex` interno solo se toma para leer o cambiar
//! el estado en memoria. Git, los comandos de verificación, el lanzamiento de
//! peces, los tokens y el disco ocurren fuera del candado. Cada espacio tiene
//! un ejecutor secuencial (hilo `ez-arbiter-<espacio>`): los comandos se
//! encolan y el llamador espera su resultado (diseño §14.5).

mod agents;
mod api;
mod artifacts;
mod delegate;
mod emit;
mod fish;
mod lifecycle;
mod load;
mod round;
mod task_cmds;
mod team;

pub use api::Arbiter;

use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use ez_team::{builtin, RoleId};

use crate::events::{ArbiterEvent, Card, ElephantEvent};
use crate::state::{Task, WorkState};
use crate::texts;
use crate::types::{
    Caller, Clock, Command, ElephantKey, FishKey, FishLauncher, InstructionWriter, MainAgent,
    Outcome, Rejection, Space,
};

/// Resultado de un trabajo del ejecutor, con tipo borrado para poder encolarlo.
pub(crate) type JobValue = Box<dyn Any + Send>;

/// Trabajo que el ejecutor de un agente procesa por completo antes del siguiente.
type JobRun = Box<dyn FnOnce(&ArbiterShared) -> Result<JobValue, Rejection> + Send>;

/// Petición encolada y canal por el que se devuelve el resultado al llamador.
struct Job {
    run: JobRun,
    done: Sender<Result<JobValue, Rejection>>,
}

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
    /// Cola del agente principal, por `space|elephant` (§15.1).
    pub elephants: HashMap<String, ElephantQueue>,
    /// Cola de novedades de cada pez para `ez wait` de sus hijos (§15.5).
    pub fish_queues: HashMap<String, ElephantQueue>,
    /// Agentes principales, por `space|elephant`.
    pub main_agents: HashMap<String, MainAgent>,
    /// Tokens vivos por clave de pez. Nunca se serializan (§14.1).
    pub tokens: HashMap<String, String>,
    /// Ejecutor secuencial de cada agente principal (§14.5 y §15.1).
    queues: HashMap<String, Sender<Job>>,
    /// Tarjetas abiertas esperando una decisión (§14.8).
    pub cards: HashMap<String, Card>,
}

/// Cola nueva y vacía.
fn elephant_queue() -> ElephantQueue {
    let (tx, rx) = crossbeam_channel::unbounded();
    ElephantQueue { tx, rx }
}

/// Clave de cola y ejecutor de un agente principal.
pub(crate) fn scope(key: &ElephantKey) -> String {
    key.storage_key()
}

/// Acepta claves nuevas (`space|elephant`) y las antiguas (solo `space`).
pub(crate) fn normalize_scope(value: &str) -> String {
    if value.contains('|') {
        value.to_string()
    } else {
        scope(&ElephantKey::default_for(value))
    }
}

/// Id del espacio dentro de una clave de cola.
pub(crate) fn scope_space(value: &str) -> String {
    value.split('|').next().unwrap_or("").to_string()
}

/// Id del agente principal dentro de una clave de cola.
pub(crate) fn scope_elephant(value: &str) -> Option<String> {
    value.split('|').nth(1).map(str::to_string)
}

/// Clave de la cola de novedades de un pez, sin el intento (§15.5).
pub(crate) fn fish_slot(key: &FishKey) -> String {
    format!("{}|{}|{}|{}", key.space, key.task, key.elephant, key.fish)
}

/// Núcleo del árbitro, compartido por la API pública y los hilos ejecutores.
pub(crate) struct ArbiterShared {
    pub(crate) data_dir: PathBuf,
    pub(crate) launcher: Arc<dyn FishLauncher>,
    pub(crate) writer: Arc<dyn InstructionWriter>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) inner: Mutex<Inner>,
    /// Orden monotónico de los eventos de traza en el proceso.
    pub(crate) seq: AtomicU64,
}

impl ArbiterShared {
    /// Construye el núcleo y recarga las tareas no finales.
    fn new(
        data_dir: PathBuf,
        launcher: Arc<dyn FishLauncher>,
        writer: Arc<dyn InstructionWriter>,
        clock: Arc<dyn Clock>,
    ) -> Result<ArbiterShared, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| format!("No se pudo crear {}: {error}.", data_dir.display()))?;
        let loaded = load::load_tasks(&data_dir);
        let mut spaces = HashMap::new();
        let mut elephants = HashMap::new();
        let mut tasks = HashMap::new();
        let mut main_agents: HashMap<String, MainAgent> = agents::load(&data_dir)
            .into_iter()
            .map(|agent| (agent.key.storage_key(), agent))
            .collect();
        for mut task in loaded {
            if task.elephant.elephant.is_empty() {
                task.elephant = ElephantKey::default_for(&task.space.id);
            }
            let key = task.elephant.clone();
            spaces
                .entry(task.space.id.clone())
                .or_insert_with(|| task.space.clone());
            let slot = scope(&key);
            main_agents.entry(slot.clone()).or_insert_with(|| {
                let branch = agents::current_branch(&task.space.folder).unwrap_or_default();
                MainAgent {
                    key: key.clone(),
                    name: "Agente principal".to_string(),
                    worktree: task.space.folder.clone(),
                    branch: branch.clone(),
                    base_branch: branch,
                }
            });
            elephants.entry(slot).or_insert_with(elephant_queue);
            tasks.insert(task.id.clone(), task);
        }
        for agent in main_agents.values() {
            elephants
                .entry(scope(&agent.key))
                .or_insert_with(elephant_queue);
        }
        let shared = ArbiterShared {
            data_dir,
            launcher,
            writer,
            clock,
            inner: Mutex::new(Inner {
                spaces,
                tasks,
                subscribers: Vec::new(),
                elephants,
                fish_queues: HashMap::new(),
                main_agents,
                tokens: HashMap::new(),
                queues: HashMap::new(),
                cards: HashMap::new(),
            }),
            seq: AtomicU64::new(0),
        };
        shared.restart();
        Ok(shared)
    }

    /// Devuelve (y crea, si hace falta) el canal del ejecutor de un espacio.
    /// El `Mutex` solo se toma para consultar e insertar, nunca durante E/S.
    fn queue_sender(self: &Arc<Self>, value: &str) -> Sender<Job> {
        let key = normalize_scope(value);
        let (sender, receiver) = {
            let mut inner = self.inner.lock();
            if let Some(sender) = inner.queues.get(&key) {
                return sender.clone();
            }
            let (sender, receiver) = crossbeam_channel::unbounded();
            inner.queues.insert(key.clone(), sender.clone());
            (sender, receiver)
        };
        let shared = self.clone();
        let name = format!("ez-arbiter-{key}");
        std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                for job in receiver {
                    let result = (job.run)(&shared);
                    let _ = job.done.send(result);
                }
            })
            .expect("crear el hilo ejecutor del árbitro");
        sender
    }

    /// Registra o actualiza un espacio (lo llama la ventana al abrirlo).
    pub(crate) fn register_space(&self, space: Space) {
        let key = ElephantKey::default_for(&space.id);
        let slot = scope(&key);
        let exists = self.inner.lock().main_agents.contains_key(&slot);
        let branch = if exists {
            String::new()
        } else {
            agents::current_branch(&space.folder).unwrap_or_default()
        };
        let mut inner = self.inner.lock();
        inner
            .main_agents
            .entry(slot.clone())
            .or_insert_with(|| MainAgent {
                key,
                name: "Agente principal".to_string(),
                worktree: space.folder.clone(),
                branch: branch.clone(),
                base_branch: branch,
            });
        inner.elephants.entry(slot).or_insert_with(elephant_queue);
        inner.spaces.insert(space.id.clone(), space);
    }

    /// Ejecuta un comando del elefante o de un pez.
    pub(crate) fn dispatch(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection> {
        if let Caller::Elephant(key) = caller {
            self.check_elephant_changes(&scope(key))?;
        }
        match command {
            Command::Status => self.status(caller),
            Command::AgentDelegate { role, brief } => {
                let key = self.fish_caller(caller)?;
                self.agent_delegate(&key, role, brief)
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
        let scope = self.elephant_scope(caller)?;
        match command {
            Command::TaskNew {
                title,
                brief,
                criteria,
            } => self.task_new(&scope, title, brief, criteria),
            Command::TeamPropose { seats, roles, card } => {
                self.team_propose(&scope, seats, roles, card)
            }
            Command::RoundStart => self.round_start(&scope),
            Command::FishNew { role, brief } => self.fish_new(&scope, role, brief),
            Command::FishSend { fish, file } => self.fish_send(&scope, &fish, file),
            Command::Verify => self.verify(&scope),
            Command::RoundClose => self.round_close(&scope),
            Command::TaskClose => self.task_close(&scope),
            Command::Pause { reason } => self.pause_command(&scope, reason),
            Command::Status | Command::Report { .. } | Command::AgentDelegate { .. } => {
                Err(Rejection::new("R1", texts::ELEPHANT_ONLY))
            }
        }
    }

    // -------------------------------------------------------------- personas

    /// La persona aprueba la tarea propuesta.
    pub(crate) fn approve_task_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        self.approve_task_impl(space)
    }

    /// La persona rechaza la tarea con un comentario para el elefante.
    pub(crate) fn reject_task_inner(
        &self,
        space: &str,
        comment: &str,
    ) -> Result<Outcome, Rejection> {
        self.reject_task_impl(space, comment)
    }

    /// La persona aprueba el equipo propuesto.
    pub(crate) fn approve_team_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        self.approve_team_impl(space)
    }

    /// La persona rechaza el equipo con un comentario para el elefante.
    pub(crate) fn reject_team_inner(
        &self,
        space: &str,
        comment: &str,
    ) -> Result<Outcome, Rejection> {
        self.reject_team_impl(space, comment)
    }

    /// La persona reanuda una tarea en pausa.
    pub(crate) fn resume_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        self.resume_impl(space)
    }

    /// La persona detiene la tarea.
    pub(crate) fn stop_inner(&self, space: &str) -> Result<Outcome, Rejection> {
        self.stop_impl(space)
    }

    /// Despacha una acción de tarjeta al método correcto (diseño §14.8).
    pub(crate) fn card_action_inner(
        &self,
        id: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Outcome, Rejection> {
        let card = self
            .inner
            .lock()
            .cards
            .get(id)
            .cloned()
            .ok_or_else(|| Rejection::new("R12", "No hay una tarjeta abierta con ese id."))?;
        let result = match (card.kind, action) {
            (crate::events::CardKind::Task, "approve") => self.approve_task_impl(&card.space),
            (crate::events::CardKind::Task, "reject") => {
                self.reject_task_impl(&card.space, comment.unwrap_or(""))
            }
            (crate::events::CardKind::Team, "approve") => self.approve_team_impl(&card.space),
            (crate::events::CardKind::Team, "reject") => {
                self.reject_team_impl(&card.space, comment.unwrap_or(""))
            }
            (crate::events::CardKind::Pause, "resume") => self.resume_impl(&card.space),
            (crate::events::CardKind::Pause, "more_time") => self.more_time_impl(&card.space),
            (crate::events::CardKind::Pause, "stop") => self.stop_impl(&card.space),
            (crate::events::CardKind::Integrate, "merge") => self.integrate_impl(&card),
            (crate::events::CardKind::Integrate, "keep") => {
                Ok(Outcome::new("El trabajo se queda en su rama.".to_string()))
            }
            (kind, other) => {
                return Err(Rejection::new(
                    "R12",
                    format!(
                        "La acción {other} no corresponde a una tarjeta de tipo {}.",
                        kind.id()
                    ),
                ));
            }
        };
        if result.is_ok() {
            self.close_card(id);
        }
        result
    }

    // --------------------------------------------------------------- señales

    /// La ventana vio que un pez quedó bloqueado en hc.
    pub(crate) fn fish_blocked(&self, key: &FishKey, kind: Option<&str>) {
        let Some(task) = self.task_for_fish_key(key).ok() else {
            return;
        };
        let message = texts::elephant_blocked(&key.fish, kind);
        let data = serde_json::json!({
            "fish": key.fish, "attempt": key.attempt, "kind": kind,
            "parent": key.parent,
        });
        let event = self.event(&task, "fish_blocked", data.clone());
        let elephant = self.elephant_event(&task, "fish_blocked", message, data);
        let scope = scope(&task.elephant);
        self.emit_only(&scope, Some(&task.id), vec![event], vec![elephant]);
    }

    /// La ventana vio que la terminal de un pez terminó antes de informar.
    pub(crate) fn fish_exited(&self, key: &FishKey) {
        self.fish_gone(key, "la terminal del pez terminó");
    }

    /// La ventana vio que un pez falló antes de informar (§14.5).
    pub(crate) fn fish_failed(&self, key: &FishKey, motive: &str) {
        self.fish_gone(key, motive);
    }

    /// Marca el trabajo como fallido, revoca el token y avisa al elefante.
    fn fish_gone(&self, key: &FishKey, motive: &str) {
        let Some(mut task) = self.task_for_fish_key(key).ok() else {
            return;
        };
        let pending = task.pending_work(key).map(|item| item.started_ts).is_some();
        if !pending {
            let message = texts::elephant_exited(&key.fish);
            let data = serde_json::json!({
                "fish": key.fish, "attempt": key.attempt, "parent": key.parent,
            });
            let event = self.event(&task, "fish_exited", data.clone());
            let elephant = self.elephant_event(&task, "fish_exited", message, data);
            let scope = scope(&task.elephant);
            self.emit_only(&scope, Some(&task.id), vec![event], vec![elephant]);
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
    pub(crate) fn wait_for_elephant(
        &self,
        space: &str,
        timeout: Duration,
    ) -> Option<ElephantEvent> {
        self.wait_for_main(&ElephantKey::default_for(space), timeout)
    }

    /// Espera una novedad de un agente principal concreto (§15.1).
    pub(crate) fn wait_for_main(
        &self,
        key: &ElephantKey,
        timeout: Duration,
    ) -> Option<ElephantEvent> {
        let slot = scope(key);
        let receiver = {
            let inner = self.inner.lock();
            inner.elephants.get(&slot).map(|queue| queue.rx.clone())
        }?;
        receiver.recv_timeout(timeout).ok()
    }

    /// Espera novedades de los hijos de un pez (§15.5).
    pub(crate) fn wait_for_fish(&self, key: &FishKey, timeout: Duration) -> Option<ElephantEvent> {
        let receiver = {
            let inner = self.inner.lock();
            inner
                .fish_queues
                .get(&fish_slot(key))
                .map(|queue| queue.rx.clone())
        }?;
        receiver.recv_timeout(timeout).ok()
    }

    /// Suscribe la vista a la traza.
    pub(crate) fn subscribe(&self) -> Receiver<ArbiterEvent> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        self.inner.lock().subscribers.push(sender);
        receiver
    }

    /// Estado de la tarea del espacio para la vista, con `last_seq` (§14.8).
    pub(crate) fn snapshot(&self, space: &str) -> Option<serde_json::Value> {
        self.snapshot_main(&ElephantKey::default_for(space))
    }

    /// Instantánea de la tarea activa de un agente principal (§15.1).
    pub(crate) fn snapshot_main(&self, key: &ElephantKey) -> Option<serde_json::Value> {
        let wanted = scope(key);
        let inner = self.inner.lock();
        let task = inner
            .tasks
            .values()
            .find(|task| task.is_active() && scope(&task.elephant) == wanted)?;
        let mut snapshot = task.snapshot();
        if let Some(object) = snapshot.as_object_mut() {
            object.insert(
                "last_seq".to_string(),
                serde_json::json!(self.seq.load(Ordering::Relaxed)),
            );
        }
        Some(snapshot)
    }

    // --------------------------------------------------------------- ayudas

    /// Reloj del árbitro.
    pub(crate) fn now(&self) -> f64 {
        self.clock.now()
    }

    /// Espacio registrado a partir de quien llama.
    pub(crate) fn elephant_scope(&self, caller: &Caller) -> Result<String, Rejection> {
        let Caller::Elephant(key) = caller else {
            return Err(Rejection::new("R1", texts::ELEPHANT_ONLY));
        };
        if self.inner.lock().spaces.contains_key(&key.space) {
            Ok(scope(key))
        } else {
            Err(Rejection::new("R1", texts::UNKNOWN_SPACE))
        }
    }

    /// Pez que llama (R1).
    pub(crate) fn fish_caller(&self, caller: &Caller) -> Result<FishKey, Rejection> {
        match caller {
            Caller::Fish(key) => Ok(key.clone()),
            Caller::Elephant(_) => Err(Rejection::new("R1", texts::FISH_ONLY)),
        }
    }

    /// Espacio registrado, o error de R1.
    pub(crate) fn space(&self, value: &str) -> Result<Space, Rejection> {
        let id = scope_space(value);
        self.inner
            .lock()
            .spaces
            .get(&id)
            .cloned()
            .ok_or_else(|| Rejection::new("R1", texts::UNKNOWN_SPACE))
    }

    /// Tarea activa de un agente principal (una por agente, §15.1).
    pub(crate) fn task_for_elephant(&self, value: &str) -> Result<Task, Rejection> {
        self.space(value)?;
        let wanted = normalize_scope(value);
        self.inner
            .lock()
            .tasks
            .values()
            .find(|task| task.is_active() && scope(&task.elephant) == wanted)
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
                    && task.id == key.task
                    && task.elephant.elephant == key.elephant
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
            Caller::Elephant(key) => self.task_for_elephant(&scope(key)).ok(),
            Caller::Fish(key) => self.task_for_fish_key(key).ok(),
        }
    }

    /// Deja constancia de un rechazo de una acción de la persona.
    fn log_person_rejection(&self, space: &str, rejection: &Rejection) {
        let task = self.task_for_elephant(space).ok();
        let event = ArbiterEvent {
            seq: self.next_seq(),
            kind: crate::events::EventKind::Rejected {
                rule: rejection.rule.to_string(),
                message: rejection.message.clone(),
            },
            space: task
                .as_ref()
                .map(|task| task.space.id.clone())
                .unwrap_or_else(|| scope_space(space)),
            elephant: scope_elephant(space),
            task: task.as_ref().map(|task| task.id.clone()),
            ts: self.now(),
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
                serde_json::json!({ "rule": rejection.rule, "message": rejection.message }),
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
pub(crate) fn fish_id(role: &RoleId, round: u32, index: u32, indexed: bool) -> String {
    let short = role.short();
    if indexed {
        format!("{short}-r{round}-{index}")
    } else {
        format!("{short}-r{round}")
    }
}

/// Los papeles con varios puestos (y el desarrollador incluido) llevan índice en su id.
pub(crate) fn is_indexed(role: &RoleId, count: u32) -> bool {
    role.as_str() == builtin::DEVELOPER || count > 1
}
