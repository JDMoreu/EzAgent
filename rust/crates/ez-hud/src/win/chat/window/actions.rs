//! View actions of the chat panel: callbacks, clipboard and handoff.

use crate::model::chat::view::{Action, ActionData};

use super::{ChatEvent, ChatPanel};

impl ChatPanel {
    // ---------------------------------------------------------- actions
    pub(super) fn perform(&self, action: Option<Action>) {
        let Some(action) = action else {
            return;
        };
        match (action.kind, action.data) {
            ("send", Some(ActionData::Text(text))) => {
                self.events.emit(ChatEvent::Send(text));
            }
            ("close", _) => self.close(),
            ("voice", Some(ActionData::Flag(value))) => {
                self.events.emit(ChatEvent::Voice(value));
            }
            ("mic", Some(ActionData::Flag(value))) => {
                self.events.emit(ChatEvent::Mic(value));
            }
            ("settings", _) => self.events.emit(ChatEvent::Settings),
            ("new", _) => self.events.emit(ChatEvent::New),
            ("stop", _) => self.events.emit(ChatEvent::Stop),
            ("switch", Some(ActionData::Session(session))) => {
                self.events.emit(ChatEvent::Switch(session));
            }
            ("delete", Some(ActionData::Session(session))) => {
                self.events.emit(ChatEvent::Delete(session));
            }
            ("confirm", Some(ActionData::Flag(value))) => {
                self.events.emit(ChatEvent::Confirm(value));
            }
            ("copy", Some(ActionData::Text(text))) => self.copy(&text),
            ("open", Some(ActionData::File(file))) => {
                self.events.emit(ChatEvent::Open(file));
            }
            ("reveal", Some(ActionData::File(file))) => {
                self.events.emit(ChatEvent::Reveal(file));
            }
            _ => {}
        }
    }

    pub(super) fn copy(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(window) = self.window.lock().clone() {
            window.copy_text(text);
            if let Some(view) = self.state.lock().view.as_mut() {
                view.flash_copied(text, None);
            }
        }
    }
}
