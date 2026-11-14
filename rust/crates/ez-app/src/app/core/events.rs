//! Colas de la aplicación (F6-01).
//!
//! `app.py` usa `queue.Queue` para tres flujos: los eventos de la UI
//! (`self.events`), los atajos (`self.hotkey_events`) y las intenciones del
//! menú (`self.menu_events`). Aquí son canales `crossbeam-channel` con un
//! receptor propio y un `wake` opcional que despierta el bucle Win32.

use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use ez_hud::model::settings::SettingsValues;
use ez_hud::win::drop::ConfigValue;
use ez_hud::win::ui_loop::UiLoop;
use parking_lot::Mutex;
use serde_json::Value;

/// Evento de `self.events`.
///
/// DECISION: los Python son tuplas con nombre en la primera posición; un enum
/// conserva ese contrato y evita `Value` en el camino caliente.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    /// `("error", mensaje[, gen])`.
    Error(String, Option<i64>),
    /// `("busy_notice", turn_gen, mode)`.
    BusyNotice(i64, String),
    /// `("menu_action", action)`.
    MenuAction(String),
    /// `("hotkey_capture", active)`.
    HotkeyCapture(bool),
    /// `("chat_new",)`.
    ChatNew,
    /// `("chat_stop",)`.
    ChatStop,
    /// `("chat_switch", session)`.
    ChatSwitch(i64),
    /// `("chat_delete", session)`.
    ChatDelete(i64),
    /// `("confirm_click", yes[, token])`.
    ConfirmClick(bool, Option<String>),
    /// `("chat_send", text)`.
    ChatSend(String),
    /// `("activity_open", key)`.
    ActivityOpen(String),
    /// `("activity_dismiss", key)`.
    ActivityDismiss(String),
    /// `("chat_mute", muted)`.
    ChatMute(bool),
    /// `("hud_action", action)`.
    HudAction(String),
    /// `("swarm_notice", text, reopen)`.
    SwarmNotice(String, bool),
    /// `("confirm_ask", summary, strong, token)`.
    ConfirmAsk(String, bool, Option<String>),
    /// `("confirm_done",)`.
    ConfirmDone,
    /// `("agent_warn", message)`.
    AgentWarn(String),
    /// `("tray_action", action)`.
    TrayAction(String),
    /// `("audio_speech_start", level)`.
    AudioSpeechStart(f64),
    /// `("audio_utterance", pcm, rate)`.
    AudioUtterance(Vec<u8>, u32),
    /// `("audio_barge_in", level)`.
    AudioBargeIn(f64),
    /// `("transcribed", text, gen)`.
    Transcribed(String, i64),
    /// `("speaking", gen)`.
    Speaking(i64),
    /// `("answer_done", gen, outcome)`.
    AnswerDone(i64, String),
    /// `("speak_done", generation)`.
    SpeakDone(i64),
    /// `("busy_done", generation)`.
    BusyDone(i64),
    /// `("done_prompt", address, agent, reason, output, task)`.
    DonePrompt {
        address: String,
        agent: String,
        reason: String,
        output: String,
        task: String,
    },
    /// `("done_answer", address, agent, text, task)`.
    DoneAnswer {
        address: String,
        agent: String,
        text: String,
        task: String,
    },
    /// `("say", text)`.
    Say(String),
    /// `("test_done", old_muted, old_barge)`.
    TestDone(bool, f64),
    /// `("llm_models_ready", provider, models)`.
    LlmModelsReady(String, Vec<String>),
    /// `("close_request", mode)`.
    CloseRequest(String),
    /// `("wake",)`.
    Wake,
    /// `("hide",)`.
    Hide,
    /// Evento que F6-02+ necesite y aún no tenga variante.
    Raw(Vec<Value>),
}

/// Intención del menú de ajustes (`self.menu_events`).
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEvent {
    /// `("live", key, value)`.
    Live(String, ConfigValue),
    /// `("save", dict(values))`.
    Save(SettingsValues),
    /// `("test", dict(values))`.
    Test(SettingsValues),
    /// `("hide",)`.
    Hide,
}

/// Cola con receptor propio y despertador opcional del bucle de la interfaz.
pub struct Queue<T> {
    sender: Sender<T>,
    receiver: Mutex<Receiver<T>>,
    wake: Option<UiLoop>,
}

impl<T: Send> Queue<T> {
    /// Crea la cola; con `wake`, cada `put` despierta el bucle Win32.
    pub fn new(wake: Option<UiLoop>) -> Arc<Self> {
        let (sender, receiver) = unbounded();
        Arc::new(Self {
            sender,
            receiver: Mutex::new(receiver),
            wake,
        })
    }

    /// `Queue.put`: nunca bloquea (el canal es ilimitado).
    pub fn put(&self, value: T) {
        let _ = self.sender.send(value);
        if let Some(wake) = &self.wake {
            wake.wake();
        }
    }

    /// `Queue.get_nowait`.
    pub fn get_nowait(&self) -> Option<T> {
        self.receiver.lock().try_recv().ok()
    }

    /// Extremo emisor, para quien publica desde otro hilo (`GlobalHotkey`).
    pub fn sender(&self) -> Sender<T> {
        self.sender.clone()
    }

    /// `Queue.empty`.
    pub fn is_empty(&self) -> bool {
        self.receiver.lock().is_empty()
    }

    /// `Queue.qsize`.
    pub fn len(&self) -> usize {
        self.receiver.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_fifo_and_reports_size() {
        let queue = Queue::<AppEvent>::new(None);
        assert!(queue.is_empty());
        queue.put(AppEvent::ChatNew);
        queue.put(AppEvent::Say("hola".to_string()));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.get_nowait(), Some(AppEvent::ChatNew));
        assert_eq!(queue.get_nowait(), Some(AppEvent::Say("hola".to_string())));
        assert_eq!(queue.get_nowait(), None);
    }

    #[test]
    fn sender_publishes_from_another_thread() {
        let queue = Queue::<String>::new(None);
        let sender = queue.sender();
        let worker = std::thread::spawn(move || {
            sender.send("listo".to_string()).expect("canal vivo");
        });
        worker.join().expect("hilo");
        assert_eq!(queue.get_nowait().as_deref(), Some("listo"));
    }
}
