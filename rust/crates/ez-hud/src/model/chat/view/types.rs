//! Input values, actions and seams of the chat view (`chat_view.py`, F5-14).

use ez_gfx::Rgba;

use crate::paint::chat::labels::LabelMessage;
use crate::paint::chat::{ChatHost, FileInfo, Span};
use crate::tokens::Rect;

/// What the view asks the window owner to do.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// `send | close | voice | mic | settings | copy | open | reveal | confirm | stop | new | switch | delete`.
    pub kind: &'static str,
    /// Payload, when the action has one.
    pub data: Option<ActionData>,
}

impl Action {
    /// Action without payload.
    pub const fn new(kind: &'static str) -> Self {
        Self { kind, data: None }
    }

    /// Action with payload.
    pub fn with(kind: &'static str, data: ActionData) -> Self {
        Self {
            kind,
            data: Some(data),
        }
    }
}

/// Payload of an [`Action`] (`data: object` in Python).
#[derive(Clone, Debug, PartialEq)]
pub enum ActionData {
    /// Text to send or copy.
    Text(String),
    /// File to open or reveal.
    File(FileInfo),
    /// Conversation row.
    Session(i64),
    /// On/off answer (voice, mic, confirmation).
    Flag(bool),
}

/// `session.ChatMessage.role`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Message written or spoken by the user.
    User,
    /// EzAgent answer.
    Assistant,
    /// Note from the app.
    System,
}

impl Role {
    /// The Python role string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

/// One visible message (`session.ChatMessage`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    /// Stable id; history messages use negative ids.
    pub id: i64,
    /// Who wrote it.
    pub role: Role,
    /// Visible text.
    pub text: String,
    /// Origin: «chat», «voice»…
    pub source: String,
    /// Unix time of the message.
    pub t: f64,
    /// Still growing.
    pub streaming: bool,
    /// Failed answer.
    pub error: bool,
}

impl ChatMessage {
    /// Message with the Python defaults: «chat» source, now, not streaming.
    pub fn new(id: i64, role: Role, text: impl Into<String>) -> Self {
        Self {
            id,
            role,
            text: text.into(),
            source: "chat".to_string(),
            t: ez_core::pytime::timestamp(),
            streaming: false,
            error: false,
        }
    }
}

impl LabelMessage for ChatMessage {
    fn role(&self) -> &str {
        self.role.as_str()
    }

    fn streaming(&self) -> bool {
        self.streaming
    }

    fn timestamp(&self) -> f64 {
        self.t
    }
}

/// A conversation of the history (`store.SessionInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionInfo {
    /// Conversation id.
    pub session: i64,
    /// Title derived from the first request.
    pub title: String,
    /// Unix time it started.
    pub started: f64,
    /// Number of saved messages.
    pub messages: usize,
}

/// Pending confirmation card (`session.ConfirmCard`).
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmCard {
    /// Summary of what will happen.
    pub summary: String,
    /// Dangerous action.
    pub strong: bool,
}

/// What the view needs from the chat model.
///
/// DECISION: mirrors the attributes of `session.ChatModel` the view reads;
/// F5-15/F6 adapt the real model to this trait.
pub trait ChatModel {
    /// `(version, messages)` of what is shown, copied.
    fn snapshot(&self) -> (i64, Vec<ChatMessage>);
    /// Saved conversations, newest first.
    fn sessions(&self) -> Vec<SessionInfo>;
    /// The model is answering.
    fn busy(&self) -> bool;
    /// Pending confirmation, if any.
    fn confirm(&self) -> Option<ConfirmCard>;
    /// Conversation shown read-only, if any.
    fn viewing(&self) -> Option<i64>;
    /// Current conversation id.
    fn current_session(&self) -> Option<i64>;
    /// Voice answers are on.
    fn voice_replies(&self) -> bool;
    /// Microphone is paused.
    fn mic_muted(&self) -> bool;
    /// Shows a past conversation read-only; `None` returns to the current one.
    fn view_session(&mut self, session: Option<i64>);
}

/// Cursor movement of the text field (`text_input.Move`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// One line up.
    Up,
    /// One line down.
    Down,
    /// Start of the line.
    Home,
    /// End of the line.
    End,
}

/// What the view needs from the input box.
///
/// DECISION: `text_input.TextInput` lives in `ez-chat`; the view keeps this
/// seam and F5-15/F6 adapt the real field. Indices count code points.
pub trait TextField {
    /// Current text.
    fn text(&self) -> &str;
    /// Caret position.
    fn cursor(&self) -> usize;
    /// No visible text (`not text.strip()`).
    fn empty(&self) -> bool;
    /// Writes or pastes at the caret.
    fn insert(&mut self, text: &str);
    /// Inserts a newline.
    fn newline(&mut self);
    /// Text to send, stripped; the field is cleared.
    fn take(&mut self) -> String;
    /// Moves the caret, extending the selection when `select`.
    fn move_caret(&mut self, key: Move, select: bool, word: bool);
    /// Deletes backwards.
    fn backspace(&mut self, word: bool);
    /// Deletes forwards.
    fn delete(&mut self, word: bool);
    /// Selects everything.
    fn select_all(&mut self);
    /// Selected text, or "".
    fn selected_text(&self) -> String;
    /// Selection bounds, if any.
    fn selection(&self) -> Option<(usize, usize)>;
    /// Empties the field.
    fn clear(&mut self);
    /// Puts the caret at `cursor` and drops the selection anchor.
    fn place_caret(&mut self, cursor: usize);
}

/// Parsed markdown plus the inline parser the view needs for notes and
/// confirmations.
///
/// DECISION: F5-13's [`ChatHost`] only exposes the block parser; the view also
/// needs `markdown.parse_inline`, so this trait extends it.
pub trait ViewHost: ChatHost {
    /// Parses a line of inline markdown.
    fn parse_inline(&self, text: &str) -> Vec<Span>;
}

/// Icon painter of the view: the real vector rasterizer from `ez-gfx`.
pub(crate) struct GfxIcons;

impl crate::paint::chat::IconPainter for GfxIcons {
    fn draw(
        &self,
        tile: &mut [f32],
        side: u32,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Rgba,
        scale: f64,
    ) {
        let _ = ez_gfx::icons::draw_icon(
            tile,
            side as usize,
            side as usize,
            name,
            cx,
            cy,
            size,
            ez_gfx::icons::Color::Rgba(color.r, color.g, color.b, color.a),
            scale,
            1.0,
        );
    }
}

pub(crate) static GFX_ICONS: GfxIcons = GfxIcons;

/// A clickable zone of the view, in window coordinates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Zone {
    pub kind: &'static str,
    pub rect: Rect,
    pub data: Option<ZoneData>,
}

/// Payload of a zone (`Hit.data` in Python).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ZoneData {
    Text(String),
    File(FileInfo),
    Session(i64),
}

impl ZoneData {
    pub(crate) fn into_action(self) -> ActionData {
        match self {
            Self::Text(text) => ActionData::Text(text),
            Self::File(file) => ActionData::File(file),
            Self::Session(session) => ActionData::Session(session),
        }
    }
}

/// `chat` or `history`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Messages of the current conversation.
    #[default]
    Chat,
    /// Saved conversations.
    History,
}

impl Mode {
    /// The Python mode string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::History => "history",
        }
    }
}

/// Deleting history row: `(index, row, start time)`.
pub(crate) type Deleting = (usize, SessionInfo, f64);
