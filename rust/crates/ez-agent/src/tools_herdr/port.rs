//! Costura de Herdr para las herramientas: el oráculo de Python sustituye el
//! adaptador por un doble; aquí el contrato es este trait y la producción lo
//! implementa sobre el `Herdr` real.

use std::sync::Arc;

use ez_herdr::herdr::{Herdr, HerdrError, RunAgent};
use serde_json::{Map, Value};

use super::delegate::driver;

/// Fallo del adaptador: los errores propios de Herdr o uno inesperado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    Herdr(HerdrError),
    Unexpected(String),
}

impl std::fmt::Display for PortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Herdr(error) => std::fmt::Display::fmt(error, formatter),
            Self::Unexpected(message) => formatter.write_str(message),
        }
    }
}

/// Lo que las herramientas necesitan de Herdr.
pub trait HerdrPort: Send + Sync {
    fn resolve_agent(&self, name: &str) -> Option<String>;
    fn label_for(&self, address: &str) -> String;
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError>;
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError>;
    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, PortError>;
    fn read_full(&self, address: &str) -> Result<String, PortError>;
    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, PortError>;
    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError>;
    fn open_terminal(
        &self,
        name: &str,
        cwd: Option<&str>,
        shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError>;
    fn agents_catalog(&self) -> Vec<Map<String, Value>>;
    fn configure_agent(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Map<String, Value>;
    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, PortError>;
    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError>;
    fn kill_agent(&self, address: &str, confirmed: bool) -> Result<Map<String, Value>, PortError>;
    fn save_context(&self, text: &str) -> Option<String>;
}

impl HerdrPort for Arc<Herdr> {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        Herdr::resolve_agent(self, name)
    }

    fn label_for(&self, address: &str) -> String {
        Herdr::label_for(self, address)
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        Herdr::list_agents(self, 20.0).map_err(PortError::Herdr)
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        Herdr::get_agent(self, address).map_err(PortError::Herdr)
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, PortError> {
        Herdr::read_agent(self, address, lines, 6000).map_err(PortError::Herdr)
    }

    fn read_full(&self, address: &str) -> Result<String, PortError> {
        Herdr::read_full(self, address, 1500).map_err(PortError::Herdr)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        Herdr::send_agent(self, address, text, confirmed).map_err(PortError::Herdr)
    }

    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        Herdr::run_agent(self, driver(self), params).map_err(PortError::Herdr)
    }

    fn open_terminal(
        &self,
        name: &str,
        cwd: Option<&str>,
        shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        Herdr::open_terminal(self, name, cwd, shell, space).map_err(PortError::Herdr)
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        driver(self).agents_catalog()
    }

    fn configure_agent(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Map<String, Value> {
        driver(self).configure_agent(address, model, effort)
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, PortError> {
        Herdr::keys_agent(self, address, keys).map_err(PortError::Herdr)
    }

    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        Herdr::focus_agent(self, address).map_err(PortError::Herdr)
    }

    fn kill_agent(&self, address: &str, confirmed: bool) -> Result<Map<String, Value>, PortError> {
        Herdr::kill_agent(self, address, confirmed).map_err(PortError::Herdr)
    }

    fn save_context(&self, text: &str) -> Option<String> {
        Herdr::save_context(self, text)
    }
}
