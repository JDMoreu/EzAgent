//! Fachada de `ChatModel` que anota mensajes y escrituras (F7-01a).
//!
//! Espejo de `instrument_chat` de `parity/replay/run_py.py`: cada método que la
//! traza observa (alta de usuario, respuesta terminada, corte, fallo, nota y
//! confirmación) avisa al [`AppTrace`] con el mismo texto y en el mismo orden
//! (primero la escritura real de `chat.jsonl` y después el evento del chat).
//! El resto de la API llega por `Deref`.

use std::ops::Deref;
use std::sync::Arc;

use ez_chat::session::{ChatMessage, ChatModel};
use ez_chat::store::Source;
use ez_core::pyvalue::py_strip;

use super::trace::{notify, TraceSlot};

/// El chat tal como lo usa `VoiceApp`.
pub struct AppChat {
    inner: Arc<ChatModel>,
    trace: TraceSlot,
}

impl AppChat {
    /// Envuelve el chat ya construido.
    pub fn new(inner: Arc<ChatModel>, trace: TraceSlot) -> Arc<Self> {
        Arc::new(Self { inner, trace })
    }

    /// El modelo sin la fachada (contextos, panel y pruebas).
    pub fn inner(&self) -> &Arc<ChatModel> {
        &self.inner
    }

    /// `_write` del historial: solo cuando hay almacén en disco.
    fn file_write(&self, role: &str) {
        if self.inner.store().is_some() {
            notify(&self.trace, |trace| trace.chat_file_write(role));
        }
    }

    /// Texto vigente de un mensaje (`chat._find(message_id).text`).
    fn text_of(&self, message_id: i64) -> String {
        self.inner
            .snapshot()
            .1
            .into_iter()
            .find(|message| message.id == message_id)
            .map_or_else(String::new, |message| message.text)
    }

    /// `add_user`: mensaje del usuario y su escritura en `chat.jsonl`.
    pub fn add_user(&self, text: &str, source: Source) -> ChatMessage {
        let message = self.inner.add_user(text, source.clone());
        self.file_write("user");
        notify(&self.trace, |trace| {
            trace.chat_user(&message.text, source.as_str());
        });
        message
    }

    /// `finish`: respuesta completa (también si se cortó).
    pub fn finish(&self, message_id: i64) {
        let writes = !py_strip(&self.text_of(message_id)).is_empty();
        self.inner.finish(message_id);
        if writes {
            self.file_write("assistant");
        }
        let text = self.text_of(message_id);
        notify(&self.trace, |trace| trace.chat_assistant(&text));
    }

    /// `cancel`: se conserva lo que llegó.
    pub fn cancel(&self, message_id: i64) {
        let writes = !py_strip(&self.text_of(message_id)).is_empty();
        self.inner.cancel(message_id);
        if writes {
            self.file_write("assistant");
        }
        let text = self.text_of(message_id);
        notify(&self.trace, |trace| trace.chat_cancel(&text));
    }

    /// `fail`: el motivo del error no se guarda en el historial.
    pub fn fail(&self, message_id: i64, text: &str) {
        self.inner.fail(message_id, text);
        notify(&self.trace, |trace| trace.chat_fail(text));
    }

    /// `add_note`: nota del sistema (compactación, por ejemplo).
    pub fn add_note(&self, text: &str) -> ChatMessage {
        let message = self.inner.add_note(text);
        notify(&self.trace, |trace| trace.chat_note(text));
        message
    }

    /// `ask_confirm`.
    pub fn ask_confirm(&self, summary: &str, strong: bool) {
        self.inner.ask_confirm(summary, strong);
        notify(&self.trace, |trace| trace.chat_confirm(summary, strong));
    }

    /// `clear_confirm`.
    pub fn clear_confirm(&self) {
        self.inner.clear_confirm();
        notify(&self.trace, |trace| trace.chat_confirm_done());
    }
}

impl Deref for AppChat {
    type Target = ChatModel;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::core::trace::{trace_slot, AppTrace};
    use parking_lot::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    impl AppTrace for Recorder {
        fn chat_user(&self, text: &str, source: &str) {
            self.0.lock().push(format!("user:{text}:{source}"));
        }

        fn chat_assistant(&self, text: &str) {
            self.0.lock().push(format!("assistant:{text}"));
        }

        fn chat_cancel(&self, text: &str) {
            self.0.lock().push(format!("cancel:{text}"));
        }

        fn chat_file_write(&self, role: &str) {
            self.0.lock().push(format!("write:{role}"));
        }
    }

    #[test]
    fn records_writes_before_chat_events_and_skips_empty_history() {
        let recorder = Arc::new(Recorder::default());
        let slot = trace_slot(Some(recorder.clone()));
        let chat = AppChat::new(Arc::new(ChatModel::new(None, true)), slot);
        chat.add_user("hola", Source::Voice);
        let message = chat.begin_assistant(Source::Voice);
        chat.append(message.id, "listo");
        chat.finish(message.id);
        chat.cancel(99);
        assert_eq!(
            *recorder.0.lock(),
            vec![
                "user:hola:voice".to_string(),
                "assistant:listo".to_string(),
                "cancel:".to_string(),
            ]
        );
    }
}
