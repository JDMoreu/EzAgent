//! Arranque del árbitro: claves de cola, estado restaurado y ejecutores.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::Mutex;

use crate::events::ElephantEvent;
use crate::state::Task;
use crate::types::{
    Clock, ElephantKey, FishKey, FishLauncher, InstructionWriter, MainAgent, Space,
};

use super::{agents, load, ArbiterShared, Inner, Job};

/// Cola nueva y vacía.
pub(crate) fn elephant_queue() -> ElephantQueue {
    let (tx, rx) = crossbeam_channel::unbounded();
    ElephantQueue { tx, rx }
}

/// Cola de novedades de un agente principal para `ez wait`.
#[derive(Clone)]
pub(crate) struct ElephantQueue {
    pub tx: Sender<ElephantEvent>,
    pub rx: crossbeam_channel::Receiver<ElephantEvent>,
}

/// Clave de cola y ejecutor de un agente principal.
pub(crate) fn scope(key: &ElephantKey) -> String {
    key.storage_key()
}

/// Acepta claves nuevas (`space|elephant`) y las antiguas (solo `space`).
// DECISION: las APIs de una sola tarea siguen recibiendo el id del espacio y
// se resuelven al agente por omisión; las nuevas reciben `ElephantKey` (§15.1).
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

/// Estado leído del disco al arrancar.
type Restored = (
    HashMap<String, Space>,
    HashMap<String, ElephantQueue>,
    HashMap<String, Task>,
    HashMap<String, MainAgent>,
);

/// Recarga tareas y agentes, dando un agente por omisión a las tareas antiguas.
fn restore(data_dir: &std::path::Path) -> Restored {
    let mut spaces = HashMap::new();
    let mut elephants = HashMap::new();
    let mut tasks = HashMap::new();
    let mut main_agents: HashMap<String, MainAgent> = agents::load(data_dir)
        .into_iter()
        .map(|agent| (agent.key.storage_key(), agent))
        .collect();
    for mut task in load::load_tasks(data_dir) {
        if task.elephant.elephant.is_empty() {
            task.elephant = ElephantKey::default_for(&task.space.id);
        }
        let key = task.elephant.clone();
        spaces
            .entry(task.space.id.clone())
            .or_insert_with(|| task.space.clone());
        let slot = scope(&key);
        main_agents
            .entry(slot.clone())
            .or_insert_with(|| default_agent(&key, &task.space.folder));
        elephants.entry(slot).or_insert_with(elephant_queue);
        tasks.insert(task.id.clone(), task);
    }
    for agent in main_agents.values() {
        elephants
            .entry(scope(&agent.key))
            .or_insert_with(elephant_queue);
    }
    (spaces, elephants, tasks, main_agents)
}

/// Agente principal por omisión: trabaja en la carpeta original del espacio.
fn default_agent(key: &ElephantKey, folder: &std::path::Path) -> MainAgent {
    // DECISION: un espacio sin `agentes.json` tiene un agente por omisión en la
    // carpeta original, sin `git worktree add`; así se conservan las tareas
    // creadas antes de que existieran los worktrees.
    let branch = agents::current_branch(folder).unwrap_or_default();
    MainAgent {
        key: key.clone(),
        name: "Agente principal".to_string(),
        worktree: folder.to_path_buf(),
        branch: branch.clone(),
        base_branch: branch,
    }
}

impl ArbiterShared {
    /// Construye el núcleo y recarga las tareas no finales.
    pub(crate) fn new(
        data_dir: PathBuf,
        launcher: Arc<dyn FishLauncher>,
        writer: Arc<dyn InstructionWriter>,
        clock: Arc<dyn Clock>,
    ) -> Result<ArbiterShared, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| format!("No se pudo crear {}: {error}.", data_dir.display()))?;
        let (spaces, elephants, tasks, main_agents) = restore(&data_dir);
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

    /// Devuelve (y crea, si hace falta) el canal del ejecutor de un agente.
    /// El `Mutex` solo se toma para consultar e insertar, nunca durante E/S.
    pub(crate) fn queue_sender(self: &Arc<Self>, value: &str) -> Sender<Job> {
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
        let agent = if exists {
            None
        } else {
            Some(default_agent(&key, &space.folder))
        };
        let mut inner = self.inner.lock();
        if let Some(agent) = agent {
            inner.main_agents.entry(slot.clone()).or_insert(agent);
        }
        inner.elephants.entry(slot).or_insert_with(elephant_queue);
        inner.spaces.insert(space.id.clone(), space);
    }
}
