//! Costura de observación de la app para el arnés de reproducción (F7-01a).
//!
//! En Python el arnés sustituye el HUD, envuelve `ChatModel`, el `VoiceConfig`
//! y el `AgentRuntime`, y parchea `transcribe`. En Rust el HUD es un tipo
//! concreto de `ez-hud`, así que la app atraviesa estas llamadas por un trait
//! con implementación vacía: producción no cambia y el arnés recibe cada
//! transición con el mismo nombre y el mismo orden que la traza de Python.
//!
//! El `TraceSlot` compartido permite instalar el registrador después de crear
//! las piezas (el HUD y el chat se construyen antes de `Inner`).

use std::sync::Arc;

use parking_lot::RwLock;

/// Eventos que el arnés necesita de la app (espejo de los parches de Python).
pub trait AppTrace: Send + Sync {
    /// `hud.show(state)`.
    fn hud_show(&self, _state: &str) {}
    /// `hud.hide()`.
    fn hud_hide(&self) {}
    /// `hud.set_in_call(value)`.
    fn hud_in_call(&self, _value: bool) {}
    /// `hud.set_muted(value)`.
    fn hud_muted(&self, _value: bool) {}
    /// `hud.notice()`.
    fn hud_notice(&self) {}
    /// `hud.show_confirmation(summary, strong)`.
    fn hud_confirm(&self, _summary: &str, _strong: bool) {}
    /// `hud.hide_confirmation()`.
    fn hud_confirm_done(&self) {}
    /// `hud.set_testing(value)`.
    fn hud_testing(&self, _value: bool) {}
    /// `hud.close()`.
    fn hud_close(&self) {}
    /// `ChatModel.add_user`.
    fn chat_user(&self, _text: &str, _source: &str) {}
    /// `ChatModel.finish`.
    fn chat_assistant(&self, _text: &str) {}
    /// `ChatModel.cancel`.
    fn chat_cancel(&self, _text: &str) {}
    /// `ChatModel.fail`.
    fn chat_fail(&self, _text: &str) {}
    /// `ChatModel.add_note`.
    fn chat_note(&self, _text: &str) {}
    /// `ChatModel.ask_confirm`.
    fn chat_confirm(&self, _summary: &str, _strong: bool) {}
    /// `ChatModel.clear_confirm`.
    fn chat_confirm_done(&self) {}
    /// Escritura real de `chat.jsonl` (`ChatStore._write`).
    fn chat_file_write(&self, _role: &str) {}
    /// Respuesta de `transcribe` (antes de descartar alucinaciones).
    fn stt_transcript(&self, _text: &str) {}
}

/// Observador vacío de producción.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullTrace;

impl AppTrace for NullTrace {}

/// Hueco compartido donde el arnés instala su registrador.
pub type TraceSlot = Arc<RwLock<Arc<dyn AppTrace>>>;

/// Crea el hueco con el observador indicado (o el vacío).
pub fn trace_slot(trace: Option<Arc<dyn AppTrace>>) -> TraceSlot {
    Arc::new(RwLock::new(
        trace.unwrap_or_else(|| Arc::new(NullTrace) as Arc<dyn AppTrace>),
    ))
}

/// Llama al observador vigente sin retener el cerrojo.
pub fn notify(slot: &TraceSlot, action: impl FnOnce(&dyn AppTrace)) {
    let trace = slot.read().clone();
    action(&*trace);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counter(AtomicUsize);

    impl AppTrace for Counter {
        fn hud_show(&self, _state: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn null_trace_ignores_every_event() {
        let slot = trace_slot(None);
        notify(&slot, |trace| trace.hud_show("idle"));
        notify(&slot, |trace| trace.stt_transcript("hola"));
    }

    #[test]
    fn slot_can_be_replaced_after_construction() {
        let slot = trace_slot(None);
        notify(&slot, |trace| trace.hud_show("idle"));
        let counter = Arc::new(Counter::default());
        *slot.write() = counter.clone();
        notify(&slot, |trace| trace.hud_show("listening"));
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    }
}
