//! Hooks de paridad del `Messenger` (estado que el Python expone como atributos).

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{lock, ActiveStream, Messenger, TurnToken};

impl Messenger {
    /// Instala el turno y el flujo que `cancel_turn` debe cortar.
    #[doc(hidden)]
    pub fn install_cancel_target(
        &self,
        token: Arc<TurnToken>,
        stream: Box<dyn crate::openai::ChatStream>,
    ) {
        *lock(&self.shared.turn_token) = Some(token.clone());
        *lock(&self.shared.active_stream) = Some(Arc::new(ActiveStream {
            token,
            stream: Mutex::new(stream),
        }));
    }

    /// Número de generación de turnos (sube al cancelar).
    #[doc(hidden)]
    pub fn turn_generation(&self) -> u64 {
        self.shared.turn_generation.load(Ordering::SeqCst)
    }

    /// Marca el turno como activo (lo hace `ask_stream`).
    #[doc(hidden)]
    pub fn set_turn_active(&self, active: bool) {
        self.history().turn_active = active;
    }

    /// Último agente lanzado, para `pending_notify_address`.
    #[doc(hidden)]
    pub fn set_last_agent_address(&self, address: Option<&str>) {
        self.history().last_agent_address = address.map(str::to_string);
    }

    /// Contexto aparcado por un cambio durante el turno.
    #[doc(hidden)]
    pub fn deferred_history(&self) -> Option<(Vec<Value>, String, String)> {
        self.history().deferred.clone()
    }

    /// Notas de agentes que esperan al siguiente turno.
    #[doc(hidden)]
    pub fn pending_notes(&self) -> Vec<Value> {
        self.history().pending_notes.clone()
    }

    /// Deja notas pendientes sin pasar por `note_done`.
    #[doc(hidden)]
    pub fn set_pending_notes(&self, notes: Vec<Value>) {
        self.history().pending_notes = notes;
    }
}
