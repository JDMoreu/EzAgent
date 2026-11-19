//! Persistencia y emisión de eventos del árbitro.

use crate::events::{ArbiterEvent, ElephantEvent};
use crate::state::Task;

use super::Arbiter;

/// Eventos que produce una acción: traza (vista y bitácora) y novedades
/// para el elefante.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    pub events: Vec<ArbiterEvent>,
    pub elephant: Vec<ElephantEvent>,
}

impl Effects {
    /// Agrega un evento de traza y un mensaje para el elefante.
    pub fn both(&mut self, event: ArbiterEvent, elephant: ElephantEvent) {
        self.events.push(event);
        self.elephant.push(elephant);
    }

    /// Agrega solo un evento de traza.
    pub fn event(&mut self, event: ArbiterEvent) {
        self.events.push(event);
    }
}

impl Arbiter {
    /// Evento de traza de una tarea.
    pub(crate) fn event(&self, task: &Task, kind: &str, data: serde_json::Value) -> ArbiterEvent {
        ArbiterEvent {
            kind: kind.to_string(),
            space: task.space.id.clone(),
            task: Some(task.id.clone()),
            ts: self.now(),
            data,
        }
    }

    /// Evento para `ez wait` de una tarea.
    pub(crate) fn elephant_event(
        &self,
        task: &Task,
        kind: &str,
        message: String,
        data: serde_json::Value,
    ) -> ElephantEvent {
        ElephantEvent {
            kind: kind.to_string(),
            space: task.space.id.clone(),
            task: Some(task.id.clone()),
            ts: self.now(),
            message,
            data,
        }
    }

    /// Guarda el estado, escribe la bitácora y reparte los eventos. Ningún
    /// candado queda tomado durante el disco.
    pub(crate) fn commit(&self, task: &Task, effects: Effects) {
        let (subscribers, elephant) = {
            let mut inner = self.inner.lock();
            inner.tasks.insert(task.id.clone(), task.clone());
            (
                inner.subscribers.clone(),
                inner
                    .elephants
                    .get(&task.space.id)
                    .map(|queue| queue.tx.clone()),
            )
        };
        self.save_state(task);
        for event in &effects.events {
            self.append_bitacora(event);
        }
        if let Some(sender) = elephant {
            for event in effects.elephant {
                let _ = sender.send(event);
            }
        }
        self.send_to_subscribers(&subscribers, &effects.events);
    }

    /// Solo emite eventos (señales de la ventana que no cambian el estado).
    pub(crate) fn emit_only(
        &self,
        space: &str,
        task: Option<&str>,
        events: Vec<ArbiterEvent>,
        elephant: Vec<ElephantEvent>,
    ) {
        let (subscribers, sender) = {
            let inner = self.inner.lock();
            (
                inner.subscribers.clone(),
                inner.elephants.get(space).map(|queue| queue.tx.clone()),
            )
        };
        for event in &events {
            if task.is_some() {
                self.append_bitacora(event);
            }
        }
        if let Some(sender) = sender {
            for event in elephant {
                let _ = sender.send(event);
            }
        }
        self.send_to_subscribers(&subscribers, &events);
    }

    fn send_to_subscribers(
        &self,
        subscribers: &[crossbeam_channel::Sender<ArbiterEvent>],
        events: &[ArbiterEvent],
    ) {
        for sender in subscribers {
            for event in events {
                let _ = sender.send(event.clone());
            }
        }
    }

    fn save_state(&self, task: &Task) {
        let path = crate::paths::task_dir(&self.data_dir, &task.id).join("estado.json");
        match serde_json::to_vec_pretty(task) {
            Ok(bytes) => {
                if let Err(error) = crate::paths::write_atomic(&path, &bytes) {
                    tracing::warn!(%error, path = %path.display(), "no se pudo guardar el estado");
                }
            }
            Err(error) => tracing::warn!(%error, "no se pudo serializar el estado"),
        }
    }

    pub(crate) fn append_bitacora(&self, event: &ArbiterEvent) {
        let Some(task) = &event.task else {
            return;
        };
        let path = crate::paths::task_dir(&self.data_dir, task).join("bitacora.jsonl");
        match serde_json::to_string(&event.redacted()) {
            Ok(line) => {
                if let Err(error) = crate::paths::append_line(&path, &line) {
                    tracing::warn!(%error, path = %path.display(), "no se pudo escribir la bitácora");
                }
            }
            Err(error) => tracing::warn!(%error, "no se pudo serializar el evento"),
        }
    }

    /// Deja constancia de un rechazo (R12) sin cambiar el estado.
    pub(crate) fn log_rejection(
        &self,
        caller: &crate::types::Caller,
        rejection: &crate::types::Rejection,
    ) {
        let task = self.task_for_caller(caller);
        let space = match (&task, caller) {
            (Some(task), _) => task.space.id.clone(),
            (None, crate::types::Caller::Elephant { space }) => space.clone(),
            (None, crate::types::Caller::Fish { .. }) => return,
        };
        let event = ArbiterEvent {
            kind: "rejected".to_string(),
            space: space.clone(),
            task: task.as_ref().map(|task| task.id.clone()),
            ts: self.now(),
            data: serde_json::json!({ "rule": rejection.rule, "message": rejection.message }),
        };
        let (subscribers, sender) = {
            let inner = self.inner.lock();
            (
                inner.subscribers.clone(),
                inner.elephants.get(&space).map(|queue| queue.tx.clone()),
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
            let _ = sender.send(message);
        }
        self.send_to_subscribers(&subscribers, std::slice::from_ref(&event));
    }

    /// Ruta de un archivo de datos de la tarea.
    pub(crate) fn task_path(&self, task: &Task, name: &str) -> std::path::PathBuf {
        crate::paths::task_dir(&self.data_dir, &task.id).join(name)
    }
}
