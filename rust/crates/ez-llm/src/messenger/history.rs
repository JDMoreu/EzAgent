//! Port of the history half of `Messenger in src/ezagent/voice/llm.py` (F4-07).
//!
//! El turno es el único dueño del historial mientras corre; lo que llega desde
//! la UI (reinicios de contexto, avisos de agentes) se aparca en `deferred` /
//! `pending_notes` y se aplica al terminar. El `RLock` del Python es un `Mutex`
//! con secciones críticas cortas.

use std::sync::MutexGuard;

use serde_json::{json, Value};

use super::Messenger;

/// Estado del historial y lo que la UI puede diferir hasta el fin del turno.
pub(crate) struct HistoryState {
    pub messages: Vec<Value>,
    pub summary: String,
    pub last_user_text: String,
    pub turn_active: bool,
    pub pending_notes: Vec<Value>,
    pub deferred: Option<(Vec<Value>, String, String)>,
    pub last_agent_address: Option<String>,
}

impl HistoryState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            summary: String::new(),
            last_user_text: String::new(),
            turn_active: false,
            pending_notes: Vec::new(),
            deferred: None,
            last_agent_address: None,
        }
    }
}

impl Default for HistoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl Messenger {
    /// El lock del historial (el `RLock` del Python).
    pub(crate) fn history(&self) -> MutexGuard<'_, HistoryState> {
        super::lock(&self.shared.history)
    }

    /// Copia del historial actual (`messenger.messages` del Python).
    pub fn messages(&self) -> Vec<Value> {
        self.history().messages.clone()
    }

    /// Sustituye el historial sin pasar por la cola de diferidos (uso de la app).
    pub fn set_messages(&self, messages: Vec<Value>) {
        self.history().messages = messages;
    }

    /// Resumen de lo que ya salió del historial por la ventana de contexto.
    pub fn summary(&self) -> String {
        self.history().summary.clone()
    }

    /// `_replace_history`: cambia el contexto sin pisar un turno activo.
    pub(crate) fn replace_history(&self, messages: Vec<Value>, summary: &str, last_user: &str) {
        let mut history = self.history();
        if history.turn_active {
            history.deferred = Some((messages, summary.to_string(), last_user.to_string()));
            return;
        }
        history.messages = messages;
        history.summary = summary.to_string();
        history.last_user_text = last_user.to_string();
    }

    /// `_apply_deferred_history`: aplica lo aparcado al soltar el turno.
    pub fn apply_deferred_history(&self) {
        let mut history = self.history();
        let deferred = history.deferred.take();
        if let Some((messages, summary, last_user)) = deferred {
            history.messages = messages;
            history.summary = summary;
            history.last_user_text = last_user;
            history.pending_notes.clear();
        }
    }

    /// `_take_pending_notes`: notas de agentes que esperan al siguiente turno.
    pub fn take_pending_notes(&self) -> Vec<Value> {
        let mut history = self.history();
        std::mem::take(&mut history.pending_notes)
    }

    /// `reset_history`: olvida la conversación (conserva solo el mensaje de sistema).
    pub fn reset_history(&self) {
        let prompt = self.system_prompt();
        let mut history = self.history();
        let system = match history.messages.first() {
            Some(message) if message.get("role").and_then(Value::as_str) == Some("system") => {
                message.clone()
            }
            _ => json!({"role": "system", "content": prompt}),
        };
        history.pending_notes.clear();
        if history.turn_active {
            history.deferred = Some((vec![system], String::new(), String::new()));
            return;
        }
        history.messages = vec![system];
        history.last_user_text.clear();
        history.summary.clear();
    }

    /// `export_context`: `(mensajes, resumen, último texto del usuario)`.
    pub fn export_context(&self) -> (Vec<Value>, String, String) {
        let history = self.history();
        (
            history.messages.clone(),
            history.summary.clone(),
            history.last_user_text.clone(),
        )
    }

    /// `import_context`: pone ese contexto como el de la conversación actual
    /// (con el prompt de sistema vigente).
    pub fn import_context(&self, messages: Vec<Value>, summary: &str, last_user: &str) {
        let mut context = vec![json!({"role": "system", "content": self.system_prompt()})];
        context.extend(
            messages
                .into_iter()
                .filter(|message| message.get("role").and_then(Value::as_str) != Some("system")),
        );
        let mut history = self.history();
        history.pending_notes.clear();
        if history.turn_active {
            history.deferred = Some((context, summary.to_string(), last_user.to_string()));
            return;
        }
        history.messages = context;
        history.summary = summary.to_string();
        history.last_user_text = last_user.to_string();
    }

    /// `note_done`'s tail (F4-09): guarda un aviso ahora o al empezar el próximo turno.
    pub fn append_context_message(&self, message: Value) {
        let mut history = self.history();
        if history.turn_active {
            // Nunca se inserta entre assistant+tool_calls y sus resultados.
            history.pending_notes.push(message);
            return;
        }
        history.messages.push(message);
    }

    /// `_trim_history`: recorta por grupos completos, nunca entre assistant y tool.
    /// (El Python ya no lo llama: el tope real es la ventana de contexto.)
    pub fn trim_history(&self, keep: usize) {
        let mut history = self.history();
        let messages = &mut history.messages;
        if messages.len() <= keep + 2 {
            return;
        }
        let system = messages[0].clone();
        let mut rest: Vec<Value> = messages[1..].to_vec();
        while rest.len() > keep {
            rest.remove(0);
        }
        // El primer mensaje del historial debe ser del usuario: así no queda un
        // assistant.tool_calls sin sus resultados tool ni un tool huérfano.
        while rest
            .first()
            .is_some_and(|message| message.get("role").and_then(Value::as_str) != Some("user"))
        {
            rest.remove(0);
        }
        *messages = std::iter::once(system).chain(rest).collect();
    }

    /// `_wants_notify`: el usuario pidió que se le avise.
    pub fn wants_notify(&self) -> bool {
        wants_any(&self.history().last_user_text, NOTIFY_WORDS)
    }

    /// `_wants_close`: el usuario pidió cerrar la escucha.
    pub fn wants_close(&self) -> bool {
        wants_any(&self.history().last_user_text, CLOSE_WORDS)
    }

    /// `pending_notify_address`: último agente lanzado, si pidió que se le avise.
    pub fn pending_notify_address(&self) -> Option<String> {
        if let Some(runtime) = self.runtime() {
            return runtime.pending_notify_address();
        }
        if !self.wants_notify() {
            return None;
        }
        self.history().last_agent_address.clone()
    }
}

