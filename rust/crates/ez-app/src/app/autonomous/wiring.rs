//! Cableado del bucle en la app: se crea al construir el núcleo, se instala
//! como host de las herramientas y se consulta desde el vigilante, la barra y
//! el atajo de cancelar (10.6 y 10.19).

use std::sync::Arc;

use ez_agent::autonomous::artifacts;
use ez_agent::autonomous::AutonomousHost;
use ez_agent::runtime::AgentRuntime;
use ez_agent::tools_herdr::port::HerdrPort;

use super::{AutonomousLoop, RealClock};
use crate::app::core::VoiceApp;

impl VoiceApp {
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
}
