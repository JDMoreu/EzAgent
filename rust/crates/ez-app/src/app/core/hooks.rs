//! Costuras de ciclo de vida entre F6-01 y las tareas F6-02…F6-07 (F6-01).
//!
//! `VoiceApp.__init__` y `run` llaman a métodos que el brief asigna a otras
//! tareas (`_start_call`, `_tick`, `_fetch_llm_models`, bandeja…). Aquí son un
//! trait con implementaciones vacías: cada tarea posterior instala la suya con
//! [`super::VoiceApp::set_hooks`] sin tocar `core.rs` ni `startup.rs`.
//!
//! // DECISION: un trait con métodos por omisión evita adelantar código ajeno y
//! // deja el arranque probable con dobles (grabar qué se llamó y en qué orden).

use std::path::PathBuf;

use serde_json::Value;

/// Ganchos de la aplicación que aún no existen en F6-01.
pub trait AppHooks: Send + Sync {
    /// Un evento del vigilante: `(address, agent, state, reason)`.
    fn watcher_event(&self, _address: &str, _agent: &str, _state: &str, _reason: &str) {}
    /// `_should_announce(address)`.
    fn should_announce(&self, _address: &str) -> bool {
        false
    }
    /// Un paso de `_tick` (atajo, evento o menú pendiente).
    fn tick(&self) {}
    /// `_start_call` (F6-03).
    fn start_call(&self) {}
    /// `_resume_pending_tasks` (F6-05).
    fn resume_pending_tasks(&self) {}
    /// `_activity_tick` (F6-07), sin reprogramarse.
    fn activity_tick(&self) {}
    /// `_fetch_llm_models` (F6-06).
    fn fetch_llm_models(
        &self,
        _provider: &str,
        _base_url: &str,
        _api_key: &str,
        _api_style: &str,
        _force: bool,
    ) {
    }
    /// `_refresh_devices` (F6-06).
    fn refresh_devices(&self) {}
    /// `_create_tray` (F6-02).
    fn create_tray(&self) {}
    /// `_update_tray_title` (F6-02).
    fn update_tray_title(&self) {}
    /// `_chat_media_roots` (F6-05): `(roots, base_dirs)`.
    fn chat_media_roots(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        (vec![ez_core::dirs::home()], Vec::new())
    }
    /// Callbacks del runtime: `_watch`.
    fn watch(&self, _text: &str) {}
    /// Callbacks del runtime: `_request_close`.
    fn request_close(&self) {}
    /// Callbacks del runtime: `_llm_mic_mute`.
    fn mic_mute(&self, _muted: bool) {}
    /// Callbacks del runtime: `_mute`.
    fn mute(&self, _minutes: i64) {}
    /// Callbacks del runtime: `_last_user_text` del mensajero.
    fn last_user_text(&self) -> String {
        String::new()
    }
    /// Callbacks del runtime: `_remember_task`.
    fn remember_task(&self, _address: &str, _agent: &str, _task: &str) {}
    /// Callbacks del runtime: `_enqueue_chain`.
    fn enqueue_chain(&self, _spec: &Value) {}
    /// Callbacks del runtime: `_deliver_answer`.
    fn deliver_answer(&self, _answer: &str) {}
    /// Callbacks del runtime: `_start_prelaunch`.
    fn prewarm_chain(&self, _spec: &Value) {}
}

/// Ganchos vacíos: el arranque funciona y cada paso futuro es un no-op.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullHooks;

impl AppHooks for NullHooks {}

/// Recibe cada paso del apagado, en orden (pruebas del orden de cierre).
pub trait ShutdownObserver: Send + Sync {
    fn step(&self, _step: &str) {}
}

/// Observador vacío de producción.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullObserver;

impl ShutdownObserver for NullObserver {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        steps: Mutex<Vec<String>>,
    }

    impl ShutdownObserver for Recorder {
        fn step(&self, step: &str) {
            self.steps
                .lock()
                .expect("sin envenenar")
                .push(step.to_string());
        }
    }

    #[test]
    fn null_hooks_are_safe_no_ops() {
        let hooks = NullHooks;
        hooks.watcher_event("a", "claude", "done", "listo");
        assert!(!hooks.should_announce("a"));
        assert_eq!(hooks.last_user_text(), "");
        hooks.tick();
        hooks.start_call();
        assert_eq!(hooks.chat_media_roots().0.len(), 1);
    }

    #[test]
    fn observer_records_order() {
        let observer = Recorder::default();
        observer.step("uno");
        observer.step("dos");
        assert_eq!(
            *observer.steps.lock().expect("sin envenenar"),
            vec!["uno".to_string(), "dos".to_string()]
        );
    }
}
