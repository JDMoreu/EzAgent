//! Lo que el supervisor, la recuperación y el stack necesitan de un agente
//! externo (Herdr en producción, un doble en las pruebas).
//!
//! El Python recibe `herdr: Any` y llama a un puñado de métodos; aquí es un
//! trait para poder sustituirlo sin abrir terminales.

use std::sync::Arc;

use ez_core::pyvalue::text_or_empty;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{Herdr, RunAgent};
use serde_json::{Map, Value};

use super::TICK_SECS;

/// Argumentos de `run_agent` que usa el lanzamiento del enjambre.
#[derive(Debug, Clone, Default)]
pub struct LaunchRequest {
    /// Carpeta de trabajo (`None` = la de Herdr).
    pub cwd: Option<String>,
    /// Nombre visible de la terminal.
    pub name: String,
    /// Variables de entorno de la shell.
    pub env: Map<String, Value>,
    /// Carpetas que se anteponen al `PATH`.
    pub path_prepend: Option<Vec<String>>,
    /// Argumentos extra del perfil (por ejemplo `--yolo`).
    pub extra_args: Option<Vec<String>>,
    /// Modelo elegido.
    pub model: Option<String>,
    /// Esfuerzo elegido.
    pub effort: Option<String>,
}

/// Llamadas del supervisor y de la recuperación al adaptador de agentes.
pub trait SwarmHost: Send + Sync {
    /// Terminales con su estado; usa la caché corta cuando está fresca.
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String>;

    /// Detalle de una terminal (`hc get`).
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, String>;

    /// Pantalla de una terminal, recortada a 6000 caracteres como Python.
    fn read_agent(&self, address: &str, lines: i64) -> Result<String, String>;

    /// Escribe una línea en la terminal y confirma que llegó.
    fn send_task(&self, address: &str, line: &str) -> Result<Map<String, Value>, String>;

    /// Abre un agente y le pega la tarea (aquí, vacía).
    fn run_agent(
        &self,
        agent: &str,
        task: &str,
        request: LaunchRequest,
    ) -> Result<Map<String, Value>, String>;

    /// Clave canónica de un agente a partir de cómo se dictó.
    fn resolve_agent(&self, name: &str) -> Option<String>;

    /// Argumentos extra del perfil cuando el agente trabaja en un enjambre.
    fn swarm_args(&self, agent: &str) -> Vec<String>;

    /// Guarda un texto largo y devuelve su ruta.
    fn save_context(&self, text: &str) -> Option<String>;

    /// Carpeta de trabajo por defecto.
    fn default_cwd(&self) -> String;

    /// Agentes del catálogo.
    fn agent_names(&self) -> Vec<String>;
}

/// Adaptador real: `Herdr` para las terminales y `AgentDriver` para lanzar.
pub struct HerdrHost {
    herdr: Arc<Herdr>,
    driver: AgentDriver<Arc<Herdr>>,
}

/// Envoltorio del arnés: sin listado, el supervisor pregunta terminal por
/// terminal (`probe`). El arnés avanza a mano y no puede depender de la caché
/// de 3 s: con ella, una edición de `fake-hc` podía quedar tapada y el
/// supervisor no veía salir al agente (escenario 20 bajo carga).
pub(crate) struct ProbeHost(Arc<dyn SwarmHost>);

impl ProbeHost {
    /// Envuelve el adaptador real para forzar `get_agent` en cada pasada.
    pub(crate) fn new(inner: Arc<dyn SwarmHost>) -> Self {
        Self(inner)
    }
}

impl SwarmHost for ProbeHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(Vec::new())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, String> {
        self.0.get_agent(address)
    }

    fn read_agent(&self, address: &str, lines: i64) -> Result<String, String> {
        self.0.read_agent(address, lines)
    }

    fn send_task(&self, address: &str, line: &str) -> Result<Map<String, Value>, String> {
        self.0.send_task(address, line)
    }

    fn run_agent(
        &self,
        agent: &str,
        task: &str,
        request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        self.0.run_agent(agent, task, request)
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.0.resolve_agent(name)
    }

    fn swarm_args(&self, agent: &str) -> Vec<String> {
        self.0.swarm_args(agent)
    }

    fn save_context(&self, text: &str) -> Option<String> {
        self.0.save_context(text)
    }

    fn default_cwd(&self) -> String {
        self.0.default_cwd()
    }

    fn agent_names(&self) -> Vec<String> {
        self.0.agent_names()
    }
}

impl HerdrHost {
    /// Empareja el adaptador de Herdr con el driver que sabe lanzar agentes.
    pub fn new(herdr: Arc<Herdr>) -> Self {
        let driver = AgentDriver::new(Arc::clone(&herdr), herdr.config());
        Self { herdr, driver }
    }
}

impl SwarmHost for HerdrHost {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        if let Some(agents) = self.herdr.cached_agents(TICK_SECS) {
            return Ok(agents);
        }
        self.herdr
            .list_agents(10.0)
            .map_err(|error| error.message().to_string())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, String> {
        self.herdr
            .get_agent(address)
            .map_err(|error| error.message().to_string())
    }

    fn read_agent(&self, address: &str, lines: i64) -> Result<String, String> {
        self.herdr
            .read_agent(address, Some(lines), 6000)
            .map_err(|error| error.message().to_string())
    }

    fn send_task(&self, address: &str, line: &str) -> Result<Map<String, Value>, String> {
        Ok(self.driver.send_task(address, line))
    }

    fn run_agent(
        &self,
        agent: &str,
        task: &str,
        request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        let params = RunAgent {
            agent,
            task,
            cwd: request.cwd.as_deref(),
            name: Some(request.name.as_str()),
            confirmed: true,
            env: Some(request.env),
            path_prepend: request.path_prepend,
            extra_args: request.extra_args,
            model: request.model.as_deref(),
            effort: request.effort.as_deref(),
            ..RunAgent::default()
        };
        self.herdr
            .run_agent(self.driver.clone(), params)
            .map_err(|error| error.message().to_string())
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.driver.resolve_agent(name)
    }

    fn swarm_args(&self, agent: &str) -> Vec<String> {
        self.driver.swarm_args(agent)
    }

    fn save_context(&self, text: &str) -> Option<String> {
        self.herdr.save_context(text)
    }

    fn default_cwd(&self) -> String {
        text_or_empty(self.herdr.config().get("default_cwd"))
    }

    fn agent_names(&self) -> Vec<String> {
        match self.herdr.config().get("agents") {
            Some(Value::Object(agents)) => agents.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn the_host_trait_is_send_and_sync() {
        assert_send_sync::<Arc<dyn SwarmHost>>();
    }
}
