//! Port of `src/ezagent/voice/watcher.py` (F4-10).
//!
//! Vigilante de agentes de Herdr: polling de `hc ls --json`. `hc events` puede
//! perder el cambio de estado de un agente que termina y se queda quieto (o
//! cuyo terminal desaparece); un polling es más simple y fiable para avisar
//! cuando un agente concreto termina.
//!
//! El bucle de polling solo publica eventos en una cola; un hilo propio los
//! entrega para que un callback lento no retrase la detección del siguiente
//! cambio. Una consulta fallida nunca se interpreta como «no hay terminales»:
//! se conserva el último snapshot y se reintenta con backoff.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use ez_core::pyloads::loads;
use ez_core::pyvalue::{py_eq, py_strip, python_str, truthy};
use serde_json::{Map, Value};

use crate::herdr::{self, HerdrRuntime};

pub const POLL_S: f64 = 1.0;
pub const IDLE_POLL_S: f64 = 5.0;
pub const BACKOFF_S: [f64; 3] = [1.0, 2.0, 5.0];
pub const STOP_JOIN_S: f64 = 5.0;

/// `(address, agent, state, reason)`.
pub type Event = (String, String, String, String);
pub type EventCallback = Arc<dyn Fn(&str, &str, &str, &str) + Send + Sync>;
pub type HealthCallback = Arc<dyn Fn(Option<bool>) + Send + Sync>;
pub type AnnounceCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;
pub type TargetsCallback = Arc<dyn Fn() -> bool + Send + Sync>;

/// Tarea que corre un hilo de `start()`.
type Work = Box<dyn FnOnce() + Send + 'static>;

/// Hilo vivo con nombre (el `threading.Thread` de `start`/`stop`).
#[doc(hidden)]
pub trait Worker: Send + Sync {
    fn is_alive(&self) -> bool;
    fn join(&self, timeout_s: f64);
}

/// Fábrica de hilos de `start()`; el arnés la sustituye por un doble.
#[doc(hidden)]
pub trait ThreadFactory: Send + Sync {
    fn spawn(&self, name: &str, work: Work) -> Option<Box<dyn Worker>>;
}

struct SystemThreads;

struct SystemWorker {
    handle: Mutex<Option<JoinHandle<()>>>,
    done: Mutex<Receiver<()>>,
}

impl Worker for SystemWorker {
    fn is_alive(&self) -> bool {
        lock(&self.handle)
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    fn join(&self, timeout_s: f64) {
        let _ = lock(&self.done).recv_timeout(Duration::from_secs_f64(timeout_s.max(0.0)));
    }
}

impl ThreadFactory for SystemThreads {
    fn spawn(&self, name: &str, work: Work) -> Option<Box<dyn Worker>> {
        let (done_sender, done) = channel();
        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                work();
                let _ = done_sender.send(());
            });
        match handle {
            Ok(handle) => Some(Box::new(SystemWorker {
                handle: Mutex::new(Some(handle)),
                done: Mutex::new(done),
            })),
            Err(error) => {
                tracing::warn!("Watcher: no pude iniciar el hilo {name}: {error}");
                None
            }
        }
    }
}

/// `threading.Event` con espera interrumpible.
struct StopFlag {
    flag: Mutex<bool>,
    changed: Condvar,
}