/// `str.lower()` + `any(word in text)`.
fn wants_any(text: &str, words: &[&str]) -> bool {
    let text = text.to_lowercase();
    words.iter().any(|word| text.contains(word))
}

/// Palabras de `_wants_notify`, en el mismo orden que el Python.
const NOTIFY_WORDS: &[&str] = &[
    "avísame",
    "avisame",
    "avisa",
    "avísame cuando",
    "me avisas",
    "me avise",
    "me avises",
    "notifícame",
    "notificame",
    "notifica",
    "cuando termine",
    "cuando acabe",
    "cuando hayas terminado",
    "cuando esté",
    "cuando este",
    "al terminar",
    "al acabar",
    "estate atento",
    "estate pendiente",
    "esté atento",
    "este atento",
    "vigila",
    "vigílame",
    "vigilame",
    "pendiente de",
    "me dice",
    "me digas",
    "me dices",
    "dime",
    "me pasas",
    "me cuentas",
    "resumen",
    "resúmeme",
    "resumeme",
];

/// Palabras de `_wants_close`.
const CLOSE_WORDS: &[&str] = &[
    "ciérrate",
    "cierrate",
    "cierra la escucha",
    "cierra el micro",
    "deja de escuchar",
    "sin escuchar",
];

#[cfg(test)]
mod tests {
    use crate::messenger::{Messenger, MessengerCallbacks};
    use ez_core::config::VoiceConfig;
    use serde_json::json;

