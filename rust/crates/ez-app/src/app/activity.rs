//! Port of `src/ezagent/voice/app.py 3488-3986` (F6-07).
//!
//! Tareas y cadenas de agentes, actividad de la barra, vigilancia y olvido de
//! terminales. Las salidas de los agentes son datos no confiables: se recortan
//! y se entregan, nunca se interpretan como instrucciones.

mod log;
mod tasks;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use log::activity_rows;

use std::sync::atomic::Ordering;

use ez_core::pytime::timestamp;

use super::core::{log as core_log, VoiceApp};

impl VoiceApp {
    /// `_activity_tick`: solo lee estados ya calculados. El bucle de la
    /// interfaz reprograma el siguiente latido (`startup::schedule_activity_tick`).
    pub fn activity_tick(&self) {
        if self.inner.session.shutdown_done.load(Ordering::SeqCst) {
            return;
        }
        log::refresh_activity(self);
    }

    /// `_refresh_activity`: la foto de la actividad, ya ordenada.
    pub fn refresh_activity(&self) {
        log::refresh_activity(self);
    }

    /// `_news_arrived`: desenlace que pide atención.
    pub fn news_arrived(&self, key: &str, agent: &str, title: &str, reason: &str) {
        log::news_arrived(self, key, agent, title, reason);
    }

    /// `_news_seen`: todo visto tras abrir la vista rápida.
    pub fn news_seen(&self) {
        log::news_seen(self);
    }

    /// `_open_activity`: pulsar una fila trae su terminal.
    pub fn open_activity(&self, key: &str) {
        log::open_activity(self, key);
    }

    /// `_dismiss_activity`: la fila sale de la vista y de los pendientes.
    pub fn dismiss_activity(&self, key: &str) {
        log::dismiss_activity(self, key);
    }

    /// `_watch`: vigila un agente y recuerda de quién es la tarea.
    pub fn watch(&self, address: &str) {
        self.inner.agents.watched.lock().insert(address.to_string());
        log::activity_log(self).forget(address); // un desenlace viejo no tapa la tarea nueva
        if self.inner.agents.tasks.get(address).is_none() {
            let user_text = self.last_user_text();
            self.remember_task(address, "", &user_text);
        }
        self.check_task_now(address);
    }

    /// `_last_user_text` del mensajero, para la tarea recordada.
    pub fn last_user_text(&self) -> String {
        self.inner
            .agents
            .messenger
            .lock()
            .as_ref()
            .map(|messenger| messenger.export_context().2)
            .unwrap_or_default()
    }

    /// `_mute(minutes)`: silencia los avisos al menos un minuto.
    pub fn mute(&self, minutes: i64) {
        *self.inner.session.mute_until.lock() = timestamp() + minutes.max(1) as f64 * 60.0;
        core_log(format!("Avisos silenciados {minutes} min."));
    }
}
