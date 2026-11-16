//! Reloj del bucle.
//!
//! Producción usa [`RealClock`]. Las pruebas usan [`TestClock`]: avanza el
//! tiempo lógico con cada espera y duerme unos milisegundos reales para que la
//! prueba no tarde minutos. No hay async ni temporizadores globales.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

/// Lo que el bucle necesita del tiempo, inyectable.
pub trait Clock: Send + Sync {
    fn now(&self) -> f64;
    fn sleep(&self, secs: f64);
}

/// Tiempo real (`time.time()` y `time.sleep`).
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64())
    }

    fn sleep(&self, secs: f64) {
        std::thread::sleep(Duration::from_secs_f64(secs.max(0.0)));
    }
}

/// Reloj de pruebas: avanza `secs` de tiempo lógico por cada espera.
pub struct TestClock {
    now: Mutex<f64>,
    real_ms: u64,
    after_sleep: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl TestClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(0.0),
            real_ms: 1,
            after_sleep: Mutex::new(Vec::new()),
        })
    }

    /// Mueve el reloj sin esperar (pruebas del reposo de 3 min).
    pub fn advance(&self, secs: f64) {
        *self.now.lock() += secs;
    }

    /// Ejecuta `hook` después de la próxima espera (pruebas de `stop`).
    pub fn after_next_sleep(&self, hook: impl FnOnce() + Send + 'static) {
        self.after_sleep.lock().push(Box::new(hook));
    }
}

impl Clock for TestClock {
    fn now(&self) -> f64 {
        *self.now.lock()
    }

    fn sleep(&self, secs: f64) {
        *self.now.lock() += secs.max(0.0);
        std::thread::sleep(Duration::from_millis(self.real_ms));
        let hook = {
            let mut hooks = self.after_sleep.lock();
            if hooks.is_empty() {
                None
            } else {
                Some(hooks.remove(0))
            }
        };
        if let Some(hook) = hook {
            hook();
        }
    }
}
