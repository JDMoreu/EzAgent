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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use ez_core::pyloads::loads;
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::Value;

use crate::herdr::process;

pub const POLL_S: f64 = 1.0;
pub const IDLE_POLL_S: f64 = 5.0;
pub const BACKOFF_S: [f64; 3] = [1.0, 2.0, 5.0];
pub const STOP_JOIN_S: f64 = 5.0;

/// `(address, agent, state, reason)`.
pub type Event = (String, String, String, String);
pub type EventCallback = Arc<dyn Fn(&str, &str, &str, &str) + Send + Sync>;
pub type HealthCallback = Arc<dyn Fn(bool) + Send + Sync>;
pub type AnnounceCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;
pub type TargetsCallback = Arc<dyn Fn() -> bool + Send + Sync>;

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

struct Started {
    handle: JoinHandle<()>,
    done: Receiver<()>,
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
    healthy: AtomicBool,
    stop: StopFlag,
    sender: Sender<Option<Event>>,
    receiver: Mutex<Receiver<Option<Event>>>,
    poll: Mutex<Option<Started>>,
    delivery: Mutex<Option<Started>>,
    previous: Mutex<Previous>,
    live: Mutex<HashMap<String, String>>,
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
            healthy: AtomicBool::new(true),
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
        }
    }

    /// Estado de la terminal en la última consulta buena; None si no existe.
    pub fn state_of(&self, address: &str) -> Option<String> {
        lock(&self.live).get(address).cloned()
    }

    pub fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    pub fn start(self: &Arc<Self>) {
        let mut delivery = lock(&self.delivery);
        if delivery
            .as_ref()
            .is_none_or(|started| started.handle.is_finished())
        {
            *delivery = self.spawn_worker("ez-watch-events", |watcher| watcher.deliver());
        }
        drop(delivery);
        let mut poll = lock(&self.poll);
        if poll
            .as_ref()
            .is_some_and(|started| !started.handle.is_finished())
        {
            return;
        }
        *poll = self.spawn_worker("ez-watch-poll", |watcher| watcher.poll_loop());
    }

    /// `stop()`: avisa a los hilos y espera como mucho `STOP_JOIN_S` por cada uno.
    ///
    /// Los hilos que sigan vivos se dejan terminar solos (en Python también son
    /// daemon); igual que allí, un `hc` colgado no bloquea la salida.
    pub fn stop(&self) {
        self.stop.set();
        let _ = self.sender.send(None);
        for slot in [&self.poll, &self.delivery] {
            let started = lock(slot);
            if let Some(started) = started.as_ref() {
                if !started.handle.is_finished() {
                    let _ = started
                        .done
                        .recv_timeout(Duration::from_secs_f64(STOP_JOIN_S));
                }
            }
        }
    }

    fn spawn_worker(self: &Arc<Self>, name: &str, work: fn(&Watcher)) -> Option<Started> {
        let watcher = Arc::clone(self);
        let (done_sender, done) = channel();
        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                work(&watcher);
                let _ = done_sender.send(());
            });
        match handle {
            Ok(handle) => Some(Started { handle, done }),
            Err(error) => {
                tracing::warn!("Watcher: no pude iniciar el hilo {name}: {error}");
                None
            }
        }
    }

    fn deliver(&self) {
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

    fn poll_loop(&self) {
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

    fn wants_fast_poll(&self) -> bool {
        match &self.has_targets {
            None => true,
            Some(callback) => {
                let callback = Arc::clone(callback);
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()))
                    .unwrap_or(true)
            }
        }
    }

    fn set_health(&self, healthy: bool) {
        if healthy == self.healthy() {
            return;
        }
        self.healthy.store(healthy, Ordering::SeqCst);
        let Some(callback) = &self.on_health else {
            return;
        };
        let callback = Arc::clone(callback);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(healthy)));
        if result.is_err() {
            tracing::debug!("Watcher: on_health falló: callback");
        }
    }

    /// Consulta `hc ls`; false si el resultado no es fiable.
    fn poll_once(&self) -> bool {
        let argv = vec![self.hc_path.clone(), "ls".to_string(), "--json".to_string()];
        let Ok(result) = process::run(&argv, 10.0, false) else {
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

    fn compare(&self, terminals: &[Value]) {
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
