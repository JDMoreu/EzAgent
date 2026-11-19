//! Casos de la vista del chat (`chat_view.ChatView`): teclado, edición, puntero
//! y rueda. La vista se reconstruye con la conversación del generador.

use std::path::{Path, PathBuf};

use ez_gfx::Image;
use ez_hud::model::chat::view::{
    Action, ActionData, ChatMessage, ChatModel, ChatView, ConfirmCard, Role, SessionInfo, ViewHost,
};
use ez_hud::paint::chat::{Block, ChatHost, FileInfo, Span};
use ez_parity::fixtures::FixtureCase;

use super::markdown;
use super::support::{b, compare_with, flag, i, int, line, num, o, s};
use super::view_input::FakeInput;

const WALL: [i64; 6] = [2026, 11, 13, 16, 46, 12];

/// Modelo con los campos que lee la vista.
#[derive(Default)]
pub(super) struct FakeModel {
    pub(super) messages: Vec<ChatMessage>,
    pub(super) sessions: Vec<SessionInfo>,
    pub(super) busy: bool,
    pub(super) confirm: Option<ConfirmCard>,
    pub(super) viewing: Option<i64>,
    pub(super) current: Option<i64>,
    pub(super) voice: bool,
    pub(super) muted: bool,
}

impl ChatModel for FakeModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        (0, self.messages.clone())
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        self.sessions.clone()
    }

    fn busy(&self) -> bool {
        self.busy
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        self.confirm.clone()
    }

    fn viewing(&self) -> Option<i64> {
        self.viewing
    }

    fn current_session(&self) -> Option<i64> {
        self.current
    }

    fn voice_replies(&self) -> bool {
        self.voice
    }

    fn mic_muted(&self) -> bool {
        self.muted
    }

    fn view_session(&mut self, session: Option<i64>) {
        self.viewing = if session == self.current {
            None
        } else {
            session
        };
    }
}

/// Markdown real y ninguna búsqueda de archivos.
pub(super) struct TestHost;

impl ChatHost for TestHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        markdown::parse(text)
    }

    fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

impl ViewHost for TestHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        markdown::parse_inline(text)
    }
}

fn message(id: i64, role: Role, text: &str, source: &str, t: f64) -> ChatMessage {
    ChatMessage {
        id,
        role,
        text: text.to_string(),
        source: source.to_string(),
        t,
        streaming: false,
        error: false,
    }
}

/// `conversation_model()` del generador.
pub(super) fn conversation() -> FakeModel {
    let base = super::support::epoch_from_wall(&WALL);
    FakeModel {
        messages: vec![
            message(
                1,
                Role::User,
                "¿Puedes comparar estos dos planes?",
                "chat",
                base,
            ),
            message(
                2,
                Role::Assistant,
                "Claro. Aquí va la comparación:\n\n| Plan | Precio |\n|---|---:|\n\
                 | Básico | 9 € |\n| Pro | 19 € |\n\n**Recomiendo** el Pro.",
                "chat",
                base + 2.0,
            ),
            message(
                3,
                Role::User,
                "Gracias, muéstrame el informe.",
                "voice",
                base + 3.0,
            ),
            message(4, Role::Assistant, "Aquí lo tienes.", "chat", base + 4.0),
        ],
        current: Some(1),
        ..FakeModel::default()
    }
}

pub(super) fn view(model: FakeModel) -> ChatView {
    let mut view = ChatView::new(
        Box::new(model),
        Box::new(TestHost),
        Box::new(FakeInput::default()),
    );
    configure(&mut view);
    view
}

pub(super) fn configure(view: &mut ChatView) {
    view.configure(
        440.0,
        640.0,
        1.0,
        "light",
        ez_hud::tokens::palette("light").expect("la paleta clara existe"),
    );
}

fn action_json(action: &Option<Action>) -> String {
    let Some(action) = action else {
        return "null".into();
    };
    let data = match &action.data {
        None => "null".into(),
        Some(ActionData::Text(text)) => s(text),
        Some(ActionData::Flag(flag)) => b(*flag),
        Some(ActionData::Session(session)) => i(*session),
        Some(ActionData::File(file)) => s(&file.name),
    };
    o(&[("kind", s(action.kind)), ("data", data)])
}

