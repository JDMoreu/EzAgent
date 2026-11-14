//! Port of `src/ezagent/voice/chat/session.py` (F3-14).
//!
//! Conversación en pantalla: mensajes escritos y hablados, streaming y
//! confirmaciones. Seguro entre hilos.

use std::sync::{Arc, Mutex, MutexGuard};

use ez_core::pytime::timestamp;
use ez_core::pyvalue::py_strip;

use crate::store::{ChatEntry, ChatStore, Role, SessionInfo, Source};

/// Un mensaje visible. El del asistente crece mientras llega en streaming.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub id: i64,
    pub role: Role,
    pub text: String,
    pub source: Source,
    pub t: f64,
    pub streaming: bool,
    pub error: bool,
}

/// Una confirmación pendiente («¿Seguro?»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmCard {
    pub summary: String,
    pub strong: bool,
}

struct Inner {
    voice_replies: bool,
    mic_muted: bool,
    messages: Vec<ChatMessage>,
    next_id: i64,
    version: i64,
    busy: bool,
    confirm: Option<ConfirmCard>,
    viewing: Option<i64>,
}

/// Estado del chat. `version` sube con cada cambio para que la vista repinte.
pub struct ChatModel {
    store: Option<Arc<ChatStore>>,
    inner: Mutex<Inner>,
}

impl ChatModel {
    /// `ChatModel(store, voice_replies)`.
    pub fn new(store: Option<Arc<ChatStore>>, voice_replies: bool) -> Self {
        let mut inner = Inner {
            voice_replies,
            mic_muted: false,
            messages: Vec::new(),
            next_id: 1,
            version: 0,
            busy: false,
            confirm: None,
            viewing: None,
        };
        if let Some(store) = &store {
            for entry in store.entries(None) {
                push(
                    &mut inner,
                    entry.role,
                    &entry.text,
                    entry.source,
                    Some(entry.t),
                );
            }
        }
        Self {
            store,
            inner: Mutex::new(inner),
        }
    }

    /// Historial asociado, si lo hay.
    pub fn store(&self) -> Option<&Arc<ChatStore>> {
        self.store.as_ref()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ----------------------------------------------------------- escritura
    /// El usuario dijo o escribió algo (se guarda en el historial).
    pub fn add_user(&self, text: &str, source: Source) -> ChatMessage {
        let mut inner = self.lock();
        inner.viewing = None;
        let message = push(&mut inner, Role::User, text, source, None);
        if let Some(store) = &self.store {
            store.append(Role::User, text, source, None);
        }
        bump(&mut inner);
        message
    }

    /// Sustituye el texto de un mensaje del usuario (la frase continuó).
    pub fn revise_user(&self, message_id: i64, text: &str) -> bool {
        let mut inner = self.lock();
        let Some(index) = find(&inner.messages, message_id) else {
            return false;
        };
        if inner.messages[index].role != Role::User {
            return false;
        }
        let previous = inner.messages[index].text.clone();
        if let Some(store) = &self.store {
            if !store.replace_last(Role::User, &previous, text) {
                return false;
            }
        }
        inner.messages[index].text = text.to_string();
        bump(&mut inner);
        true
    }

    /// Una nota de la propia app: se ve, pero no se guarda.
    pub fn add_note(&self, text: &str) -> ChatMessage {
        let mut inner = self.lock();
        let message = push(&mut inner, Role::System, text, Source::Agent, None);
        bump(&mut inner);
        message
    }

    /// Empieza una respuesta vacía que irá llenándose.
    pub fn begin_assistant(&self, source: Source) -> ChatMessage {
        let mut inner = self.lock();
        let mut message = push(&mut inner, Role::Assistant, "", source, None);
        message.streaming = true;
        if let Some(last) = inner.messages.last_mut() {
            last.streaming = true;
        }
        inner.busy = true;
        bump(&mut inner);
        message
    }

    /// Añade un trozo a una respuesta en curso.
    pub fn append(&self, message_id: i64, delta: &str) {
        let mut inner = self.lock();
        let Some(index) = find(&inner.messages, message_id) else {
            return;
        };
        if !delta.is_empty() {
            inner.messages[index].text.push_str(delta);
            bump(&mut inner);
        }
    }

    /// La respuesta terminó: se guarda en el historial.
    pub fn finish(&self, message_id: i64) {
        let mut inner = self.lock();
        let Some(index) = find(&inner.messages, message_id) else {
            return;
        };
        let text = py_strip(&inner.messages[index].text).to_string();
        let source = inner.messages[index].source;
        inner.messages[index].streaming = false;
        inner.messages[index].text = text.clone();
        let now = timestamp();
        inner.messages[index].t = now;
        if text.is_empty() {
            inner.messages.remove(index);
        } else if let Some(store) = &self.store {
            store.append(Role::Assistant, &text, source, Some(now));
        }
        inner.busy = inner.messages.iter().any(|message| message.streaming);
        bump(&mut inner);
    }

    /// La respuesta falló: se muestra el motivo (sin guardarlo).
    pub fn fail(&self, message_id: i64, text: &str) {
        let mut inner = self.lock();
        let Some(index) = find(&inner.messages, message_id) else {
            return;
        };
        inner.messages[index].streaming = false;
        inner.messages[index].error = true;
        inner.messages[index].text = text.to_string();
        inner.busy = inner.messages.iter().any(|message| message.streaming);
        bump(&mut inner);
    }

    /// Se interrumpió la respuesta: se conserva lo que llegó.
    pub fn cancel(&self, message_id: i64) {
        let mut inner = self.lock();
        let Some(index) = find(&inner.messages, message_id) else {
            return;
        };
        inner.messages[index].streaming = false;
        let text = py_strip(&inner.messages[index].text).to_string();
        if text.is_empty() {
            inner.messages.remove(index);
        } else {
            inner.messages[index].text = text.clone();
            if let Some(store) = &self.store {
                store.append(Role::Assistant, &text, inner.messages[index].source, None);
            }
        }
        inner.busy = inner.messages.iter().any(|message| message.streaming);
        bump(&mut inner);
    }

    // ------------------------------------------------------ confirmaciones
    pub fn ask_confirm(&self, summary: &str, strong: bool) {
        let mut inner = self.lock();
        inner.confirm = Some(ConfirmCard {
            summary: summary.to_string(),
            strong,
        });
        bump(&mut inner);
    }

    pub fn clear_confirm(&self) {
        let mut inner = self.lock();
        if inner.confirm.is_some() {
            inner.confirm = None;
            bump(&mut inner);
        }
    }

    // ------------------------------------------------------ conversaciones
    /// Empieza una conversación nueva (la anterior queda en el historial).
    pub fn new_session(&self) {
        let mut inner = self.lock();
        inner.messages.clear();
        inner.busy = false;
        inner.confirm = None;
        inner.viewing = None;
        if let Some(store) = &self.store {
            store.new_session();
        }
        bump(&mut inner);
    }

    /// Retoma una conversación anterior como la actual.
    pub fn switch_session(&self, session: i64) -> bool {
        let mut inner = self.lock();
        let Some(store) = &self.store else {
            return false;
        };
        if inner.busy || !store.resume(session) {
            return false;
        }
        inner.messages.clear();
        for entry in store.entries(Some(session)) {
            push(
                &mut inner,
                entry.role,
                &entry.text,
                entry.source,
                Some(entry.t),
            );
        }
        inner.confirm = None;
        inner.viewing = None;
        bump(&mut inner);
        true
    }

    /// Borra una conversación del historial; si era la actual, la pantalla queda vacía.
    pub fn delete_session(&self, session: i64) -> bool {
        let mut inner = self.lock();
        let Some(store) = &self.store else {
            return false;
        };
        let current = session == store.session();
        if current && inner.busy {
            return false;
        }
        if !store.delete_session(session) {
            return false;
        }
        if current {
            inner.messages.clear();
            inner.confirm = None;
        }
        if inner.viewing == Some(session) {
            inner.viewing = None;
        }
        bump(&mut inner);
        true
    }

    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.store
            .as_ref()
            .map_or_else(Vec::new, |store| store.sessions())
    }

