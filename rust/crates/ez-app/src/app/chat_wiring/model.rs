//! Adapter from `ez_chat::session::ChatModel` to the view model (F6-08).

use std::sync::Arc;

use ez_chat::session::ChatModel;
use ez_chat::store::{Role, SessionInfo};
use ez_hud::model::chat::view::{
    ChatMessage, ChatModel as ViewModel, ConfirmCard as ViewConfirmCard, Role as ViewRole,
    SessionInfo as ViewSessionInfo,
};

/// `self.chat` as the `ez-hud` view sees it (`chat_model` in `app.py`).
pub(super) struct HudModel {
    chat: Arc<ChatModel>,
}

impl HudModel {
    pub(super) fn new(chat: Arc<ChatModel>) -> Self {
        Self { chat }
    }
}

/// `session.Role` → `chat_view.Role`.
fn role(role: Role) -> ViewRole {
    match role {
        Role::User => ViewRole::User,
        Role::Assistant => ViewRole::Assistant,
        Role::System => ViewRole::System,
    }
}

impl ViewModel for HudModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        let (version, messages) = self.chat.snapshot();
        let messages = messages
            .into_iter()
            .map(|message| ChatMessage {
                id: message.id,
                role: role(message.role),
                text: message.text,
                source: message.source.as_str().to_string(),
                t: message.t,
                streaming: message.streaming,
                error: message.error,
            })
            .collect();
        (version, messages)
    }

    fn sessions(&self) -> Vec<ViewSessionInfo> {
        self.chat
            .sessions()
            .into_iter()
            .map(|info: SessionInfo| ViewSessionInfo {
                session: info.session,
                title: info.title,
                started: info.started,
                messages: info.messages,
            })
            .collect()
    }

    fn busy(&self) -> bool {
        self.chat.busy()
    }

    fn confirm(&self) -> Option<ViewConfirmCard> {
        self.chat.confirm().map(|card| ViewConfirmCard {
            summary: card.summary,
            strong: card.strong,
        })
    }

    fn viewing(&self) -> Option<i64> {
        self.chat.viewing()
    }

    fn current_session(&self) -> Option<i64> {
        self.chat.current_session()
    }

    fn voice_replies(&self) -> bool {
        self.chat.voice_replies()
    }

    fn mic_muted(&self) -> bool {
        self.chat.mic_muted()
    }

    fn view_session(&mut self, session: Option<i64>) {
        self.chat.view_session(session);
    }
}