pub(super) fn run(case: &FixtureCase, function: &str, root: &str) {
    match function {
        "chat_view.ChatView.key" => key_case(case),
        "chat_view.ChatView.edit" => edit_case(case),
        "chat_view.ChatView.add_files" => add_files(case, root),
        "chat_view.ChatView.can_start_new" => {
            let has_messages = case.case.ends_with("messages");
            let model = if has_messages {
                conversation()
            } else {
                FakeModel::default()
            };
            compare_with(
                case,
                o(&[("value", b(view(model).can_start_new()))]),
                Some(root),
            );
        }
        "chat_view.ChatView.animating" => animating(case, root),
        "chat_view.ChatView.pointer_down" => pointer_down(case, root),
        "chat_view.ChatView.pointer_move" => pointer_move(case, root),
        "chat_view.ChatView.wheel" => wheel(case, root),
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

fn key_case(case: &FixtureCase) {
    let mut view = view(conversation());
    let text = line(case, "text");
    if !text.is_empty() {
        view.input.insert(&text);
        view.input.place_caret(5);
    }
    let action = view.key(
        int(case, "vk") as i32,
        flag(case, "ctrl"),
        flag(case, "shift"),
    );
    let input = o(&[
        ("text", s(view.input.text())),
        ("cursor", i(view.input.cursor() as i64)),
        ("anchor", selection_anchor(&view)),
    ]);
    super::support::compare(
        case,
        o(&[
            ("action", action_json(&action)),
            ("input", input),
            ("mode", s(view.mode.as_str())),
        ]),
    );
}

fn selection_anchor(view: &ChatView) -> String {
    match view.input.selection() {
        Some((start, end)) => {
            let cursor = view.input.cursor();
            let anchor = if cursor == start { end } else { start };
            i(anchor as i64)
        }
        None => "null".into(),
    }
}

fn edit_case(case: &FixtureCase) {
    let mut view = view(FakeModel::default());
    let kind = line(case, "kind");
    let value = line(case, "value");
    if kind == "char" {
        view.char(&value);
    } else {
        view.paste(&value);
    }
    super::support::compare(
        case,
        o(&[(
            "input",
            o(&[
                ("text", s(view.input.text())),
                ("cursor", i(view.input.cursor() as i64)),
                ("anchor", selection_anchor(&view)),
            ]),
        )]),
    );
}

fn add_files(case: &FixtureCase, root: &str) {
    let mut view = view(FakeModel::default());
    let paths: Vec<String> = case
        .input
        .get("paths")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .unwrap_or("")
                        .replace("<tmp>", root)
                        .replace('/', "\\")
                })
                .collect()
        })
        .unwrap_or_default();
    view.add_files(&paths);
    let input = o(&[
        ("text", s(view.input.text())),
        ("cursor", i(view.input.cursor() as i64)),
    ]);
    compare_with(case, o(&[("input", input)]), Some(root));
}

fn animating(case: &FixtureCase, root: &str) {
    let mut model = FakeModel::default();
    let mut flash = false;
    match case.case.rsplit('/').next().unwrap_or("") {
        "view-animating-busy" => model.busy = true,
        "view-animating-confirm" => {
            model.confirm = Some(ConfirmCard {
                summary: "¿Seguro?".into(),
                strong: true,
            });
        }
        "view-animating-flash" => flash = true,
        _ => {}
    }
    let mut view = view(model);
    if flash {
        view.flash_copied("texto", Some(0.0));
    }
    compare_with(
        case,
        o(&[("value", b(view.animating(num(case, "now"))))]),
        Some(root),
    );
}

fn pointer_down(case: &FixtureCase, root: &str) {
    let mut view = view(conversation());
    let kind = line(case, "kind");
    let x = num(case, "x");
    let y = num(case, "y");
    if kind == "send" {
        view.input.insert("Hola");
    }
    view.render(0.0);
    let action = view.pointer_down(x, y);
    compare_with(
        case,
        o(&[
            ("action", action_json(&action)),
            (
                "hover",
                view.hover.as_deref().map_or_else(|| "null".into(), s),
            ),
            (
                "input",
                o(&[
                    ("text", s(view.input.text())),
                    ("cursor", i(view.input.cursor() as i64)),
                    ("anchor", selection_anchor(&view)),
                ]),
            ),
            ("mode", s(view.mode.as_str())),
        ]),
        Some(root),
    );
}

fn pointer_move(case: &FixtureCase, root: &str) {
    let mut view = view(conversation());
    view.render(0.0);
    let changed = view.pointer_move(num(case, "x"), num(case, "y"));
    compare_with(
        case,
        o(&[
            ("changed", b(changed)),
            (
                "hover",
                view.hover.as_deref().map_or_else(|| "null".into(), s),
            ),
        ]),
        Some(root),
    );
}

fn wheel(case: &FixtureCase, root: &str) {
    let mut view = view(conversation());
    view.render(0.0);
    view.scroll.set_offset(num(case, "scroll"));
    view.wheel(int(case, "delta") as i32);
    compare_with(
        case,
        o(&[
            ("scroll", super::support::f(view.scroll.offset())),
            ("max_scroll", super::support::f(view.scroll.max())),
        ]),
        Some(root),
    );
}
