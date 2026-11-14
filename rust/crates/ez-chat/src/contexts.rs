//! Port of `src/ezagent/voice/chat/contexts.py` (F3-14).
//!
//! Un contexto de modelo por conversación del chat. Si no hay contexto en
//! memoria, se reconstruye con el historial.

use std::sync::Arc;

use crate::session::ChatModel;
use crate::store::{as_messages, ModelMessage, Role};

// DECISION: 8 contextos cubren la lista visible; los demás se reconstruyen.
pub const MAX_SAVED: usize = 8;

/// Lo que se le pide al `Messenger`: exportar e importar su contexto.
pub trait ContextHolder: Send + Sync {
    fn reset_history(&self);
    fn export_context(&self) -> (Vec<ModelMessage>, String, String);
    fn import_context(&self, messages: Vec<ModelMessage>, summary: &str, last_user: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedContext {
    pub messages: Vec<ModelMessage>,
    pub summary: String,
    pub last_user: String,
}

type HolderSource = Box<dyn Fn() -> Option<Arc<dyn ContextHolder>> + Send + Sync>;

/// Reparte el contexto del modelo entre las conversaciones del chat.
pub struct ChatContexts {
    pub chat: Arc<ChatModel>,
    holder: HolderSource,
    /// Pares ordenados por recencia (el último es el más reciente).
    pub saved: Vec<(i64, SavedContext)>,
}

impl ChatContexts {
    pub fn new(chat: Arc<ChatModel>, holder: HolderSource) -> Self {
        Self {
            chat,
            holder,
            saved: Vec::new(),
        }
    }

    fn holder(&self) -> Option<Arc<dyn ContextHolder>> {
        (self.holder)()
    }

    // ------------------------------------------------------------ internos
    fn take(&mut self, session: i64) -> Option<SavedContext> {
        let index = self.saved.iter().position(|(key, _)| *key == session)?;
        Some(self.saved.remove(index).1)
    }

    /// Deja `session` como el contexto más reciente y expulsa el más antiguo.
    fn remember(&mut self, session: i64, saved: SavedContext) {
        self.saved.retain(|(key, _)| *key != session);
        self.saved.push((session, saved));
        while self.saved.len() > MAX_SAVED {
            let current = self.chat.current_session();
            let Some(victim) = self.saved.iter().position(|(key, _)| Some(*key) != current) else {
                break;
            };
            self.saved.remove(victim);
        }
    }

    /// Guarda el contexto de la conversación que se está dejando.
    fn park_current(&mut self) {
        let (Some(holder), Some(session)) = (self.holder(), self.chat.current_session()) else {
            return;
        };
        let (messages, summary, last_user) = holder.export_context();
        self.remember(
            session,
            SavedContext {
                messages,
                summary,
                last_user,
            },
        );
    }

    fn load(&mut self, session: i64) {
        let Some(holder) = self.holder() else {
            return;
        };
        if let Some(found) = self.take(session) {
            holder.import_context(found.messages.clone(), &found.summary, &found.last_user);
            self.remember(session, found);
            return;
        }
        self.rebuild(Some(session));
    }

    // ------------------------------------------------------------ operaciones
    /// Reconstruye el contexto de una conversación con lo guardado en el historial.
    pub fn rebuild(&mut self, session: Option<i64>) {
        let Some(holder) = self.holder() else {
            return;
        };
        let Some(store) = self.chat.store() else {
            return;
        };
        let entries = store.entries(session);
        let messages = as_messages(&entries);
        let last_user = entries
            .iter()
            .rev()
            .find(|entry| entry.role == Role::User)
            .map_or_else(String::new, |entry| entry.text.clone());
        holder.import_context(messages, "", &last_user);
    }

    /// Conversación nueva: la actual se guarda aparte y el modelo empieza de cero.
    pub fn new_chat(&mut self) {
        self.park_current();
        if let Some(holder) = self.holder() {
            holder.reset_history();
        }
        self.chat.new_session();
    }

    /// Retoma otra conversación con su propio contexto. `false` si no se pudo.
    pub fn switch(&mut self, session: i64) -> bool {
        let Some(current) = self.chat.current_session() else {
            return false;
        };
        if session == current {
            return false;
        }
        self.park_current();
        if !self.chat.switch_session(session) {
            self.saved.retain(|(key, _)| *key != current);
            return false;
        }
        self.load(session);
        tracing::info!("Chat: conversación {} retomada.", session);
        true
    }

    /// Borra una conversación y su contexto; si era la actual, el modelo empieza de cero.
    pub fn delete(&mut self, session: i64) -> bool {
        let was_current = Some(session) == self.chat.current_session();
        if !self.chat.delete_session(session) {
            return false;
        }
        self.saved.retain(|(key, _)| *key != session);
        if was_current {
            if let Some(holder) = self.holder() {
                holder.reset_history();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::store::{ChatStore, Source};

    #[derive(Default)]
    struct HolderState {
        messages: Vec<ModelMessage>,
        summary: String,
        last_user: String,
    }

    struct FakeHolder {
        state: Mutex<HolderState>,
    }

    impl FakeHolder {
        fn new() -> Self {
            Self {
                state: Mutex::new(HolderState {
                    messages: vec![ModelMessage {
                        role: Role::System,
                        content: "sys".into(),
                    }],
                    ..HolderState::default()
                }),
            }
        }

        fn say(&self, user: &str, answer: &str) {
            let mut state = self.state.lock().expect("lock");
            state.messages.push(ModelMessage {
                role: Role::User,
                content: user.into(),
            });
            state.messages.push(ModelMessage {
                role: Role::Assistant,
                content: answer.into(),
            });
            state.last_user = user.into();
        }

        fn bodies(&self) -> Vec<String> {
            self.state
                .lock()
                .expect("lock")
                .messages
                .iter()
                .filter(|message| message.role != Role::System)
                .map(|message| message.content.clone())
                .collect()
        }
    }

    impl ContextHolder for FakeHolder {
        fn reset_history(&self) {
            let mut state = self.state.lock().expect("lock");
            state.messages.truncate(1);
            state.summary.clear();
            state.last_user.clear();
        }

        fn export_context(&self) -> (Vec<ModelMessage>, String, String) {
            let state = self.state.lock().expect("lock");
            (
                state.messages.clone(),
                state.summary.clone(),
                state.last_user.clone(),
            )
        }

        fn import_context(&self, messages: Vec<ModelMessage>, summary: &str, last_user: &str) {
            let mut state = self.state.lock().expect("lock");
            let mut body: Vec<ModelMessage> = messages
                .into_iter()
                .filter(|message| message.role != Role::System)
                .collect();
            let mut all = vec![ModelMessage {
                role: Role::System,
                content: "sys".into(),
            }];
            all.append(&mut body);
            state.messages = all;
            state.summary = summary.to_string();
            state.last_user = last_user.to_string();
        }
    }

    fn rig(name: &str) -> (Arc<ChatModel>, Arc<FakeHolder>, ChatContexts) {
        let path = std::env::temp_dir()
            .join(format!("ez-chat-contexts-{name}-{}", std::process::id()))
            .join("chat.jsonl");
        let chat = Arc::new(ChatModel::new(
            Some(Arc::new(ChatStore::new(Some(path)))),
            false,
        ));
        let holder = Arc::new(FakeHolder::new());
        let weak = Arc::downgrade(&holder);
        let contexts = ChatContexts::new(
            Arc::clone(&chat),
            Box::new(move || {
                weak.upgrade()
                    .map(|holder| holder as Arc<dyn ContextHolder>)
            }),
        );
        (chat, holder, contexts)
    }

    #[test]
    fn each_conversation_keeps_its_own_context() {
        let (chat, holder, mut contexts) = rig("own");
        chat.add_user("mi color es azul", Source::Chat);
        chat.begin_assistant(Source::Chat);
        holder.say("mi color es azul", "anotado");
        let a = chat.current_session().expect("sesión");
        contexts.new_chat();
        assert!(holder.bodies().is_empty());
        chat.add_user("mi color es rojo", Source::Chat);
        holder.say("mi color es rojo", "listo");
        assert!(contexts.switch(a));
        assert_eq!(holder.bodies(), vec!["mi color es azul", "anotado"]);
    }

    #[test]
    fn the_summary_travels_with_its_conversation() {
        let (chat, holder, mut contexts) = rig("summary");
        chat.add_user("hola", Source::Chat);
        let a = chat.current_session().expect("sesión");
        holder.state.lock().expect("lock").summary = "Resumen de A".into();
        contexts.new_chat();
        assert!(holder.state.lock().expect("lock").summary.is_empty());
        assert!(contexts.switch(a));
        assert_eq!(holder.state.lock().expect("lock").summary, "Resumen de A");
    }

    #[test]
    fn without_saved_context_it_is_rebuilt_from_the_history() {
        let (chat, holder, mut contexts) = rig("rebuild");
        chat.add_user("recuérdame esto", Source::Chat);
        let reply = chat.begin_assistant(Source::Chat);
        chat.append(reply.id, "hecho");
        chat.finish(reply.id);
        let a = chat.current_session().expect("sesión");
        chat.new_session();
        assert!(contexts.switch(a));
        assert_eq!(holder.bodies(), vec!["recuérdame esto", "hecho"]);
        assert_eq!(
            holder.state.lock().expect("lock").last_user,
            "recuérdame esto"
        );
    }

    #[test]
    fn switching_to_the_current_or_a_missing_conversation_keeps_the_context() {
        let (chat, holder, mut contexts) = rig("missing");
        chat.add_user("hola", Source::Chat);
        holder.say("hola", "buenas");
        let current = chat.current_session().expect("sesión");
        assert!(!contexts.switch(current));
        assert!(!contexts.switch(404));
        assert_eq!(holder.bodies(), vec!["hola", "buenas"]);
    }

    #[test]
    fn deleting_a_conversation_drops_its_context() {
        let (chat, holder, mut contexts) = rig("delete");
        chat.add_user("vieja", Source::Chat);
        holder.say("vieja", "x");
        let old = chat.current_session().expect("sesión");
        contexts.new_chat();
        chat.add_user("actual", Source::Chat);
        holder.say("actual", "y");
        assert!(contexts.saved.iter().any(|(key, _)| *key == old));
        assert!(contexts.delete(old));
        assert!(!contexts.saved.iter().any(|(key, _)| *key == old));
        assert!(contexts.delete(chat.current_session().expect("sesión")));
        assert!(holder.bodies().is_empty());
    }
}
