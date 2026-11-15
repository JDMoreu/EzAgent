//! Lo que el adaptador de Herdr pide al sistema: procesos, PATH, reloj, ids y
//! hilos.
//!
//! Las pruebas de paridad inyectan un doble determinista (el mismo que usó el
//! oráculo de Python para `subprocess.run`, `shutil.which`, el reloj y
//! `uuid.uuid4`); en producción se usa [`SystemRuntime`].

use std::path::PathBuf;
use std::sync::Arc;

use super::process::{self, Completed, RunError};

/// Tarea que el refresco de contexto ejecuta en su hilo (`ezagent-herdr-context`).
pub type SpawnTask = Box<dyn FnOnce() + Send + 'static>;

pub trait HerdrRuntime: Send + Sync {
    /// `subprocess.run(..., capture_output=True, text=True, timeout=…)`.
    fn run(&self, argv: &[String], timeout: f64, anonymous: bool) -> Result<Completed, RunError>;

    /// `shutil.which`.
    fn which(&self, program: &str) -> Option<PathBuf>;

    /// `time.monotonic()`.
    fn monotonic(&self) -> f64;

    /// `uuid.uuid4().hex[:8]`.
    fn next_id(&self) -> String;

    /// Arranca un hilo con nombre; `false` si no se pudo.
    fn spawn(&self, name: &str, task: SpawnTask) -> bool;

    /// Doble de `list_agents`: el oráculo de Python lo monkeypatchea para leer
    /// la metadata de terminales sin ejecutar `hc`. En producción, `None`.
    fn agents(&self) -> Option<Vec<serde_json::Map<String, serde_json::Value>>> {
        None
    }
}

/// Implementación real: procesos del sistema, reloj monotónico e hilos.
pub struct SystemRuntime;

impl HerdrRuntime for SystemRuntime {
    fn run(&self, argv: &[String], timeout: f64, anonymous: bool) -> Result<Completed, RunError> {
        process::run(argv, timeout, anonymous)
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        process::which(program)
    }

    fn monotonic(&self) -> f64 {
        ez_core::metrics::monotonic()
    }

    fn next_id(&self) -> String {
        super::handoff::hex_id()
    }

    fn spawn(&self, name: &str, task: SpawnTask) -> bool {
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(task)
            .is_ok()
    }
}

/// Runtime por defecto, compartido por [`super::Herdr`] y
/// [`crate::watcher::Watcher`].
pub fn system() -> Arc<dyn HerdrRuntime> {
    Arc::new(SystemRuntime)
}