impl StopFlag {
    fn new() -> Self {
        Self {
            flag: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    fn set(&self) {
        let mut flag = lock(&self.flag);
        *flag = true;
        self.changed.notify_all();
    }

    fn is_set(&self) -> bool {
        *lock(&self.flag)
    }

    /// `Event.wait(timeout)`: True si ya estaba puesto.
    fn wait(&self, seconds: f64) -> bool {
        let flag = lock(&self.flag);
        if *flag {
            return true;
        }
        let (flag, _) = self
            .changed
            .wait_timeout(flag, Duration::from_secs_f64(seconds.max(0.0)))
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *flag
    }
}

fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Previous {
    entries: Vec<(String, String, String)>,
    primed: bool,
}

pub struct Watcher {
    hc_path: String,
    on_event: EventCallback,
    should_announce: AnnounceCallback,
    on_health: Option<HealthCallback>,
    has_targets: Option<TargetsCallback>,
    healthy: Mutex<Value>,
    stop: StopFlag,
    sender: Sender<Option<Event>>,
    receiver: Mutex<Receiver<Option<Event>>>,
    poll: Mutex<Option<Box<dyn Worker>>>,
    delivery: Mutex<Option<Box<dyn Worker>>>,
    previous: Mutex<Previous>,
    live: Mutex<HashMap<String, String>>,
    runtime: Arc<dyn HerdrRuntime>,
    threads: Arc<dyn ThreadFactory>,
}

impl Watcher {
    pub fn new(
        hc_path: impl Into<String>,
        on_event: EventCallback,
        should_announce: AnnounceCallback,
        on_health: Option<HealthCallback>,
        has_targets: Option<TargetsCallback>,
    ) -> Self {
        let (sender, receiver) = channel();
        Self {
            hc_path: hc_path.into(),
            on_event,
            should_announce,
            on_health,
            has_targets,
            healthy: Mutex::new(Value::Bool(true)),
            stop: StopFlag::new(),
            sender,
            receiver: Mutex::new(receiver),
            poll: Mutex::new(None),
            delivery: Mutex::new(None),
            previous: Mutex::new(Previous {
                entries: Vec::new(),
                primed: false,
            }),
            live: Mutex::new(HashMap::new()),
            runtime: herdr::system_runtime(),
            threads: Arc::new(SystemThreads),
        }
    }

    /// `subprocess.run` inyectable (el oráculo lo sustituye por un doble).
    #[doc(hidden)]
    pub fn with_runtime(mut self, runtime: Arc<dyn HerdrRuntime>) -> Self {
        self.runtime = runtime;
        self
    }

    /// Fábrica de hilos inyectable (doble de `threading.Thread`).
    #[doc(hidden)]
    pub fn with_threads(mut self, threads: Arc<dyn ThreadFactory>) -> Self {
        self.threads = threads;
        self
    }

    /// Estado de la terminal en la última consulta buena; None si no existe.
    pub fn state_of(&self, address: &str) -> Option<String> {
        lock(&self.live).get(address).cloned()
    }

    pub fn healthy(&self) -> bool {
        truthy(&lock(&self.healthy))
    }

    /// `_live` como diccionario (costura de paridad).
    #[doc(hidden)]
    pub fn live_map(&self) -> Map<String, Value> {
        lock(&self.live)
            .iter()
            .map(|(address, state)| (address.clone(), Value::String(state.clone())))
            .collect()
    }

    /// `_prev` como diccionario `dirección -> [estado, agente]`.
    #[doc(hidden)]
    pub fn previous_map(&self) -> Map<String, Value> {
        lock(&self.previous)
            .entries
            .iter()
            .map(|(address, state, agent)| {
                (
                    address.clone(),
                    Value::Array(vec![
                        Value::String(state.clone()),
                        Value::String(agent.clone()),
                    ]),
                )
            })
            .collect()
    }

    /// Primer evento de la cola sin bloquear: `Some(None)` es el centinela.
    #[doc(hidden)]
    pub fn next_event_now(&self) -> Option<Option<Event>> {
        lock(&self.receiver).try_recv().ok()
    }

    #[doc(hidden)]
    pub fn stop_requested(&self) -> bool {
        self.stop.is_set()
    }

    /// `_deliver`: entrega de la cola (`ez-watch-events`). Costura pública.
    #[doc(hidden)]
    pub fn deliver(&self) {
        self.deliver_loop();
    }

    /// `_loop`: sondeo con backoff (`ez-watch-poll`). Costura pública.
    #[doc(hidden)]
    pub fn poll_loop(&self) {
        self.poll_loop_inner();
    }

    pub fn start(self: &Arc<Self>) {
        let mut delivery = lock(&self.delivery);
        if delivery.as_ref().is_none_or(|worker| !worker.is_alive()) {
            *delivery = self.spawn_worker("ez-watch-events", |watcher| watcher.deliver_loop());
        }
        drop(delivery);
        let mut poll = lock(&self.poll);
        if poll.as_ref().is_some_and(|worker| worker.is_alive()) {
            return;
        }
        *poll = self.spawn_worker("ez-watch-poll", |watcher| watcher.poll_loop_inner());
    }

    /// `stop()`: avisa a los hilos y espera como mucho `STOP_JOIN_S` por cada uno.
    ///
    /// Los hilos que sigan vivos se dejan terminar solos (en Python también son
    /// daemon); igual que allí, un `hc` colgado no bloquea la salida.
    pub fn stop(&self) {
        self.stop.set();
        let _ = self.sender.send(None);
        for slot in [&self.poll, &self.delivery] {
            let worker = lock(slot);
            if let Some(worker) = worker.as_ref() {
                if worker.is_alive() {
                    worker.join(STOP_JOIN_S);
                }
            }
        }
    }

    fn spawn_worker(self: &Arc<Self>, name: &str, work: fn(&Watcher)) -> Option<Box<dyn Worker>> {
        let watcher = Arc::clone(self);
        self.threads.spawn(name, Box::new(move || work(&watcher)))
    }

    fn deliver_loop(&self) {
        loop {
            let event = lock(&self.receiver).recv();
            match event {
                Ok(Some((address, agent, state, reason))) => {
                    let callback = Arc::clone(&self.on_event);
                    // Un aviso que falla no debe matar el hilo de entrega.
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        callback(&address, &agent, &state, &reason);
                    }));
                    if result.is_err() {
                        tracing::debug!("Watcher: el callback falló: callback");
                    }
                }
                Ok(None) | Err(_) => return,
            }
        }
    }

    fn poll_loop_inner(&self) {
        let mut failures = 0usize;
        while !self.stop.is_set() {
            let delay = if self.poll_once() {
                failures = 0;
                self.set_health(true);
                if self.wants_fast_poll() {
                    POLL_S
                } else {
                    IDLE_POLL_S
                }
            } else {
                failures += 1;
                self.set_health(false);
                BACKOFF_S[failures.min(BACKOFF_S.len()) - 1]
            };
            if self.stop.wait(delay) {
                return;
            }
        }
    }
    #[doc(hidden)]
    pub fn wants_fast_poll(&self) -> bool {
        match &self.has_targets {
            None => true,
            Some(callback) => {
                let callback = Arc::clone(callback);
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()))
                    .unwrap_or(true)
            }
        }
    }

    /// `_set_health(bool)`.
    #[doc(hidden)]
    pub fn set_health(&self, healthy: bool) {
        self.set_health_value(Value::Bool(healthy));
    }

    /// `_set_health` de Python: guarda el valor tal cual (también `None`) y lo
    /// avisa si cambió. Devuelve el `healthy` resultante.
    #[doc(hidden)]
    pub fn set_health_value(&self, healthy: Value) -> Value {
        {
            let mut current = lock(&self.healthy);
            if py_eq(&healthy, &current) {
                return current.clone();
            }
            *current = healthy.clone();
        }
        let Some(callback) = &self.on_health else {
            return healthy;
        };
        let callback = Arc::clone(callback);
        let value = healthy.as_bool();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(value)));
        if result.is_err() {
            tracing::debug!("Watcher: on_health falló: callback");
        }
        healthy
    }

    /// Consulta `hc ls`; false si el resultado no es fiable.
    #[doc(hidden)]
    pub fn poll_once(&self) -> bool {
        let argv = vec![self.hc_path.clone(), "ls".to_string(), "--json".to_string()];
        let Ok(result) = self.runtime.run(&argv, 10.0, false) else {
            return false;
        };
        if result.returncode != 0 {
            return false;
        }
        let output = py_strip(&result.stdout);
        if output.is_empty() {
            return false;
        }
        let Ok(data) = loads(output) else {
            return false;
        };
        // DECISION: un objeto sin "terminals" es una respuesta incompleta, no
        // una lista vacía; comparar contra [] anunciaría finales falsos.
        let Some(terminals) = data
            .as_object()
            .and_then(|map| map.get("terminals"))
            .and_then(Value::as_array)
        else {
            return false;
        };
        self.compare(terminals);
        true
    }

    #[doc(hidden)]
    pub fn compare(&self, terminals: &[Value]) {
        let current = collect(terminals);
        self.store_live(&current);
        let mut previous = lock(&self.previous);
        if !previous.primed {
            previous.entries.clone_from(&current);
            previous.primed = true;
            return;
        }
        self.announce_lost(&current, &mut previous);
        self.announce_transitions(&current, &mut previous);
    }

    fn store_live(&self, current: &[(String, String, String)]) {
        let mut live = lock(&self.live);
        live.clear();
        for (address, state, _) in current {
            live.insert(address.clone(), state.clone());
        }
    }

    /// Desaparecer no es terminar: el desenlace es desconocido ("lost"). Solo
    /// una transición observada working -> done/idle se anuncia como done.
    fn announce_lost(&self, current: &[(String, String, String)], previous: &mut Previous) {
        let lost: Vec<(String, String, String)> = previous
            .entries
            .iter()
            .filter(|(address, _, _)| !has_address(current, address))
            .cloned()
            .collect();
        for (address, state, agent) in &lost {
            if state != "done" && state != "idle" && (self.should_announce)(address) {
                self.publish(address, agent, "lost", "lost");
            }
        }
        previous
            .entries
            .retain(|(address, _, _)| has_address(current, address));
    }

    fn announce_transitions(&self, current: &[(String, String, String)], previous: &mut Previous) {
        for (address, state, agent) in current {
            let position = previous
                .entries
                .iter()
                .position(|(other, _, _)| other == address);
            let Some(position) = position else {
                // Terminal nuevo: se registra sin avisar.
                previous
                    .entries
                    .push((address.clone(), state.clone(), agent.clone()));
                continue;
            };
            let entry = &mut previous.entries[position];
            let previous_state = entry.1.clone();
            entry.1.clone_from(state);
            entry.2.clone_from(agent);
            if previous_state == *state || !(self.should_announce)(address) {
                continue;
            }
            let reason = if state == "blocked" {
                Some("blocked")
            } else if (state == "done" || state == "idle") && previous_state == "working" {
                Some("done")
            } else if state == "error" {
                Some("error")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.publish(address, agent, state, reason);
            }
        }
    }

    fn publish(&self, address: &str, agent: &str, state: &str, reason: &str) {
        let _ = self.sender.send(Some((
            address.to_string(),
            agent.to_string(),
            state.to_string(),
            reason.to_string(),
        )));
    }
}

fn has_address(current: &[(String, String, String)], address: &str) -> bool {
    current.iter().any(|(other, _, _)| other == address)
}

fn collect(terminals: &[Value]) -> Vec<(String, String, String)> {
    let mut current: Vec<(String, String, String)> = Vec::new();
    for terminal in terminals {
        let Some(terminal) = terminal.as_object() else {
            continue;
        };
        let Some(status) = terminal.get("status") else {
            continue;
        };
        let (state, agent) = match status {
            Value::Object(status) => (
                text_or(status.get("state"), "idle"),
                text_or(
                    status.get("app").or_else(|| terminal.get("agent")),
                    "agente",
                ),
            ),
            Value::String(state) => (state.clone(), text_or(terminal.get("agent"), "agente")),
            _ => continue,
        };
        let address = text_or(terminal.get("address"), "");
        if address.is_empty() {
            continue;
        }
        match current.iter_mut().find(|(other, _, _)| *other == address) {
            Some(entry) => {
                entry.1 = state;
                entry.2 = agent;
            }
            None => current.push((address, state, agent)),
        }
    }
    current
}

/// `str(value or fallback)`.
fn text_or(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests;
