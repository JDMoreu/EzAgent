//! `hc` y el reloj que usa el vigilante.
//!
//! `WatchRuntime` es la costura de las pruebas; [`HcRuntime`] es la de
//! producción y habla con el `hc` del adaptador de Herdr.

use std::sync::Arc;
use std::time::Duration;

use crate::herdr::HerdrRuntime;

use super::epoch_ms;

/// La salida de `hc`.
#[derive(Debug, Clone, Default)]
pub struct HcOutput {
    /// El código de salida del proceso.
    pub code: i64,
    /// La salida estándar.
    pub stdout: String,
    /// La salida de error.
    pub stderr: String,
}

/// Lo que el vigilante necesita del exterior: `hc` y el reloj.
pub trait WatchRuntime: Send + Sync {
    /// Ejecuta `hc` con estos argumentos (sin shell).
    fn hc(&self, args: &[String]) -> Result<HcOutput, String>;

    /// Ahora, en milisegundos desde epoch.
    fn now_ms(&self) -> u64;

    /// Duerme estos milisegundos.
    fn sleep_ms(&self, ms: u64);
}

/// El runtime real: `hc` a través del runtime de Herdr.
pub struct HcRuntime {
    runtime: Arc<dyn HerdrRuntime>,
    hc: String,
    timeout: f64,
}

impl HcRuntime {
    /// Un runtime que habla con el `hc` de `path`.
    pub fn new(runtime: Arc<dyn HerdrRuntime>, path: impl Into<String>) -> Self {
        Self {
            runtime,
            hc: path.into(),
            timeout: 30.0,
        }
    }

    /// La ruta de `hc` que usa este runtime.
    pub fn hc_path(&self) -> &str {
        &self.hc
    }

    fn argv(&self, args: &[String]) -> Vec<String> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(self.hc.clone());
        argv.extend_from_slice(args);
        argv
    }
}

impl WatchRuntime for HcRuntime {
    fn hc(&self, args: &[String]) -> Result<HcOutput, String> {
        let argv = self.argv(args);
        match self.runtime.run(&argv, self.timeout, false) {
            Ok(done) => Ok(HcOutput {
                code: done.returncode,
                stdout: done.stdout,
                stderr: done.stderr,
            }),
            Err(error) => Err(error.to_string()),
        }
    }

    fn now_ms(&self) -> u64 {
        epoch_ms()
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}