    fn messenger() -> Messenger {
        Messenger::new(VoiceConfig::new(), MessengerCallbacks::default(), None)
    }

    #[test]
    fn reset_history_keeps_only_the_system_prompt() {
        let messenger = messenger();
        messenger.import_context(
            vec![
                json!({"role": "user", "content": "hola"}),
                json!({"role": "assistant", "content": "adiós"}),
            ],
            "resumen",
            "hola",
        );
        assert_eq!(messenger.messages().len(), 3);
        messenger.reset_history();
        let messages = messenger.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messenger.summary(), "");
    }

    #[test]
    fn import_replaces_the_incoming_system_prompt() {
        let messenger = messenger();
        messenger.import_context(
            vec![
                json!({"role": "system", "content": "viejo"}),
                json!({"role": "user", "content": "hola"}),
            ],
            "",
            "hola",
        );
        let messages = messenger.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_ne!(messages[0]["content"], "viejo");
        assert!(messages[0]["content"]
            .as_str()
            .is_some_and(|text| text.contains("núcleo agéntico")));
        assert_eq!(messages[1]["content"], "hola");
        assert_eq!(
            messenger.export_context(),
            (messages, String::new(), "hola".to_string())
        );
    }

    #[test]
    fn changes_are_deferred_while_a_turn_is_active() {
        let messenger = messenger();
        messenger.reset_history();
        messenger.history().turn_active = true;
        messenger.import_context(
            vec![json!({"role": "user", "content": "nueva"})],
            "s",
            "nueva",
        );
        assert_eq!(messenger.messages().len(), 1);
        messenger.history().turn_active = false;
        messenger.apply_deferred_history();
        assert_eq!(messenger.messages().len(), 2);
        assert_eq!(messenger.messages()[1]["content"], "nueva");
        assert_eq!(messenger.summary(), "s");
    }

    #[test]
    fn trim_never_starts_with_a_tool_result() {
        let messenger = messenger();
        let mut messages = vec![json!({"role": "system", "content": "s"})];
        for index in 0..10 {
            messages.push(json!({"role": "user", "content": format!("u{index}")}));
            messages.push(json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": format!("c{index}")}
            ]}));
            messages.push(
                json!({"role": "tool", "tool_call_id": format!("c{index}"), "content": "{}"}),
            );
            messages.push(json!({"role": "assistant", "content": format!("a{index}")}));
        }
        messenger.set_messages(messages);
        messenger.trim_history(12);
        let messages = messenger.messages();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        for index in 1..messages.len() {
            if messages[index]["role"] == "tool" {
                assert_eq!(messages[index - 1]["role"], "assistant");
                assert_eq!(
                    messages[index - 1]["tool_calls"][0]["id"],
                    messages[index]["tool_call_id"]
                );
            }
        }
    }

    #[test]
    fn short_history_is_left_alone() {
        let messenger = messenger();
        let before: Vec<_> = (0..6)
            .map(|index| json!({"role": "user", "content": format!("x{index}")}))
            .collect();
        messenger.set_messages(before.clone());
        messenger.trim_history(12);
        assert_eq!(messenger.messages(), before);
    }

    #[test]
    fn notices_follow_the_turn() {
        let messenger = messenger();
        messenger.history().turn_active = true;
        messenger.append_context_message(json!({"role": "user", "content": "nota"}));
        assert_eq!(messenger.messages().len(), 1);
        let pending = messenger.take_pending_notes();
        assert_eq!(pending.len(), 1);
        messenger.history().turn_active = false;
        messenger.append_context_message(json!({"role": "user", "content": "otra"}));
        assert_eq!(messenger.messages().len(), 2);
    }

    #[test]
    fn notify_words_match_the_python_lists() {
        let messenger = messenger();
        messenger.history().last_user_text = "Avísame cuando termine".into();
        assert!(messenger.wants_notify());
        messenger.history().last_user_text = "cierra el micro".into();
        assert!(messenger.wants_close());
        messenger.history().last_user_text = "solo charla".into();
        assert!(!messenger.wants_notify() && !messenger.wants_close());
    }
}