    /// Conversación actual, si hay historial.
    pub fn current_session(&self) -> Option<i64> {
        self.store.as_ref().map(|store| store.session())
    }

    /// Muestra una conversación pasada (solo lectura); `None` vuelve a la actual.
    pub fn view_session(&self, session: Option<i64>) {
        let mut inner = self.lock();
        let current = self.current_session();
        inner.viewing = match session {
            Some(value) if Some(value) != current => Some(value),
            _ => None,
        };
        bump(&mut inner);
    }

    /// Conversación pasada que se está viendo.
    pub fn viewing(&self) -> Option<i64> {
        self.lock().viewing
    }

    pub fn set_voice_replies(&self, enabled: bool) {
        let mut inner = self.lock();
        if inner.voice_replies != enabled {
            inner.voice_replies = enabled;
            bump(&mut inner);
        }
    }

    pub fn voice_replies(&self) -> bool {
        self.lock().voice_replies
    }

    /// El micrófono está en pausa (el mismo estado que el botón de la barra).
    pub fn mic_muted(&self) -> bool {
        self.lock().mic_muted
    }

    pub fn set_mic_muted(&self, muted: bool) {
        let mut inner = self.lock();
        if inner.mic_muted != muted {
            inner.mic_muted = muted;
            bump(&mut inner);
        }
    }

    // -------------------------------------------------------------- lectura
    pub fn version(&self) -> i64 {
        self.lock().version
    }

    pub fn busy(&self) -> bool {
        self.lock().busy
    }

    pub fn confirm(&self) -> Option<ConfirmCard> {
        self.lock().confirm.clone()
    }

    /// `(versión, mensajes)` de lo que se está viendo, copiados.
    pub fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        let inner = self.lock();
        let shown = match (inner.viewing, &self.store) {
            (Some(viewing), Some(store)) => store
                .entries(Some(viewing))
                .iter()
                .enumerate()
                .map(|(index, entry)| from_entry(index, entry))
                .collect(),
            _ => inner.messages.clone(),
        };
        (inner.version, shown)
    }

    /// Texto de la última respuesta del asistente que no falló.
    pub fn last_assistant_text(&self) -> String {
        let inner = self.lock();
        inner
            .messages
            .iter()
            .rev()
            .find(|message| {
                message.role == Role::Assistant && !message.error && !message.text.is_empty()
            })
            .map_or_else(String::new, |message| message.text.clone())
    }
}

fn push(inner: &mut Inner, role: Role, text: &str, source: Source, t: Option<f64>) -> ChatMessage {
    let message = ChatMessage {
        id: inner.next_id,
        role,
        text: text.to_string(),
        source,
        t: t.unwrap_or_else(timestamp),
        streaming: false,
        error: false,
    };
    inner.next_id += 1;
    inner.messages.push(message.clone());
    message
}

fn find(messages: &[ChatMessage], message_id: i64) -> Option<usize> {
    messages
        .iter()
        .rposition(|message| message.id == message_id)
}

fn bump(inner: &mut Inner) {
    inner.version += 1;
}

fn from_entry(index: usize, entry: &ChatEntry) -> ChatMessage {
    ChatMessage {
        id: -((index as i64) + 1),
        role: entry.role,
        text: entry.text.clone(),
        source: entry.source,
        t: entry.t,
        streaming: false,
        error: false,
    }
}

#[cfg(test)]
mod tests;
