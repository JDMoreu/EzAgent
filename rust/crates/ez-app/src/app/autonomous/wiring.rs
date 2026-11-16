//! Cableado del bucle en la app: se crea al construir el núcleo, se instala
//! como host de las herramientas y se consulta desde el vigilante, la barra y
//! el atajo de cancelar (10.6 y 10.19).

use std::sync::Arc;

use ez_agent::autonomous::artifacts;
use ez_agent::autonomous::{AutonomousHost, Summary, TaskState};
use ez_agent::runtime::AgentRuntime;
use ez_agent::tools_herdr::port::HerdrPort;

use super::{AutonomousLoop, RealClock, StackSwarm, TeamSwarm};
use crate::app::core::VoiceApp;

/// Clave de la fila fija de la vista rápida (10.19).
pub const AUTONOMOUS_KEY: &str = "autonomous";
/// Pregunta de la parada desde la fila, generada por el código (10.19).
pub const STOP_QUESTION: &str = "¿Detengo el modo autónomo?";
/// Motivo que viaja al bucle cuando se para desde la fila.
const ROW_STOP_REASON: &str = "desde la vista rápida";

/// La fila de la vista rápida mientras la tarea existe (9.8 y 10.19).
///
/// Devuelve el título, el detalle de la segunda línea y si lleva marca en la
/// órbita. Juntos, título y detalle forman `Summary::describe`.
pub fn peek_row(summary: &Summary) -> Option<(String, String, bool)> {
    if !summary.state.unfinished() {
        return None;
    }
    let title = format!("Autónomo · ronda {}", summary.round);
    let detail = if summary.state == TaskState::Paused {
        match &summary.pause_reason {
            Some(reason) => format!("en pausa: {reason}"),
            None => String::from("en pausa"),
        }
    } else {
        ez_agent::autonomous::texts::state_label(summary.state).to_string()
    };
    // 9.8: la marca de la órbita solo con la tarea trabajando, no en pausa.
    let active = matches!(summary.state, TaskState::Running | TaskState::Verifying);
    Some((title, detail, active))
}

impl VoiceApp {
    /// El enjambre del modo con equipo, si la app lo montó (10.18).
    fn team_swarm(&self) -> Option<Arc<dyn TeamSwarm>> {
        let stack = self.inner.agents.swarm.lock().clone();
        stack.map(|stack| Arc::new(StackSwarm::new(stack)) as Arc<dyn TeamSwarm>)
    }

    /// Crea el bucle y, en producción, lo instala antes del runtime.
    pub(crate) fn prepare_autonomous(&self) {
        // El arnés de reproducción envuelve el runtime: allí no se instala el
        // bucle, para no cambiar los esquemas de herramientas ni tocar datos
        // reales del usuario.
        if self.inner.runtime.runtime_wrap.is_some() {
            return;
        }
        let loop_ = AutonomousLoop::new(
            Arc::new(Arc::clone(&self.inner.agents.herdr)) as Arc<dyn HerdrPort>,
            Arc::clone(&self.inner.ui.events),
            Arc::new(RealClock),
            artifacts::root(),
            self.team_swarm(),
        );
        *self.inner.agents.autonomous.lock() = Some(Arc::clone(&loop_));
        if self.inner.runtime.background {
            ez_agent::autonomous::tools::install_host(loop_ as Arc<dyn AutonomousHost>);
        }
    }

    /// Auditoría del runtime y, al arrancar, el reinicio en pausa (9.3).
    pub(crate) fn attach_autonomous(&self, runtime: &AgentRuntime) {
        let Some(loop_) = self.autonomous_loop() else {
            return;
        };
        loop_.set_audit(runtime.audit());
        if self.inner.runtime.background {
            loop_.restore_after_restart();
        }
    }

    /// El bucle del proceso, si el núcleo agéntico está disponible.
    pub fn autonomous_loop(&self) -> Option<Arc<AutonomousLoop>> {
        self.inner.agents.autonomous.lock().clone()
    }

    /// 10.6: mientras la tarea existe, sus direcciones no se anuncian solas.
    pub fn autonomous_covers(&self, address: &str) -> bool {
        self.autonomous_loop()
            .is_some_and(|loop_| loop_.covers(address))
    }

    /// ¿Hay una tarea en ejecución o verificación? (atajo de cancelar, 10.19)
    pub fn autonomous_running(&self) -> bool {
        self.autonomous_loop()
            .is_some_and(|loop_| loop_.running_or_verifying())
    }

    /// «Para» desde la app.
    pub fn stop_autonomous(&self, reason: &str, accept: bool) -> Result<(), String> {
        match self.autonomous_loop() {
            Some(loop_) => loop_.stop_task(reason, accept),
            None => Err(ez_agent::autonomous::texts::NO_TASK.to_string()),
        }
    }

    /// El atajo de cancelar detiene la tarea, después de cancelar la
    /// confirmación en curso (decisión 7 de A3).
    pub fn cancel_hotkey(&self) {
        self.hang_up();
        if self.autonomous_running() {
            let _ = self.stop_autonomous("atajo de cancelar", false);
        }
    }

    /// 10.19: pulsar la fila pregunta y, solo con «sí», detiene la tarea.
    ///
    /// `ask` puede tardar lo que tarde el usuario: se llama sin ningún cerrojo
    /// tomado (regla de hilos de 9.1), desde el hilo `ezagent-autonomo-stop`.
    pub fn stop_autonomous_from_row(&self, ask: impl FnOnce(&str) -> bool) -> bool {
        let Some(loop_) = self.autonomous_loop() else {
            return false;
        };
        if loop_.status().is_none() || !ask(STOP_QUESTION) {
            return false;
        }
        match self.stop_autonomous(ROW_STOP_REASON, false) {
            Ok(()) => true,
            Err(error) => {
                crate::app::core::log(format!("No pude detener el modo autónomo: {error}"));
                false
            }
        }
    }
}
