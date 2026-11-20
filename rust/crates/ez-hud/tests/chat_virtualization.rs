//! DX-1c: el chat solo maqueta lo que se ve.
//!
//! La fase A (DX-1b) dejó de pintar el historial entero, pero seguía
//! **maquetando** (midiendo y pintando la tesela) cada mensaje para saber
//! su alto. Con 1 000 mensajes el primer cuadro costaba un minuto.
//!
//! - `the_first_frame_lays_out_only_what_is_on_screen`: con 1 000 mensajes
//!   el primer cuadro maqueta lo visible más el margen, y el número de
//!   maquetas no crece con el historial.
//! - `the_first_frame_does_not_pay_for_the_whole_history`: el primer cuadro
//!   con 1 000 mensajes cuesta lo mismo (mismo orden de magnitud) que con
//!   10.
//! - `scrolling_to_the_top_lays_out_what_comes_in`: al desplazarse hasta
//!   arriba se maqueta lo que entra, y solo eso.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ez_gfx::raster::Image;
use ez_hud::model::chat::view::{
    ChatMessage, ChatModel, ChatView, ConfirmCard, Move, Role, SessionInfo, TextField, ViewHost,
};
use ez_hud::paint::chat::{Block, ChatHost, FileInfo, Paragraph, Span};

/// Model with a fixed history.
struct StoreModel {
    messages: Vec<ChatMessage>,
}

impl ChatModel for StoreModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        (0, self.messages.clone())
    }

    fn version(&self) -> i64 {
        0
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        Vec::new()
    }

    fn busy(&self) -> bool {
        false
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        None
    }

    fn viewing(&self) -> Option<i64> {
        None
    }

    fn current_session(&self) -> Option<i64> {
        None
    }

    fn voice_replies(&self) -> bool {
        false
    }

    fn mic_muted(&self) -> bool {
        false
    }

    fn view_session(&mut self, _: Option<i64>) {}
}

/// Markdown host: one paragraph per line, no files.
struct PlainHost;

impl ChatHost for PlainHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                Block::Paragraph(Paragraph {
                    spans: vec![Span::plain(line)],
                })
            })
            .collect()
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

impl ViewHost for PlainHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        vec![Span::plain(text)]
    }
}

/// Empty input box.
#[derive(Default)]
struct EmptyInput {
    text: String,
    cursor: usize,
}

impl TextField for EmptyInput {
    fn text(&self) -> &str {
        &self.text
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn insert(&mut self, value: &str) {
        self.text.push_str(value);
        self.cursor = self.text.chars().count();
    }

    fn newline(&mut self) {
        self.insert("\n");
    }

    fn take(&mut self) -> String {
        let value = std::mem::take(&mut self.text);
        self.cursor = 0;
        value
    }

    fn move_caret(&mut self, _: Move, _: bool, _: bool) {}

    fn backspace(&mut self, _: bool) {
        self.text.pop();
        self.cursor = 0;
    }

    fn delete(&mut self, _: bool) {}

    fn select_all(&mut self) {}

    fn selected_text(&self) -> String {
        String::new()
    }

    fn selection(&self) -> Option<(usize, usize)> {
        None
    }

    fn clear(&mut self) {
        self.text.clear();
    }

    fn place_caret(&mut self, cursor: usize) {
        self.cursor = cursor;
    }
}

/// A view with `count` short messages.
fn view_for(count: usize) -> ChatView {
    let messages: Vec<ChatMessage> = (0..count)
        .map(|index| {
            ChatMessage::new(
                index as i64 + 1,
                if index % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                },
                format!("mensaje {index}"),
            )
        })
        .collect();
    let mut view = ChatView::new(
        Box::new(StoreModel { messages }),
        Box::new(PlainHost),
        Box::new(EmptyInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        ez_hud::tokens::palette("dark").expect("the dark palette exists"),
    );
    view.set_focus_instant(0.0);
    view
}

/// The first frame lays out the visible messages and the margin around them,
/// not the whole history.
///
/// DECISION: the height of every message is measured (cheap) and the tile is
/// only built for the viewport and a screen above and below (DX-1c). Before,
/// the first frame with 1 000 messages laid out (and painted) all of them:
/// about a minute.
#[test]
fn the_first_frame_lays_out_only_what_is_on_screen() {
    let mut small = view_for(10);
    let mut big = view_for(1000);
    small.render(0.0);
    big.render(0.0);

    let small_layouts = small.layouts();
    let big_layouts = big.layouts();
    assert!(small_layouts > 0, "the first frame laid out nothing at all");
    assert!(
        big_layouts < 100,
        "the first frame laid out {big_layouts} messages of 1 000"
    );
    assert!(
        big_layouts < small_layouts * 4,
        "the number of layouts grew with the history: {small_layouts} with 10 \
         messages and {big_layouts} with 1 000"
    );
}

/// The first frame does not pay for the whole history.
///
/// The height of every message is still measured (the conversation needs the
/// total to scroll), but measuring is about ten times cheaper than
/// painting: the frame with 1 000 messages costs the same order of
/// magnitude as the frame with 10.
#[test]
fn the_first_frame_does_not_pay_for_the_whole_history() {
    let mut small = view_for(10);
    let mut big = view_for(1000);
    let start = Instant::now();
    small.render(0.0);
    let cold_small = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    big.render(0.0);
    let cold_big = start.elapsed().as_secs_f64() * 1000.0;
    assert!(
        cold_big < cold_small.max(1.0) * 20.0,
        "the first frame cost {cold_small:.2} ms with 10 messages and {cold_big:.2} \
         ms with 1 000"
    );
}

/// Scrolling to the top lays out what comes in, and only that.
#[test]
fn scrolling_to_the_top_lays_out_what_comes_in() {
    let mut view = view_for(1000);
    view.render(0.0);
    let before = view.layouts();
    // Up to the first message of the conversation.
    view.set_scroll(view.scroll.max());
    view.render(0.1);
    let after = view.layouts();
    assert!(
        after > before,
        "the messages that came in were not laid out ({before} -> {after})"
    );
    assert!(
        after - before < 100,
        "scrolling laid out {} messages",
        after - before
    );
}
